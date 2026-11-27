//! The Phase 0 gate says a partner token cannot read another partner's shipment
//! data, and POD/POP evidence (recipient photos, signature, GPS) is exactly that.
//! A pid-bound caller reads only the evidence it captured itself; another
//! driver's evidence in the same tenant is "not found" by id and empty by
//! shipment. An unscoped caller (tenant admin) still reads everything.
//!
//! These drive the real HTTP handlers with real claims.
//!
//! Needs Postgres with the pod migrations applied. Skipped locally when
//! DATABASE_URL is unset; fatal in CI, where a missing database means the
//! harness broke and a green pass over nothing would be worse than no test.

use std::sync::Arc;

use async_trait::async_trait;
use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

use axum::extract::{Path, Query, State};
use logisticos_auth::claims::Claims;
use logisticos_auth::jwt::JwtService;
use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;
use logisticos_pod::api::http::{pod as h, AppState};
use logisticos_events::producer::KafkaProducer;
use logisticos_pod::application::services::PodService;
use logisticos_pod::domain::entities::{ProofOfDelivery, ProofOfPickup};
use logisticos_pod::domain::repositories::{PickupRepository, PodRepository};
use logisticos_pod::domain::value_objects::delivery_pin::DeliveryPinPolicy;
use logisticos_pod::infrastructure::db::{
    PgOtpRepository, PgPickupRepository, PgPodRepository, PgTelemetryRepository,
};
use logisticos_pod::infrastructure::external::sms::NoOpSmsAdapter;
use logisticos_pod::infrastructure::external::storage::{PresignedUpload, StorageAdapter};

struct NoStorage;

#[async_trait]
impl StorageAdapter for NoStorage {
    async fn presign_upload(&self, key: &str, _ct: &str, _ttl: u32) -> anyhow::Result<PresignedUpload> {
        Ok(PresignedUpload { url: format!("https://s3.test/{key}"), headers: Default::default() })
    }
    async fn presign_download(&self, key: &str, _ttl: u32) -> anyhow::Result<String> {
        Ok(format!("https://s3.test/dl/{key}"))
    }
    async fn object_exists(&self, _key: &str) -> anyhow::Result<bool> { Ok(true) }
    async fn delete(&self, _key: &str) -> anyhow::Result<()> { Ok(()) }
}

async fn pool() -> Option<PgPool> {
    match std::env::var("DATABASE_URL") {
        Ok(url) => Some(PgPoolOptions::new().max_connections(4).connect(&url).await.expect("connect")),
        Err(_) => {
            assert!(
                std::env::var("CI").is_err(),
                "DATABASE_URL is unset while CI is set: Postgres provisioning failed. \
                 These tests guard cross-tenant reads of delivery evidence; they must not be skipped here."
            );
            eprintln!("skipping: DATABASE_URL not set (this is fatal when CI is set)");
            None
        }
    }
}

fn service(pool: &PgPool) -> PodService {
    PodService::new(
        Arc::new(PgPodRepository::new(pool.clone())),
        Arc::new(PgOtpRepository::new(pool.clone())),
        Arc::new(PgPickupRepository::new(pool.clone())),
        Arc::new(PgTelemetryRepository::new(pool.clone())),
        Arc::new(NoStorage),
        Arc::new(NoStorage),
        Arc::new(NoOpSmsAdapter),
        Arc::new(KafkaProducer::new("localhost:9092").expect("producer")),
        None,
        DeliveryPinPolicy::default(),
    )
}


fn claims(tenant: Uuid, user: Uuid, roles: &[&str], pid: Option<Uuid>) -> AuthClaims {
    AuthClaims(
        Claims::new(
            user, tenant, "demo".into(), "growth".into(), "x@example.com".into(),
            roles.iter().map(|r| r.to_string()).collect(), vec![], 3600,
        )
        .with_partner(pid),
    )
}

fn state(pool: &PgPool) -> State<Arc<AppState>> {
    State(Arc::new(AppState {
        pod_service: Arc::new(service(pool)),
        jwt: Arc::new(JwtService::new("test-secret-test-secret-test-secret", 3600, 86400)),
    }))
}

fn is_not_found(e: &AppError) -> bool {
    matches!(e, AppError::NotFound { .. })
}

fn empty(v: &axum::Json<serde_json::Value>) -> bool {
    v.0["data"].as_array().is_some_and(|a| a.is_empty())
}

struct Seed {
    tenant: Uuid,
    mine: Uuid,
    my_pod: ProofOfDelivery,
    their_pod: ProofOfDelivery,
    my_pop: ProofOfPickup,
    their_pop: ProofOfPickup,
}

async fn seed(pool: &PgPool) -> Seed {
    let (tenant, mine, other) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let pod = |driver| ProofOfDelivery::new(
        tenant, Uuid::new_v4(), Uuid::new_v4(), driver,
        "Maria Santos".into(), 14.5995, 120.9842, true, false, None, true, true,
    );
    let pop = |driver| ProofOfPickup::new(
        tenant, Uuid::new_v4(), Uuid::new_v4(), driver,
        14.5995, 120.9842, true, false, None, "standard".into(), None, None,
    );
    let s = Seed {
        tenant, mine,
        my_pod: pod(mine), their_pod: pod(other), my_pop: pop(mine), their_pop: pop(other),
    };
    let (pods, pops) = (PgPodRepository::new(pool.clone()), PgPickupRepository::new(pool.clone()));
    for p in [&s.my_pod, &s.their_pod] { pods.save(p).await.expect("seed pod"); }
    for p in [&s.my_pop, &s.their_pop] { pops.save(p).await.expect("seed pop"); }
    s
}

fn q<T: serde::de::DeserializeOwned>(shipment: Uuid) -> Query<T> {
    Query(serde_json::from_value(serde_json::json!({ "shipment_id": shipment })).expect("query"))
}

#[tokio::test]
async fn a_partner_driver_reads_its_own_evidence() {
    let Some(pool) = pool().await else { return };
    let s = seed(&pool).await;
    let me = || claims(s.tenant, s.mine, &["driver"], Some(Uuid::new_v4()));

    let _ = h::get_pod(me(), Path(s.my_pod.id), state(&pool)).await.expect("own POD by id");
    let _ = h::get_pop(me(), Path(s.my_pop.id), state(&pool)).await.expect("own POP by id");
    let v = h::list_pods(me(), q(s.my_pod.shipment_id), state(&pool)).await.expect("list");
    assert!(!empty(&v), "own POD by shipment");
    let v = h::list_pops(me(), q(s.my_pop.shipment_id), state(&pool)).await.expect("list");
    assert!(!empty(&v) && v.0["data"].is_object(), "own POP by shipment");
}

#[tokio::test]
async fn a_partner_driver_cannot_read_another_drivers_evidence() {
    let Some(pool) = pool().await else { return };
    let s = seed(&pool).await;
    let me = || claims(s.tenant, s.mine, &["driver"], Some(Uuid::new_v4()));

    let e = h::get_pod(me(), Path(s.their_pod.id), state(&pool)).await.expect_err("POD by id");
    assert!(is_not_found(&e), "{e:?}");
    let e = h::get_pop(me(), Path(s.their_pop.id), state(&pool)).await.expect_err("POP by id");
    assert!(is_not_found(&e), "{e:?}");
    let v = h::list_pods(me(), q(s.their_pod.shipment_id), state(&pool)).await.expect("list");
    assert!(empty(&v), "POD by shipment must look empty");
    let v = h::list_pops(me(), q(s.their_pop.shipment_id), state(&pool)).await.expect("list");
    assert!(empty(&v), "POP by shipment must look empty");
}

#[tokio::test]
async fn an_unscoped_admin_still_reads_both() {
    let Some(pool) = pool().await else { return };
    let s = seed(&pool).await;
    let admin = || claims(s.tenant, Uuid::new_v4(), &["admin"], None);

    for pod in [&s.my_pod, &s.their_pod] {
        let _ = h::get_pod(admin(), Path(pod.id), state(&pool)).await.expect("POD by id");
        let v = h::list_pods(admin(), q(pod.shipment_id), state(&pool)).await.expect("list");
        assert!(!empty(&v));
    }
    for pop in [&s.my_pop, &s.their_pop] {
        let _ = h::get_pop(admin(), Path(pop.id), state(&pool)).await.expect("POP by id");
        let v = h::list_pops(admin(), q(pop.shipment_id), state(&pool)).await.expect("list");
        assert!(!empty(&v));
    }
}

#[tokio::test]
async fn a_partner_role_token_that_captured_nothing_reads_nothing() {
    let Some(pool) = pool().await else { return };
    let s = seed(&pool).await;
    // A pid-bound `partner` (not a driver): no evidence is its own.
    let p = || claims(s.tenant, Uuid::new_v4(), &["partner"], Some(Uuid::new_v4()));

    for id in [s.my_pod.id, s.their_pod.id] {
        let e = h::get_pod(p(), Path(id), state(&pool)).await.expect_err("POD");
        assert!(is_not_found(&e), "{e:?}");
    }
    for id in [s.my_pop.id, s.their_pop.id] {
        let e = h::get_pop(p(), Path(id), state(&pool)).await.expect_err("POP");
        assert!(is_not_found(&e), "{e:?}");
    }
    for pod in [&s.my_pod, &s.their_pod] {
        assert!(empty(&h::list_pods(p(), q(pod.shipment_id), state(&pool)).await.expect("list")));
    }
    for pop in [&s.my_pop, &s.their_pop] {
        assert!(empty(&h::list_pops(p(), q(pop.shipment_id), state(&pool)).await.expect("list")));
    }
}
