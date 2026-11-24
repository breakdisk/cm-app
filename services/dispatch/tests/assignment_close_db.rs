//! Proves, against a real database, that finished work frees the rider:
//! an assignment closes only when its route has tasks, none are open and
//! every route stop has a task; replays and races change nothing; and
//! cancelling a route frees its rider.
//!
//! Requires Postgres with PostGIS. Skipped locally when DATABASE_URL is unset;
//! fatal in CI, where a missing database means the harness broke. Each test
//! creates (and drops) its own scratch database, so the tests are independent.

use std::sync::Arc;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{ConnectOptions, PgPool};
use uuid::Uuid;

use logisticos_dispatch::application::services::DriverAssignmentService;
use logisticos_dispatch::domain::repositories::{DriverAssignmentRepository, RouteRepository};
use logisticos_dispatch::infrastructure::db::{
    DispatchQueueRepository, PgDispatchQueueRepository, PgDriverAssignmentRepository,
    PgDriverAvailabilityRepository, PgRouteRepository,
};
use logisticos_dispatch::infrastructure::messaging::task_outcome_consumer::handle_task_outcome;
use logisticos_events::producer::KafkaProducer;
use logisticos_types::{RouteId, TenantId};

/// The database URL, or `None` when it is legitimate to skip. Never legitimate
/// in CI: an unset DATABASE_URL there means provisioning failed, and a quiet
/// return would print a green pass for a test that never executed.
fn database_url() -> Option<String> {
    match std::env::var("DATABASE_URL") {
        Ok(url) => Some(url),
        Err(_) => {
            assert!(
                std::env::var("CI").is_err(),
                "DATABASE_URL is unset while CI is set — Postgres provisioning failed. \
                 These tests prove a rider is freed when work ends; they must not be skipped here."
            );
            eprintln!("skipping: DATABASE_URL not set (this is fatal when CI is set)");
            None
        }
    }
}

static MIGRATE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct Scratch {
    admin: PgPool,
    name: String,
    pool: PgPool,
}

impl Scratch {
    async fn create(url: &str) -> Self {
        let base: PgConnectOptions = url.parse().expect("DATABASE_URL parses");
        let admin = PgPoolOptions::new()
            .max_connections(1)
            .connect_with(base.clone())
            .await
            .expect("connect admin");
        let name = format!("dispatch_close_{}", Uuid::new_v4().simple());
        sqlx::query(&format!(r#"CREATE DATABASE "{name}""#))
            .execute(&admin)
            .await
            .expect("create scratch database");

        let opts = base.database(&name).disable_statement_logging();
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .after_connect(|c, _| {
                Box::pin(async move {
                    // Same search_path the service uses.
                    sqlx::query("SET search_path TO dispatch, driver_ops, public")
                        .execute(&mut *c)
                        .await?;
                    Ok(())
                })
            })
            .connect_with(opts)
            .await
            .expect("connect scratch");

        sqlx::query(r#"CREATE EXTENSION IF NOT EXISTS "uuid-ossp""#).execute(&pool).await.expect("uuid-ossp");
        sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis").execute(&pool).await.expect("postgis");
        sqlx::query("CREATE SCHEMA IF NOT EXISTS dispatch").execute(&pool).await.expect("schema");
        // Migration 0009 runs ALTER ROLE, which is cluster-global: parallel
        // scratch databases race on the role row ("tuple concurrently updated").
        let _serial = MIGRATE_LOCK.lock().await;
        logisticos_common::migrations::run(&pool, "dispatch", &sqlx::migrate!("./migrations"))
            .await
            .expect("migrate dispatch");

        // CI migrates only dispatch's own schema, so driver-ops' table is a
        // minimal fixture with just the columns the queries read.
        sqlx::query("CREATE SCHEMA IF NOT EXISTS driver_ops").execute(&pool).await.expect("driver_ops");
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS driver_ops.tasks (
                id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                route_id    UUID NOT NULL,
                shipment_id UUID NOT NULL,
                status      TEXT NOT NULL DEFAULT 'pending'
             )",
        )
        .execute(&pool)
        .await
        .expect("tasks fixture");

        Self { admin, name, pool }
    }

    async fn drop_db(self) {
        self.pool.close().await;
        let _ = sqlx::query(&format!(r#"DROP DATABASE IF EXISTS "{}" WITH (FORCE)"#, self.name))
            .execute(&self.admin)
            .await;
    }
}

struct Fixture {
    tenant: Uuid,
    driver: Uuid,
    route: Uuid,
}

async fn route_with_assignment(pool: &PgPool, route_status: &str, assignment_status: &str) -> Fixture {
    let f = Fixture { tenant: Uuid::new_v4(), driver: Uuid::new_v4(), route: Uuid::new_v4() };
    sqlx::query(
        "INSERT INTO dispatch.routes (id, tenant_id, driver_id, vehicle_id, status)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(f.route)
    .bind(f.tenant)
    .bind(f.driver)
    .bind(Uuid::new_v4())
    .bind(route_status)
    .execute(pool)
    .await
    .expect("route");
    sqlx::query(
        "INSERT INTO dispatch.driver_assignments (tenant_id, driver_id, route_id, status)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(f.tenant)
    .bind(f.driver)
    .bind(f.route)
    .bind(assignment_status)
    .execute(pool)
    .await
    .expect("assignment");
    f
}

async fn stop(pool: &PgPool, f: &Fixture, shipment: Uuid, seq: i32) {
    sqlx::query(
        "INSERT INTO dispatch.route_stops
            (route_id, tenant_id, shipment_id, sequence, stop_type,
             address_line1, address_city, address_province)
         VALUES ($1, $2, $3, $4, 'delivery', 'a', 'b', 'c')",
    )
    .bind(f.route)
    .bind(f.tenant)
    .bind(shipment)
    .bind(seq)
    .execute(pool)
    .await
    .expect("stop");
}

async fn task(pool: &PgPool, f: &Fixture, shipment: Uuid, status: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO driver_ops.tasks (route_id, shipment_id, status)
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(f.route)
    .bind(shipment)
    .bind(status)
    .fetch_one(pool)
    .await
    .expect("task")
}

async fn assignment_status(pool: &PgPool, f: &Fixture) -> String {
    sqlx::query_scalar("SELECT status FROM dispatch.driver_assignments WHERE route_id = $1")
        .bind(f.route)
        .fetch_one(pool)
        .await
        .expect("assignment status")
}

async fn route_status(pool: &PgPool, f: &Fixture) -> String {
    sqlx::query_scalar("SELECT status FROM dispatch.routes WHERE id = $1")
        .bind(f.route)
        .fetch_one(pool)
        .await
        .expect("route status")
}

fn outcome_payload(tenant: Uuid, task_id: Uuid, shipment: Uuid) -> Vec<u8> {
    // driver-ops' TaskCompleted shape, trimmed; the consumer reads leniently.
    serde_json::to_vec(&serde_json::json!({
        "id": Uuid::new_v4(),
        "source": "driver-ops",
        "event_type": "delivery.completed",
        "time": chrono::Utc::now(),
        "tenant_id": tenant,
        "data": {
            "task_id": task_id,
            "driver_id": Uuid::new_v4(),
            "shipment_id": shipment,
            "tenant_id": tenant,
        }
    }))
    .expect("payload")
}

#[tokio::test]
async fn last_task_done_closes_assignment_and_route() {
    let Some(url) = database_url() else { return };
    let db = Scratch::create(&url).await;
    let repo = PgDriverAssignmentRepository::new(db.pool.clone());

    let f = route_with_assignment(&db.pool, "in_progress", "accepted").await;
    let (s1, s2) = (Uuid::new_v4(), Uuid::new_v4());
    stop(&db.pool, &f, s1, 1).await;
    stop(&db.pool, &f, s2, 2).await;
    let t1 = task(&db.pool, &f, s1, "completed").await;
    task(&db.pool, &f, s2, "completed").await;

    // Resolved from the event's task id, the way the consumer does it.
    let resolved = repo.find_route_for_outcome(f.tenant, Some(t1), None).await.expect("resolve");
    assert_eq!(resolved, Some(f.route));
    assert!(repo.close_if_finished(f.tenant, f.route).await.expect("close"));

    assert_eq!(assignment_status(&db.pool, &f).await, "completed");
    assert_eq!(route_status(&db.pool, &f).await, "completed");
    db.drop_db().await;
}

#[tokio::test]
async fn open_task_keeps_it_active() {
    let Some(url) = database_url() else { return };
    let db = Scratch::create(&url).await;
    let repo = PgDriverAssignmentRepository::new(db.pool.clone());

    let f = route_with_assignment(&db.pool, "in_progress", "accepted").await;
    let (s1, s2) = (Uuid::new_v4(), Uuid::new_v4());
    stop(&db.pool, &f, s1, 1).await;
    stop(&db.pool, &f, s2, 2).await;
    task(&db.pool, &f, s1, "completed").await;
    task(&db.pool, &f, s2, "in_progress").await;

    assert!(!repo.close_if_finished(f.tenant, f.route).await.expect("close"));
    assert_eq!(assignment_status(&db.pool, &f).await, "accepted");
    assert_eq!(route_status(&db.pool, &f).await, "in_progress");
    db.drop_db().await;
}

#[tokio::test]
async fn accepted_with_no_tasks_yet_stays_active() {
    let Some(url) = database_url() else { return };
    let db = Scratch::create(&url).await;
    let repo = PgDriverAssignmentRepository::new(db.pool.clone());

    // A quick-dispatch / gig-claim route: no route stops, tasks not written yet.
    let f = route_with_assignment(&db.pool, "in_progress", "accepted").await;

    assert!(!repo.close_if_finished(f.tenant, f.route).await.expect("close"));
    assert_eq!(repo.close_finished_assignments(100).await.expect("sweep"), 0);
    assert_eq!(assignment_status(&db.pool, &f).await, "accepted");
    assert_eq!(route_status(&db.pool, &f).await, "in_progress");
    db.drop_db().await;
}

#[tokio::test]
async fn partially_arrived_tasks_stay_active() {
    let Some(url) = database_url() else { return };
    let db = Scratch::create(&url).await;
    let repo = PgDriverAssignmentRepository::new(db.pool.clone());

    // Three stops, only one task has reached driver-ops, and it is finished.
    let f = route_with_assignment(&db.pool, "in_progress", "accepted").await;
    let shipments = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
    for (i, s) in shipments.iter().enumerate() {
        stop(&db.pool, &f, *s, i as i32 + 1).await;
    }
    task(&db.pool, &f, shipments[0], "completed").await;

    assert!(!repo.close_if_finished(f.tenant, f.route).await.expect("close"));
    assert_eq!(assignment_status(&db.pool, &f).await, "accepted");
    assert_eq!(route_status(&db.pool, &f).await, "in_progress");
    db.drop_db().await;
}

#[tokio::test]
async fn failed_and_cancelled_count_as_finished() {
    let Some(url) = database_url() else { return };
    let db = Scratch::create(&url).await;
    let repo = PgDriverAssignmentRepository::new(db.pool.clone());

    let f = route_with_assignment(&db.pool, "in_progress", "accepted").await;
    let shipments = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
    for (i, s) in shipments.iter().enumerate() {
        stop(&db.pool, &f, *s, i as i32 + 1).await;
    }
    task(&db.pool, &f, shipments[0], "failed").await;
    task(&db.pool, &f, shipments[1], "cancelled").await;
    task(&db.pool, &f, shipments[2], "skipped").await;

    assert!(repo.close_if_finished(f.tenant, f.route).await.expect("close"));
    assert_eq!(assignment_status(&db.pool, &f).await, "completed");
    assert_eq!(route_status(&db.pool, &f).await, "completed");
    db.drop_db().await;
}

#[tokio::test]
async fn replayed_outcome_changes_nothing() {
    let Some(url) = database_url() else { return };
    let db = Scratch::create(&url).await;
    let repo = PgDriverAssignmentRepository::new(db.pool.clone());

    let f = route_with_assignment(&db.pool, "in_progress", "accepted").await;
    let s1 = Uuid::new_v4();
    stop(&db.pool, &f, s1, 1).await;
    let t1 = task(&db.pool, &f, s1, "completed").await;
    let payload = outcome_payload(f.tenant, t1, s1);

    assert!(handle_task_outcome(&payload, &repo, true).await.expect("first"));
    let completed_at: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT completed_at FROM dispatch.routes WHERE id = $1")
            .bind(f.route)
            .fetch_one(&db.pool)
            .await
            .expect("completed_at");

    // Replay, and two replicas racing on the same outcome: nothing changes.
    assert!(!handle_task_outcome(&payload, &repo, true).await.expect("replay"));
    let (a, b) = tokio::join!(
        handle_task_outcome(&payload, &repo, true),
        handle_task_outcome(&payload, &repo, true),
    );
    assert!(!a.expect("race a") && !b.expect("race b"));

    assert_eq!(assignment_status(&db.pool, &f).await, "completed");
    let after: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT completed_at FROM dispatch.routes WHERE id = $1")
            .bind(f.route)
            .fetch_one(&db.pool)
            .await
            .expect("completed_at");
    assert_eq!(completed_at, after, "a replay must not touch the route");
    db.drop_db().await;
}

#[tokio::test]
async fn two_replicas_racing_close_exactly_once() {
    let Some(url) = database_url() else { return };
    let db = Scratch::create(&url).await;
    let repo = Arc::new(PgDriverAssignmentRepository::new(db.pool.clone()));

    let f = route_with_assignment(&db.pool, "in_progress", "accepted").await;
    let s1 = Uuid::new_v4();
    stop(&db.pool, &f, s1, 1).await;
    task(&db.pool, &f, s1, "completed").await;

    let (a, b) = tokio::join!(
        { let r = Arc::clone(&repo); async move { r.close_if_finished(f.tenant, f.route).await } },
        { let r = Arc::clone(&repo); async move { r.close_if_finished(f.tenant, f.route).await } },
    );
    let wins = [a.expect("a"), b.expect("b")].iter().filter(|x| **x).count();
    assert_eq!(wins, 1, "exactly one replica closes the route");
    db.drop_db().await;
}

#[tokio::test]
async fn rider_can_be_assigned_again_after_close() {
    let Some(url) = database_url() else { return };
    let db = Scratch::create(&url).await;
    let repo = PgDriverAssignmentRepository::new(db.pool.clone());

    let f = route_with_assignment(&db.pool, "in_progress", "accepted").await;
    let s1 = Uuid::new_v4();
    stop(&db.pool, &f, s1, 1).await;
    task(&db.pool, &f, s1, "completed").await;

    // Busy: the unique partial index refuses a second active assignment.
    let second_route = Uuid::new_v4();
    sqlx::query("INSERT INTO dispatch.routes (id, tenant_id, driver_id, vehicle_id) VALUES ($1,$2,$3,$4)")
        .bind(second_route).bind(f.tenant).bind(f.driver).bind(Uuid::new_v4())
        .execute(&db.pool).await.expect("second route");
    let blocked = sqlx::query(
        "INSERT INTO dispatch.driver_assignments (tenant_id, driver_id, route_id, status) VALUES ($1,$2,$3,'pending')",
    )
    .bind(f.tenant).bind(f.driver).bind(second_route)
    .execute(&db.pool).await;
    assert!(blocked.is_err(), "rider with an accepted assignment must be busy");

    assert!(repo.close_if_finished(f.tenant, f.route).await.expect("close"));

    sqlx::query(
        "INSERT INTO dispatch.driver_assignments (tenant_id, driver_id, route_id, status) VALUES ($1,$2,$3,'pending')",
    )
    .bind(f.tenant).bind(f.driver).bind(second_route)
    .execute(&db.pool).await.expect("rider is free after close");
    db.drop_db().await;
}

#[tokio::test]
async fn cancel_route_closes_its_assignment() {
    let Some(url) = database_url() else { return };
    let db = Scratch::create(&url).await;

    let f = route_with_assignment(&db.pool, "in_progress", "accepted").await;
    // An open task: cancel must free the rider regardless of task state.
    let s1 = Uuid::new_v4();
    stop(&db.pool, &f, s1, 1).await;
    task(&db.pool, &f, s1, "pending").await;

    let route_repo = Arc::new(PgRouteRepository::new(db.pool.clone()));
    let service = DriverAssignmentService::new(
        route_repo as Arc<dyn RouteRepository>,
        Arc::new(PgDriverAssignmentRepository::new(db.pool.clone())),
        Arc::new(PgDriverAvailabilityRepository::new(db.pool.clone())),
        Arc::new(KafkaProducer::new("localhost:9092").expect("producer")),
        None,
        Arc::new(PgDispatchQueueRepository::new(db.pool.clone())) as Arc<dyn DispatchQueueRepository>,
    );

    let cancelled = service
        .cancel_route(&RouteId::from_uuid(f.route), &TenantId::from_uuid(f.tenant))
        .await
        .expect("cancel_route");
    assert!(cancelled);
    assert_eq!(route_status(&db.pool, &f).await, "cancelled");
    assert_eq!(assignment_status(&db.pool, &f).await, "cancelled");

    // A retry after a partial failure still frees a stranded rider.
    sqlx::query("UPDATE dispatch.driver_assignments SET status = 'accepted' WHERE route_id = $1")
        .bind(f.route).execute(&db.pool).await.expect("strand");
    let again = service
        .cancel_route(&RouteId::from_uuid(f.route), &TenantId::from_uuid(f.tenant))
        .await
        .expect("cancel_route retry");
    assert!(!again);
    assert_eq!(assignment_status(&db.pool, &f).await, "cancelled");
    db.drop_db().await;
}

#[tokio::test]
async fn flag_off_writes_nothing() {
    let Some(url) = database_url() else { return };
    let db = Scratch::create(&url).await;
    let repo = PgDriverAssignmentRepository::new(db.pool.clone());

    let f = route_with_assignment(&db.pool, "in_progress", "accepted").await;
    let s1 = Uuid::new_v4();
    stop(&db.pool, &f, s1, 1).await;
    let t1 = task(&db.pool, &f, s1, "completed").await;

    // Work has ended, but the writer is off: the consumer must not touch a row.
    assert!(!handle_task_outcome(&outcome_payload(f.tenant, t1, s1), &repo, false).await.expect("flag off"));
    assert_eq!(assignment_status(&db.pool, &f).await, "accepted");
    assert_eq!(route_status(&db.pool, &f).await, "in_progress");

    // And with it on, the same event closes it.
    assert!(handle_task_outcome(&outcome_payload(f.tenant, t1, s1), &repo, true).await.expect("flag on"));
    assert_eq!(assignment_status(&db.pool, &f).await, "completed");
    db.drop_db().await;
}

#[tokio::test]
async fn sweep_repairs_a_missed_outcome_and_shipment_fallback_resolves() {
    let Some(url) = database_url() else { return };
    let db = Scratch::create(&url).await;
    let repo = PgDriverAssignmentRepository::new(db.pool.clone());

    let f = route_with_assignment(&db.pool, "in_progress", "accepted").await;
    let s1 = Uuid::new_v4();
    task(&db.pool, &f, s1, "completed").await; // no route stops: gig-claim shape

    // Unknown task id falls back to the shipment.
    let resolved = repo
        .find_route_for_outcome(f.tenant, Some(Uuid::new_v4()), Some(s1))
        .await
        .expect("resolve");
    assert_eq!(resolved, Some(f.route));
    // Another tenant never resolves it.
    assert_eq!(
        repo.find_route_for_outcome(Uuid::new_v4(), None, Some(s1)).await.expect("resolve"),
        None
    );

    assert_eq!(repo.close_finished_assignments(100).await.expect("sweep"), 1);
    assert_eq!(assignment_status(&db.pool, &f).await, "completed");
    assert_eq!(repo.close_finished_assignments(100).await.expect("sweep again"), 0);
    db.drop_db().await;
}
