//! The idempotency guarantee the courier app's offline queue depends on,
//! proved against a real database rather than an in-memory double.
//!
//! The unit tests assert that `raise_exception` records once per `client_ref`,
//! but they do it against a fake that enforces uniqueness in Rust. The thing
//! that actually has to hold in production is the unique index in migration
//! 0010 — and a fake agreeing with itself proves nothing about that.

use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use logisticos_field_ops::domain::entities::{
    AssignmentException, Courier, CourierAssignment, ExceptionReason, ProductKey,
};
use logisticos_field_ops::domain::repositories::CourierRepository;
use logisticos_field_ops::infrastructure::db::{
    AssignmentRepository, ExceptionRepository, PgAssignmentRepository, PgCourierRepository,
    PgExceptionRepository,
};

/// The database URL, or `None` when it is legitimate to skip.
///
/// Skipping is legitimate on a dev machine with no Postgres. It is never
/// legitimate in CI: the workflow provisions a Postgres service and runs this
/// service's migrations before the test step, so an absent DATABASE_URL there
/// means that provisioning failed. Returning quietly in that case prints a
/// green pass for a test that never executed — which is worse than no test,
/// because it is indistinguishable from a real one.
fn database_url() -> Option<String> {
    match std::env::var("DATABASE_URL") {
        Ok(url) => Some(url),
        Err(_) => {
            assert!(
                std::env::var("CI").is_err(),
                "DATABASE_URL is unset while CI is set — Postgres provisioning \
                 failed. These tests prove that a replayed offline write lands \
                 once; they must not be skipped here."
            );
            eprintln!("skipping: DATABASE_URL not set (this is fatal when CI is set)");
            None
        }
    }
}

/// A pool with migrations applied and one saved courier + claimed assignment,
/// because both are foreign keys on `assignment_exceptions`.
async fn fixture(url: &str) -> (PgExceptionRepository, Uuid, Uuid, Uuid) {
    let pool = PgPoolOptions::new()
        .after_connect(|c, _| {
            Box::pin(async move {
                sqlx::query("SET search_path TO field_ops, public")
                    .execute(&mut *c)
                    .await?;
                Ok(())
            })
        })
        .connect(url)
        .await
        .expect("connect");

    logisticos_common::migrations::run(&pool, "field_ops", &sqlx::migrate!("./migrations"))
        .await
        .expect("migrate");

    let tenant = Uuid::new_v4();
    let couriers = PgCourierRepository::new(pool.clone(), false);
    let assignments = PgAssignmentRepository::new(pool.clone());

    let courier = Courier::new(
        tenant,
        Uuid::new_v4(),
        "Exception".into(),
        "Test".into(),
        "+639170000009".into(),
    );
    couriers.save(&courier).await.expect("save courier");

    let a = CourierAssignment::offer_with_earnings(
        tenant,
        courier.id,
        ProductKey::new("omnideliv"),
        Uuid::new_v4(),
        4_500,
        0,
        9_100,
    );
    assignments.save(&a).await.expect("save assignment");

    (
        PgExceptionRepository::new(pool),
        tenant,
        a.id,
        courier.id,
    )
}

#[tokio::test]
async fn a_replayed_client_ref_inserts_exactly_once() {
    let Some(url) = database_url() else { return };
    let (exceptions, tenant, assignment_id, courier_id) = fixture(&url).await;

    let client_ref = Uuid::new_v4();
    let mut inserted = 0;
    for _ in 0..3 {
        // A fresh entity each time, exactly as a replay produces: a new id and
        // a new server_timestamp, carrying the same client_ref. If the index
        // keyed on `id` instead, all three would land.
        let e = AssignmentException::new(
            tenant,
            assignment_id,
            courier_id,
            ExceptionReason::CannotPay,
            Some("no cash at the door".into()),
            None,
            Some((14.5995, 120.9842)),
            client_ref,
            None,
        );
        if exceptions.record(&e).await.expect("record") {
            inserted += 1;
        }
    }

    assert_eq!(inserted, 1, "a replayed tap is one exception");

    let open = exceptions.list_open(tenant, 50).await.expect("list");
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].reason, ExceptionReason::CannotPay);
    assert_eq!(open[0].note.as_deref(), Some("no cash at the door"));
    assert_eq!(open[0].capture_lat, Some(14.5995));
    assert!(open[0].resolved_at.is_none(), "a new exception is open");
}

/// The index keys on `(assignment_id, client_ref)`, not on the assignment
/// alone: a courier who fails, is re-dispatched, and fails again has two real
/// failures to answer for.
#[tokio::test]
async fn two_distinct_failures_on_one_assignment_are_both_kept() {
    let Some(url) = database_url() else { return };
    let (exceptions, tenant, assignment_id, courier_id) = fixture(&url).await;

    for reason in [
        ExceptionReason::CustomerUnreachable,
        ExceptionReason::AddressUnreachable,
    ] {
        let e = AssignmentException::new(
            tenant,
            assignment_id,
            courier_id,
            reason,
            None,
            None,
            None,
            Uuid::new_v4(),
            None,
        );
        assert!(exceptions.record(&e).await.expect("record"));
    }

    assert_eq!(exceptions.list_open(tenant, 50).await.expect("list").len(), 2);
}

/// The ops queue is per tenant. A missing filter here would show one tenant's
/// couriers and customer notes to another.
#[tokio::test]
async fn the_open_queue_does_not_leak_across_tenants() {
    let Some(url) = database_url() else { return };
    let (exceptions, tenant, assignment_id, courier_id) = fixture(&url).await;

    let e = AssignmentException::new(
        tenant,
        assignment_id,
        courier_id,
        ExceptionReason::GoodsDamaged,
        None,
        None,
        None,
        Uuid::new_v4(),
        None,
    );
    exceptions.record(&e).await.expect("record");

    let other_tenant = Uuid::new_v4();
    assert!(
        exceptions.list_open(other_tenant, 50).await.expect("list").is_empty(),
        "another tenant must not see this exception"
    );
}
