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
use logisticos_pod::domain::repositories::{
    PickupRepository, PodRepository, ShipmentBillingContext, ShipmentBillingContextSource, ShipmentNotFound,
};
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
    service_with(pool, None)
}

fn service_with(pool: &PgPool, source: Option<Arc<dyn ShipmentBillingContextSource>>) -> PodService {
    PodService::new(
        Arc::new(PgPodRepository::new(pool.clone())),
        Arc::new(PgOtpRepository::new(pool.clone())),
        Arc::new(PgPickupRepository::new(pool.clone())),
        Arc::new(PgTelemetryRepository::new(pool.clone())),
        Arc::new(NoStorage),
        Arc::new(NoStorage),
        Arc::new(NoOpSmsAdapter),
        Arc::new(KafkaProducer::new("localhost:9092").expect("producer")),
        source,
        DeliveryPinPolicy::default(),
    )
}

/// order-intake stand-in: every shipment of `tenant` was booked by `booker`.
/// `booker: None` models an order-intake that cannot name one; `down` an outage.
struct Booked { tenant: Uuid, booker: Option<Uuid>, down: bool }

#[async_trait]
impl ShipmentBillingContextSource for Booked {
    async fn fetch(&self, tenant_id: Uuid, shipment_id: Uuid) -> anyhow::Result<ShipmentBillingContext> {
        if self.down { anyhow::bail!("order-intake unreachable"); }
        if tenant_id != self.tenant {
            return Err(anyhow::Error::new(ShipmentNotFound(shipment_id)));
        }
        Ok(ShipmentBillingContext {
            service_code: "standard".into(),
            declared_value_cents: None,
            customer_phone: None,
            merchant_id: self.booker,
        })
    }
}

fn state_booked(pool: &PgPool, src: Booked) -> State<Arc<AppState>> {
    State(Arc::new(AppState {
        pod_service: Arc::new(service_with(pool, Some(Arc::new(src)))),
        jwt: Arc::new(JwtService::new("test-secret-test-secret-test-secret", 3600, 86400)),
    }))
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

// ── Role / ownership scoping inside a tenant (no partner binding) ─────────────

/// A merchant or customer reads the proofs of shipments they booked and nothing
/// else in the tenant. A foreign shipment is "not found" by id, empty by shipment.
#[tokio::test]
async fn merchants_and_customers_read_only_shipments_they_booked() {
    let Some(pool) = pool().await else { return };
    let s = seed(&pool).await;
    for role in ["merchant", "customer"] {
        let me = Uuid::new_v4();
        let mine = || state_booked(&pool, Booked { tenant: s.tenant, booker: Some(me), down: false });
        let theirs = || state_booked(&pool, Booked { tenant: s.tenant, booker: Some(Uuid::new_v4()), down: false });
        let c = || claims(s.tenant, me, &[role], None);

        let _ = h::get_pod(c(), Path(s.their_pod.id), mine()).await.expect("own POD by id");
        let _ = h::get_pop(c(), Path(s.their_pop.id), mine()).await.expect("own POP by id");
        assert!(!empty(&h::list_pods(c(), q(s.their_pod.shipment_id), mine()).await.expect("list")));

        let e = h::get_pod(c(), Path(s.my_pod.id), theirs()).await.expect_err("foreign POD by id");
        assert!(is_not_found(&e), "{role}: {e:?}");
        let e = h::get_pop(c(), Path(s.my_pop.id), theirs()).await.expect_err("foreign POP by id");
        assert!(is_not_found(&e), "{role}: {e:?}");
        assert!(empty(&h::list_pods(c(), q(s.my_pod.shipment_id), theirs()).await.expect("list")), "{role}");
        assert!(empty(&h::list_pops(c(), q(s.my_pop.shipment_id), theirs()).await.expect("list")), "{role}");
    }
}

/// An unverifiable owner is refused (503), never read as "owned".
#[tokio::test]
async fn an_owner_is_refused_when_order_intake_cannot_confirm_the_booker() {
    let Some(pool) = pool().await else { return };
    let s = seed(&pool).await;
    let c = || claims(s.tenant, Uuid::new_v4(), &["merchant"], None);
    for src in [
        Booked { tenant: s.tenant, booker: None, down: false },
        Booked { tenant: s.tenant, booker: None, down: true },
    ] {
        let e = h::get_pod(c(), Path(s.my_pod.id), state_booked(&pool, src)).await.expect_err("must refuse");
        assert!(matches!(e, AppError::ServiceUnavailable(_)), "{e:?}");
    }
}

/// A plain (non-partner) driver reads the proofs they captured, and no one else's.
#[tokio::test]
async fn a_driver_reads_only_proofs_they_captured() {
    let Some(pool) = pool().await else { return };
    let s = seed(&pool).await;
    let me = || claims(s.tenant, s.mine, &["driver"], None);

    let _ = h::get_pod(me(), Path(s.my_pod.id), state(&pool)).await.expect("own POD");
    let _ = h::get_pop(me(), Path(s.my_pop.id), state(&pool)).await.expect("own POP");
    let e = h::get_pod(me(), Path(s.their_pod.id), state(&pool)).await.expect_err("other POD");
    assert!(is_not_found(&e), "{e:?}");
    let e = h::get_pop(me(), Path(s.their_pop.id), state(&pool)).await.expect_err("other POP");
    assert!(is_not_found(&e), "{e:?}");
    assert!(empty(&h::list_pods(me(), q(s.their_pod.shipment_id), state(&pool)).await.expect("list")));
    assert!(empty(&h::list_pops(me(), q(s.their_pop.shipment_id), state(&pool)).await.expect("list")));
}

/// Operators read the tenant; roles outside the policy read nothing.
#[tokio::test]
async fn operators_read_the_tenant_and_other_roles_read_nothing() {
    let Some(pool) = pool().await else { return };
    let s = seed(&pool).await;
    for role in ["admin", "tenant_admin", "dispatcher", "ops", "support"] {
        let c = claims(s.tenant, Uuid::new_v4(), &[role], None);
        let _ = h::get_pod(c, Path(s.their_pod.id), state(&pool)).await.unwrap_or_else(|e| panic!("{role}: {e:?}"));
    }
    for role in ["finance", "readonly", "hub_scanner", "mystery"] {
        let c = claims(s.tenant, s.mine, &[role], None);
        let e = h::get_pod(c, Path(s.my_pod.id), state(&pool)).await.expect_err(role);
        assert!(is_not_found(&e), "{role}: {e:?}");
    }
}
