//! Proves `uq_courier_ledger_entry_job` against a real database: a delivery
//! credited in one ISO week cannot be credited again in the next.
//!
//! This is the spec's V1, and it is deliberately written **at the index**
//! rather than at the application guard. `DispatchService::credit_courier`
//! decides which ledger to look in by calling `current_period()`, which reads
//! `Utc::now()` with no clock seam — so a test of the guard would either need a
//! real week to elapse or would silently never cross the boundary and pass for
//! the wrong reason. The index has no clock, so it is testable today.
//!
//! It is also the layer that actually has to hold. The guard covers the one
//! path its author knew about; the index covers every path anyone writes later.
//!
//! Requires a running Postgres. Skipped locally when DATABASE_URL is unset;
//! fatal in CI, where a missing database means the harness broke.

use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use logisticos_field_ops::domain::entities::{Courier, CourierLedger};
use logisticos_field_ops::domain::repositories::CourierRepository;
use logisticos_field_ops::infrastructure::db::{
    CourierLedgerRepository, PgCourierLedgerRepository, PgCourierRepository,
};

/// The database URL, or `None` when it is legitimate to skip.
///
/// Skipping is legitimate on a dev machine with no Postgres. It is never
/// legitimate in CI: the workflow provisions Postgres and runs this service's
/// migrations before the test step, so an absent DATABASE_URL there means that
/// provisioning failed. Returning quietly would print a green pass for a test
/// that never executed — indistinguishable from a real one, and this test
/// guards money.
fn database_url() -> Option<String> {
    match std::env::var("DATABASE_URL") {
        Ok(url) => Some(url),
        Err(_) => {
            assert!(
                std::env::var("CI").is_err(),
                "DATABASE_URL is unset while CI is set — Postgres provisioning \
                 failed. This test proves a courier cannot be paid twice for one \
                 job across a week boundary; it must not be skipped here."
            );
            eprintln!("skipping: DATABASE_URL not set (this is fatal when CI is set)");
            None
        }
    }
}

async fn pool() -> sqlx::PgPool {
    let url = database_url().expect("checked by the caller");
    let pool = PgPoolOptions::new()
        .after_connect(|c, _| {
            Box::pin(async move {
                sqlx::query("SET search_path TO field_ops, public").execute(&mut *c).await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .expect("connect");

    logisticos_common::migrations::run(&pool, "field_ops", &sqlx::migrate!("./migrations"))
        .await
        .expect("migrate");
    pool
}

/// The bug, reproduced exactly: the delivery's response was lost, the retry
/// arrives after the Sunday→Monday UTC boundary, and a *fresh* ledger is open.
/// The application guard that scanned only the current period found nothing.
#[tokio::test]
async fn a_job_credited_in_one_week_cannot_be_credited_again_in_the_next() {
    let Some(_) = database_url() else { return };
    let pool = pool().await;

    let tenant = Uuid::new_v4();
    let couriers = PgCourierRepository::new(pool.clone());
    let ledgers = PgCourierLedgerRepository::new(pool.clone());

    let courier = Courier::new(
        tenant, Uuid::new_v4(), "Period".into(), "Boundary".into(), "+639170000002".into(),
    );
    couriers.save(&courier).await.expect("save courier");

    let job = Uuid::new_v4();

    // Week one: the delivery lands and the courier is paid.
    let mut week33 = CourierLedger::open(tenant, courier.id, "2026-W33".into());
    week33.credit_trip(3_500, 2, job);
    ledgers.save(&week33).await.expect("first credit must succeed");

    // Week two: the same job, a new ledger — which is precisely what made the
    // old application guard blind.
    let mut week34 = CourierLedger::open(tenant, courier.id, "2026-W34".into());
    week34.credit_trip(3_500, 2, job);
    let second = ledgers.save(&week34).await;

    let err = second.expect_err(
        "the unique index must refuse a second credit for the same job, whatever \
         period the retry lands in",
    );
    let text = format!("{err:#}");
    assert!(
        text.contains("uq_courier_ledger_entry_job"),
        "refused, but not by the index this test exists to prove — got: {text}",
    );

    // The money is where it was. A rejected retry must not have moved anything.
    let stored = ledgers
        .find_open(tenant, courier.id, "2026-W33")
        .await
        .expect("read week 33")
        .expect("week 33 ledger");
    assert_eq!(
        stored.recomputed_balance_cents(),
        3_500,
        "the first week's balance must be untouched by the refused retry",
    );

    let entries: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM field_ops.courier_ledger_entries \
          WHERE tenant_id = $1 AND courier_id = $2 AND external_ref = $3",
    )
    .bind(tenant)
    .bind(courier.id)
    .bind(job)
    .fetch_one(&pool)
    .await
    .expect("count entries");
    assert_eq!(entries, 1, "exactly one entry may exist for one job, ever");
}

/// The index must not be so broad that ordinary work trips it. Two different
/// jobs in the same week are the normal case, and a courier who cannot be paid
/// for their second delivery is a worse bug than the one being prevented.
#[tokio::test]
async fn two_different_jobs_in_one_week_are_both_credited() {
    let Some(_) = database_url() else { return };
    let pool = pool().await;

    let tenant = Uuid::new_v4();
    let couriers = PgCourierRepository::new(pool.clone());
    let ledgers = PgCourierLedgerRepository::new(pool.clone());

    let courier = Courier::new(
        tenant, Uuid::new_v4(), "Two".into(), "Jobs".into(), "+639170000003".into(),
    );
    couriers.save(&courier).await.expect("save courier");

    let mut week = CourierLedger::open(tenant, courier.id, "2026-W35".into());
    week.credit_trip(3_500, 2, Uuid::new_v4());
    week.credit_trip(4_100, 1, Uuid::new_v4());
    ledgers.save(&week).await.expect("two distinct jobs must both be credited");

    let stored = ledgers
        .find_open(tenant, courier.id, "2026-W35")
        .await
        .expect("read")
        .expect("ledger");
    assert_eq!(stored.recomputed_balance_cents(), 7_600);
}
