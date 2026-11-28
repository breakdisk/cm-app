//! Customer ratings folded into `driver_ops.drivers`, against a real database.
//!
//! Needs Postgres with driver-ops' migrations applied (including
//! 0020_driver_rating_events). Skipped locally when DATABASE_URL is unset;
//! fatal in CI, where a missing database means the harness broke and a green
//! pass over nothing would be worse than no test.

use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

use logisticos_driver_ops::infrastructure::messaging::feedback_consumer::{apply_rating, RatingOutcome};

async fn pool() -> Option<PgPool> {
    match std::env::var("DATABASE_URL") {
        Ok(url) => Some(PgPoolOptions::new().max_connections(4).connect(&url).await.expect("connect")),
        Err(_) => {
            assert!(
                std::env::var("CI").is_err(),
                "DATABASE_URL is unset while CI is set: Postgres provisioning failed. \
                 These tests guard which driver a rating lands on; they must not be skipped here."
            );
            eprintln!("skipping: DATABASE_URL not set (this is fatal when CI is set)");
            None
        }
    }
}

async fn seed_driver(pool: &PgPool, tenant: Uuid) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO driver_ops.drivers (id, tenant_id, user_id, first_name, last_name, phone)
         VALUES ($1, $2, $1, 'T', 'D', '+630000000')",
    )
    .bind(id)
    .bind(tenant)
    .execute(pool)
    .await
    .expect("seed driver");
    id
}

async fn rating_of(pool: &PgPool, driver: Uuid) -> (Option<f32>, i32) {
    sqlx::query_as("SELECT rating_avg, rating_count FROM driver_ops.drivers WHERE id = $1")
        .bind(driver)
        .fetch_one(pool)
        .await
        .expect("read driver")
}

#[tokio::test]
async fn a_rating_updates_the_average() {
    let Some(pool) = pool().await else { return };
    let tenant = Uuid::new_v4();
    let driver = seed_driver(&pool, tenant).await;

    for (r, want_avg, want_count) in [(4, 4.0, 1), (5, 4.5, 2)] {
        let out = apply_rating(&pool, tenant, Uuid::new_v4(), Uuid::new_v4(), driver, r).await.expect("apply");
        assert_eq!(out, RatingOutcome::Applied);
        let (avg, count) = rating_of(&pool, driver).await;
        assert!((avg.expect("avg") - want_avg).abs() < 1e-6);
        assert_eq!(count, want_count);
    }
}

#[tokio::test]
async fn a_redelivered_event_does_not_double_count() {
    let Some(pool) = pool().await else { return };
    let tenant = Uuid::new_v4();
    let driver = seed_driver(&pool, tenant).await;
    let (event, shipment) = (Uuid::new_v4(), Uuid::new_v4());

    let first = apply_rating(&pool, tenant, event, shipment, driver, 5).await.expect("apply");
    let again = apply_rating(&pool, tenant, event, shipment, driver, 5).await.expect("apply");
    // Same shipment under a fresh event id is also the same rating.
    let republished = apply_rating(&pool, tenant, Uuid::new_v4(), shipment, driver, 1).await.expect("apply");

    assert_eq!(first, RatingOutcome::Applied);
    assert_eq!(again, RatingOutcome::Duplicate);
    assert_eq!(republished, RatingOutcome::Duplicate);
    assert_eq!(rating_of(&pool, driver).await, (Some(5.0), 1));
}

#[tokio::test]
async fn a_wrong_tenant_updates_nothing() {
    let Some(pool) = pool().await else { return };
    let tenant = Uuid::new_v4();
    let driver = seed_driver(&pool, tenant).await;
    let shipment = Uuid::new_v4();

    let out = apply_rating(&pool, Uuid::new_v4(), Uuid::new_v4(), shipment, driver, 5).await.expect("apply");

    assert_eq!(out, RatingOutcome::UnknownDriver);
    assert_eq!(rating_of(&pool, driver).await, (None, 0));
    // No dedupe row either: the right tenant can still rate this shipment.
    let ok = apply_rating(&pool, tenant, Uuid::new_v4(), shipment, driver, 5).await.expect("apply");
    assert_eq!(ok, RatingOutcome::Applied);
}

#[tokio::test]
async fn an_unknown_driver_is_skipped() {
    let Some(pool) = pool().await else { return };
    let out = apply_rating(&pool, Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), 3)
        .await
        .expect("apply");
    assert_eq!(out, RatingOutcome::UnknownDriver);
}
