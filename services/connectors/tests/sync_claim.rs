//! Who gets to run an unattended catalog sync, and exactly once.
//!
//! The platform runs rolling updates, so two connectors replicas is the normal
//! case. If both sweeps claim the same connector, that vendor's shop is fetched
//! twice and their catalog written twice in the same second — the cost lands on
//! *their* server, not ours, which is the kind of bug that gets a merchant's
//! API credentials revoked rather than filed.
//!
//! So the claim has to be atomic, and the only honest way to show that is two
//! concurrent claims against a real Postgres. A fake cannot express
//! `FOR UPDATE SKIP LOCKED`.
//!
//! Requires a running Postgres: skipped locally when DATABASE_URL is unset,
//! fatal in CI where a missing database means the harness broke rather than
//! that there is nothing to test.

use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use logisticos_connectors::domain::repositories::CredentialsRepository;
use logisticos_connectors::infrastructure::db::PgCredentialsRepository;

fn database_url() -> Option<String> {
    match std::env::var("DATABASE_URL") {
        Ok(url) => Some(url),
        Err(_) => {
            assert!(
                std::env::var("CI").is_err(),
                "DATABASE_URL is unset while CI is set — Postgres provisioning failed."
            );
            eprintln!("skipping: DATABASE_URL not set (this is fatal when CI is set)");
            None
        }
    }
}

async fn pool(url: &str) -> PgPool {
    let pool = PgPoolOptions::new()
        .max_connections(6)
        .after_connect(|c, _| {
            Box::pin(async move {
                sqlx::query("SET search_path TO connectors, public")
                    .execute(&mut *c)
                    .await?;
                Ok(())
            })
        })
        .connect(url)
        .await
        .expect("connect");

    logisticos_common::migrations::run(&pool, "connectors", &sqlx::migrate!("./migrations"))
        .await
        .expect("migrations must apply cleanly");

    pool
}

/// A connector with auto-sync enabled and never yet run — i.e. due now.
async fn seed(pool: &PgPool, interval: Option<i32>, last_synced_mins_ago: Option<i64>) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO connectors.credentials
            (id, tenant_id, merchant_id, tenant_slug, platform, webhook_secret,
             config, is_active, sync_interval_mins, last_synced_at)
        VALUES ($1, $2, $3, 'demo', $4, 'secret',
                '{"omnideliv_vendor_id":"00000000-0000-0000-0000-000000000009"}'::jsonb,
                true, $5,
                CASE WHEN $6::BIGINT IS NULL THEN NULL
                     ELSE NOW() - ($6::BIGINT || ' minutes')::interval END)
        "#,
    )
    .bind(id)
    .bind(Uuid::new_v4())
    .bind(Uuid::new_v4())
    // Unique per row: the table has UNIQUE (tenant_id, platform), and every
    // seed here uses a fresh tenant, so the platform value only has to be one
    // of the two the CHECK-free column accepts.
    .bind(if rand_bool(id) { "shopify" } else { "woocommerce" })
    .bind(interval)
    .bind(last_synced_mins_ago)
    .execute(pool)
    .await
    .expect("seed");
    id
}

/// Deterministic per-id, so a failure reproduces.
fn rand_bool(id: Uuid) -> bool {
    id.as_bytes()[0].is_multiple_of(2)
}


/// One test, six scenarios, run in order.
///
/// `claim_due_syncs` is global across tenants on purpose — an unattended sweep
/// that only ran for tenants someone enumerated would silently miss the rest.
/// That makes the table a resource every scenario shares, and cargo runs tests
/// in a binary **in parallel**: written as six `#[tokio::test]`s they stole each
/// other's rows and failed for reasons that had nothing to do with the claim.
/// Sequencing them here is cheaper and clearer than a `serial_test` dependency.
#[tokio::test]
async fn the_sync_claim_behaves() {
    let Some(url) = database_url() else { return };
    let pool = pool(&url).await;

    auto_sync_is_off_unless_an_interval_is_set(&pool).await;
    only_connectors_past_their_interval_are_claimed(&pool).await;
    a_claimed_connector_is_not_due_again_until_its_interval_elapses(&pool).await;
    the_last_error_is_recorded_and_then_cleared(&pool).await;
    an_interval_below_the_floor_is_refused_by_the_database(&pool).await;
    // Last: its drain loop takes every due row in the table.
    two_replicas_never_claim_the_same_connector(&pool).await;
}

/// The property the whole design rests on: two sweeps running at the same
/// instant must not both take the same connector.
async fn two_replicas_never_claim_the_same_connector(pool: &PgPool) {
    let repo_a = PgCredentialsRepository::new(pool.clone());
    let repo_b = PgCredentialsRepository::new(pool.clone());

    let mut ids = Vec::new();
    for _ in 0..8 {
        ids.push(seed(pool, Some(60), None).await);
    }

    // A deterministic race, not a hopeful one.
    //
    // `tokio::join!` on two claims does NOT reproduce this: both queries finish
    // in microseconds and the window never opens. Written that way the test
    // passed with `FOR UPDATE SKIP LOCKED` *removed* — a green test that proved
    // nothing, which is the failure mode this codebase keeps paying for.
    //
    // So hold the lock explicitly. An open transaction that has selected these
    // rows FOR UPDATE is exactly the state a mid-sweep replica is in.
    let mut held = pool.begin().await.expect("begin");
    let locked: Vec<Uuid> = sqlx::query_scalar(
        r#"SELECT id FROM connectors.credentials
            WHERE id = ANY($1) FOR UPDATE"#,
    )
    .bind(&ids)
    .fetch_all(&mut *held)
    .await
    .expect("lock rows");
    assert_eq!(locked.len(), ids.len(), "the fixture rows should all be lockable");

    // The other replica sweeps while those rows are held. With SKIP LOCKED it
    // steps over them and returns promptly; without it, it would block on the
    // open transaction until the timeout, or take rows already being synced.
    let concurrent = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        repo_b.claim_due_syncs(8),
    )
    .await
    .expect("a sweep must not block on rows another replica is already syncing")
    .expect("claim");

    for c in &concurrent {
        assert!(
            !ids.contains(&c.id),
            "claimed connector {} while another replica held it — that vendor's shop              would be fetched twice at once",
            c.id,
        );
    }

    drop(held); // rolls back; the rows become claimable again

    // Now drain them for real and confirm none is handed out twice.
    let mut seen: Vec<Uuid> = Vec::new();
    for _ in 0..12 {
        let batch = repo_a.claim_due_syncs(8).await.expect("claim");
        if batch.is_empty() {
            break;
        }
        seen.extend(batch.iter().map(|c| c.id));
    }
    let mut deduped = seen.clone();
    deduped.sort();
    deduped.dedup();
    assert_eq!(deduped.len(), seen.len(), "a connector was claimed twice");

    // And nothing starved. Asked of the database rather than of `seen`,
    // because a row taken by a different sweep is still claimed — what matters
    // is that no row stays perpetually due.
    let still_due: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM connectors.credentials
            WHERE id = ANY($1)
              AND sync_interval_mins IS NOT NULL
              AND (last_synced_at IS NULL
                   OR last_synced_at < NOW() - (sync_interval_mins || ' minutes')::interval)"#,
    )
    .bind(&ids)
    .fetch_one(pool)
    .await
    .expect("due count");

    assert_eq!(still_due, 0, "a due connector was never claimed by any sweep");
}

/// Claiming stamps `last_synced_at`, so the next sweep inside the interval
/// finds nothing. Without this a 60-second tick would sync hourly-configured
/// vendors sixty times an hour.
async fn a_claimed_connector_is_not_due_again_until_its_interval_elapses(pool: &PgPool) {
    let repo = PgCredentialsRepository::new(pool.clone());

    let id = seed(pool, Some(60), None).await;

    let first: Vec<Uuid> = repo.claim_due_syncs(50).await.unwrap().iter().map(|c| c.id).collect();
    assert!(first.contains(&id), "a never-synced connector is due");

    let second: Vec<Uuid> = repo.claim_due_syncs(50).await.unwrap().iter().map(|c| c.id).collect();
    assert!(!second.contains(&id), "it must not be due again immediately after being claimed");
}

/// The opt-in. A connector with no interval is never swept, however long it has
/// been — nightly-overwriting a catalog nobody asked to have overwritten is the
/// failure mode this column exists to prevent.
async fn auto_sync_is_off_unless_an_interval_is_set(pool: &PgPool) {
    let repo = PgCredentialsRepository::new(pool.clone());

    let off = seed(pool, None, None).await;

    let claimed: Vec<Uuid> = repo.claim_due_syncs(50).await.unwrap().iter().map(|c| c.id).collect();
    assert!(!claimed.contains(&off), "a connector with no interval must never be swept");
}

/// Overdue rows are claimed; rows inside their window are not.
async fn only_connectors_past_their_interval_are_claimed(pool: &PgPool) {
    let repo = PgCredentialsRepository::new(pool.clone());

    let overdue = seed(pool, Some(60), Some(90)).await;  // 90m ago, interval 60m
    let fresh   = seed(pool, Some(60), Some(10)).await;  // 10m ago, interval 60m

    let claimed: Vec<Uuid> = repo.claim_due_syncs(50).await.unwrap().iter().map(|c| c.id).collect();

    assert!(claimed.contains(&overdue), "90 minutes past a 60 minute interval is due");
    assert!(!claimed.contains(&fresh), "10 minutes into a 60 minute interval is not");
}

/// A failing sync has to be visible without reading logs, and a fixed one has
/// to stop looking broken.
async fn the_last_error_is_recorded_and_then_cleared(pool: &PgPool) {
    let repo = PgCredentialsRepository::new(pool.clone());

    let id = seed(pool, Some(60), None).await;

    repo.record_sync_result(id, Some("shop_domain not configured")).await.unwrap();
    let err: Option<String> = sqlx::query("SELECT last_sync_error FROM connectors.credentials WHERE id = $1")
        .bind(id).fetch_one(pool).await.unwrap().get("last_sync_error");
    assert_eq!(err.as_deref(), Some("shop_domain not configured"));

    repo.record_sync_result(id, None).await.unwrap();
    let cleared: Option<String> = sqlx::query("SELECT last_sync_error FROM connectors.credentials WHERE id = $1")
        .bind(id).fetch_one(pool).await.unwrap().get("last_sync_error");
    assert!(cleared.is_none(), "a stale error must not outlive the fault it described");
}

/// The floor. Below 15 minutes a sweep is hammering someone else's server.
async fn an_interval_below_the_floor_is_refused_by_the_database(pool: &PgPool) {
    let res = sqlx::query(
        r#"INSERT INTO connectors.credentials
               (tenant_id, merchant_id, tenant_slug, platform, webhook_secret, sync_interval_mins)
           VALUES ($1, $2, 'demo', 'shopify', 's', 5)"#,
    )
    .bind(Uuid::new_v4())
    .bind(Uuid::new_v4())
    .execute(pool)
    .await;

    assert!(res.is_err(), "a 5 minute interval must be refused, not merely discouraged");
}
