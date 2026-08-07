//! Proves ADR-0015's load-bearing invariant against a real database:
//! two products racing for the same courier produce exactly one winner.
//!
//! Requires a running Postgres. Skipped when DATABASE_URL is unset.

use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use logisticos_field_ops::domain::entities::{Courier, CourierAssignment, ProductKey};
use logisticos_field_ops::domain::repositories::CourierRepository;
use logisticos_field_ops::infrastructure::db::{
    AssignmentRepository, ClaimOutcome, PgAssignmentRepository, PgCourierRepository,
};

#[tokio::test]
async fn two_products_racing_for_one_courier_produce_exactly_one_winner() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let pool = PgPoolOptions::new()
        .after_connect(|c, _| Box::pin(async move {
            sqlx::query("SET search_path TO field_ops, public").execute(&mut *c).await?;
            Ok(())
        }))
        .connect(&url)
        .await
        .expect("connect");

    logisticos_common::migrations::run(&pool, "field_ops", &sqlx::migrate!("./migrations"))
        .await
        .expect("migrate");

    let tenant = Uuid::new_v4();
    let couriers = PgCourierRepository::new(pool.clone());
    let assignments = PgAssignmentRepository::new(pool.clone());

    let mut courier = Courier::new(
        tenant, Uuid::new_v4(), "Race".into(), "Test".into(), "+639170000001".into(),
    );
    courier.go_available();
    couriers.save(&courier).await.expect("save courier");

    // Both products offer the same courier a job.
    let a_logistics = CourierAssignment::offer(tenant, courier.id, ProductKey::new("logistics"), Uuid::new_v4());
    let a_omnideliv = CourierAssignment::offer(tenant, courier.id, ProductKey::new("omnideliv"), Uuid::new_v4());
    assignments.save(&a_logistics).await.expect("save A");
    assignments.save(&a_omnideliv).await.expect("save B");

    // Race the claims concurrently.
    let (r1, r2) = tokio::join!(
        assignments.try_claim(tenant, a_logistics.id),
        assignments.try_claim(tenant, a_omnideliv.id),
    );

    let wins = [r1.expect("claim A"), r2.expect("claim B")]
        .iter()
        .filter(|o| **o == ClaimOutcome::Won)
        .count();

    assert_eq!(wins, 1, "exactly one product must win the courier, got {wins}");
}

/// The registry is what makes a third consumer an INSERT rather than a
/// migration. This asserts the FK actually rejects an unregistered key — if it
/// did not, `product` would be free text and the registry decorative.
#[tokio::test]
async fn an_unregistered_product_is_rejected_by_the_foreign_key() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let pool = PgPoolOptions::new()
        .after_connect(|c, _| Box::pin(async move {
            sqlx::query("SET search_path TO field_ops, public").execute(&mut *c).await?;
            Ok(())
        }))
        .connect(&url)
        .await
        .expect("connect");

    logisticos_common::migrations::run(&pool, "field_ops", &sqlx::migrate!("./migrations"))
        .await
        .expect("migrate");

    let tenant = Uuid::new_v4();
    let couriers = PgCourierRepository::new(pool.clone());
    let assignments = PgAssignmentRepository::new(pool.clone());

    let courier = Courier::new(
        tenant, Uuid::new_v4(), "Fk".into(), "Test".into(), "+639170000002".into(),
    );
    couriers.save(&courier).await.expect("save courier");

    let rogue = CourierAssignment::offer(
        tenant, courier.id, ProductKey::new("not_a_registered_product"), Uuid::new_v4());

    assert!(
        assignments.save(&rogue).await.is_err(),
        "an unregistered product key must be rejected by the products FK",
    );

    // And registering it is a plain INSERT — no schema change, which is the
    // whole property ADR-0015 required.
    sqlx::query(
        "INSERT INTO field_ops.products (key, display_name, completion_topic)
         VALUES ('not_a_registered_product', 'Test Product', 'test.assignment.completed')
         ON CONFLICT (key) DO NOTHING",
    )
    .execute(&pool)
    .await
    .expect("register product");

    assignments.save(&rogue).await.expect("the same assignment saves once its product is registered");
}
