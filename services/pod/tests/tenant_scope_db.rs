//! A proof of delivery or pickup is evidence (photos, signature, GPS, the
//! recipient's name). It must be readable only inside the tenant that owns it:
//! asking for another tenant's id is "not found", exactly like an id that does
//! not exist, so ids cannot be probed.
//!
//! Needs Postgres with the pod migrations applied. Skipped locally when
//! DATABASE_URL is unset; fatal in CI, where a missing database means the
//! harness broke and a green pass over nothing would be worse than no test.

use std::sync::Arc;

use async_trait::async_trait;
use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

use logisticos_errors::AppError;
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
use logisticos_types::TenantId;

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

fn service_with(
    pool: &PgPool,
    ctx: Option<Arc<dyn logisticos_pod::domain::repositories::ShipmentBillingContextSource>>,
) -> PodService {
    PodService::new(
        Arc::new(PgPodRepository::new(pool.clone())),
        Arc::new(PgOtpRepository::new(pool.clone())),
        Arc::new(PgPickupRepository::new(pool.clone())),
        Arc::new(PgTelemetryRepository::new(pool.clone())),
        Arc::new(NoStorage),
        Arc::new(NoStorage),
        Arc::new(NoOpSmsAdapter),
        Arc::new(KafkaProducer::new("localhost:9092").expect("producer")),
        ctx,
        DeliveryPinPolicy::default(),
    )
}

fn pod_for(tenant: Uuid) -> ProofOfDelivery {
    ProofOfDelivery::new(
        tenant, Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(),
        "Maria Santos".into(), 14.5995, 120.9842, true, false, None, true, true,
    )
}

fn pop_for(tenant: Uuid) -> ProofOfPickup {
    ProofOfPickup::new(
        tenant, Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(),
        14.5995, 120.9842, true, false, None, "standard".into(), None, None,
    )
}

fn is_not_found(e: &AppError) -> bool {
    matches!(e, AppError::NotFound { .. })
}

#[tokio::test]
async fn pod_of_another_tenant_is_not_found() {
    let Some(pool) = pool().await else { return };
    let (mine, theirs) = (Uuid::new_v4(), Uuid::new_v4());
    let pod = pod_for(theirs);
    PgPodRepository::new(pool.clone()).save(&pod).await.expect("seed");
    let svc = service(&pool);

    let err = svc.get_by_id(&TenantId::from_uuid(mine), pod.id).await
        .expect_err("another tenant's POD must not be readable");
    assert!(is_not_found(&err), "must look exactly like a missing id: {err:?}");

    // Same answer through the repository, and by shipment.
    let repo = PgPodRepository::new(pool.clone());
    assert!(repo.find_by_id(mine, pod.id).await.expect("query").is_none());
    assert!(repo.find_by_shipment(mine, pod.shipment_id).await.expect("query").is_none());
    assert!(svc.get_by_shipment(&TenantId::from_uuid(mine), pod.shipment_id).await.expect("query").is_none());
}

#[tokio::test]
async fn own_pod_is_returned() {
    let Some(pool) = pool().await else { return };
    let mine = Uuid::new_v4();
    let pod = pod_for(mine);
    PgPodRepository::new(pool.clone()).save(&pod).await.expect("seed");
    let svc = service(&pool);

    let got = svc.get_by_id(&TenantId::from_uuid(mine), pod.id).await.expect("own POD");
    assert_eq!(got.id, pod.id);
    let by_ship = svc.get_by_shipment(&TenantId::from_uuid(mine), pod.shipment_id).await.expect("query")
        .expect("own POD by shipment");
    assert_eq!(by_ship.id, pod.id);
}

#[tokio::test]
async fn the_mutating_paths_are_scoped_too() {
    let Some(pool) = pool().await else { return };
    let (mine, theirs) = (Uuid::new_v4(), Uuid::new_v4());
    let pod = pod_for(theirs);
    PgPodRepository::new(pool.clone()).save(&pod).await.expect("seed");
    let svc = service(&pool);
    let me = TenantId::from_uuid(mine);

    let err = svc
        .get_upload_url(pod.id, &me, "image/jpeg")
        .await
        .expect_err("presigning an upload against another tenant's POD");
    assert!(is_not_found(&err), "{err:?}");
}

#[tokio::test]
async fn pop_of_another_tenant_is_not_found_and_own_is_returned() {
    let Some(pool) = pool().await else { return };
    let (mine, theirs) = (Uuid::new_v4(), Uuid::new_v4());
    let (their_pop, my_pop) = (pop_for(theirs), pop_for(mine));
    let repo = PgPickupRepository::new(pool.clone());
    repo.save(&their_pop).await.expect("seed theirs");
    repo.save(&my_pop).await.expect("seed mine");
    let svc = service(&pool);
    let me = TenantId::from_uuid(mine);

    let err = svc.get_pop_by_id(&me, their_pop.id).await.expect_err("another tenant's POP");
    assert!(is_not_found(&err), "{err:?}");
    assert!(svc.get_pop_by_shipment(&me, their_pop.shipment_id).await.expect("query").is_none());
    let err = svc
        .get_pop_upload_url(their_pop.id, &me, "image/jpeg")
        .await
        .expect_err("presigning an upload against another tenant's POP");
    assert!(is_not_found(&err), "{err:?}");

    assert_eq!(svc.get_pop_by_id(&me, my_pop.id).await.expect("own POP").id, my_pop.id);
    assert_eq!(
        svc.get_pop_by_shipment(&me, my_pop.shipment_id).await.expect("query").expect("own").id,
        my_pop.id
    );
}

// ── Cross-tenant evidence poisoning ────────────────────────────────────────

fn submitted(mut pop: ProofOfPickup) -> ProofOfPickup {
    pop.status = logisticos_pod::domain::entities::PopStatus::Submitted;
    pop
}

fn initiate_pod_cmd(shipment: Uuid) -> logisticos_pod::application::commands::InitiatePodCommand {
    serde_json::from_value(serde_json::json!({
        "shipment_id": shipment, "task_id": Uuid::new_v4(), "recipient_name": "R",
        "capture_lat": 14.5995, "capture_lng": 120.9842,
    }))
    .expect("command")
}

fn initiate_pop_cmd(shipment: Uuid) -> logisticos_pod::application::commands::InitiatePickupCommand {
    serde_json::from_value(serde_json::json!({
        "shipment_id": shipment, "task_id": Uuid::new_v4(),
        "capture_lat": 14.5995, "capture_lng": 120.9842, "pickup_lat": 14.5995, "pickup_lng": 120.9842,
    }))
    .expect("command")
}

#[tokio::test]
async fn planted_pod_from_another_tenant_is_not_returned_to_the_owner() {
    let Some(pool) = pool().await else { return };
    let (owner, attacker) = (Uuid::new_v4(), Uuid::new_v4());
    // uq_pod_shipment is global, so the attacker's row can only be the one that
    // exists. An unscoped read would hand it to the owner as their evidence.
    let planted = pod_for(attacker);
    let repo = PgPodRepository::new(pool.clone());
    repo.save(&planted).await.expect("seed planted");

    assert!(
        repo.find_by_shipment(owner, planted.shipment_id).await.expect("query").is_none(),
        "the attacker's row must not be returned as the owner's evidence"
    );
    assert!(repo.find_by_shipment(attacker, planted.shipment_id).await.expect("query").is_some());

    // The same for pickups: a submitted POP planted by another tenant must not
    // satisfy (or be reported as) the owner's POP gate.
    let their_pop = submitted(pop_for(attacker));
    let shipment = their_pop.shipment_id;
    let pops = PgPickupRepository::new(pool.clone());
    pops.save(&their_pop).await.expect("seed planted pop");
    assert!(pops.find_completed_by_shipment(owner, shipment).await.expect("query").is_none());
    assert!(pops.find_completed_by_shipment(attacker, shipment).await.expect("query").is_some());
}

/// order-intake as pod sees it: the booking belongs to `owner`. Anyone else's
/// lookup is `ShipmentNotFound`, exactly like a missing shipment.
struct Booking {
    owner: Uuid,
    phone: Option<String>,
}

#[async_trait]
impl logisticos_pod::domain::repositories::ShipmentBillingContextSource for Booking {
    async fn fetch(
        &self,
        tenant_id: Uuid,
        shipment_id: Uuid,
    ) -> anyhow::Result<logisticos_pod::domain::repositories::ShipmentBillingContext> {
        if tenant_id != self.owner {
            return Err(anyhow::Error::new(logisticos_pod::domain::repositories::ShipmentNotFound(shipment_id)));
        }
        Ok(logisticos_pod::domain::repositories::ShipmentBillingContext {
            service_code: "standard".into(),
            declared_value_cents: None,
            customer_phone: self.phone.clone(),
            merchant_id: None,
        })
    }
}

/// order-intake unreachable, or too old to name the tenant.
struct Down;

#[async_trait]
impl logisticos_pod::domain::repositories::ShipmentBillingContextSource for Down {
    async fn fetch(
        &self,
        _t: Uuid,
        _s: Uuid,
    ) -> anyhow::Result<logisticos_pod::domain::repositories::ShipmentBillingContext> {
        anyhow::bail!("order-intake unreachable")
    }
}

fn driver() -> logisticos_types::DriverId {
    logisticos_types::DriverId::from_uuid(Uuid::new_v4())
}

fn is_shipment_not_found(e: &AppError) -> bool {
    matches!(e, AppError::NotFound { resource, .. } if *resource == "Shipment")
}

#[tokio::test]
async fn a_foreign_row_on_the_shipment_does_not_block_the_owner() {
    let Some(pool) = pool().await else { return };
    let (owner, squatter) = (Uuid::new_v4(), Uuid::new_v4());

    // The squatter got there first: a draft POD and a submitted POP on S.
    let squat_pod = pod_for(squatter);
    let shipment = squat_pod.shipment_id;
    PgPodRepository::new(pool.clone()).save(&squat_pod).await.expect("squat pod");
    let mut squat_pop = submitted(pop_for(squatter));
    squat_pop.shipment_id = shipment;
    PgPickupRepository::new(pool.clone()).save(&squat_pop).await.expect("squat pop");

    // Who wrote first decides nothing: the owner opens its own evidence on S.
    let svc = service_with(&pool, Some(Arc::new(Booking { owner, phone: None })));
    let me = TenantId::from_uuid(owner);
    let pod = svc.initiate(&driver(), &me, initiate_pod_cmd(shipment), 14.5995, 120.9842).await
        .expect("owner's POD initiate is not blocked by the squatter's draft");
    assert_eq!(pod.tenant_id, owner);
    let pop = svc.initiate_pickup(&driver(), &me, initiate_pop_cmd(shipment)).await
        .expect("owner's POP initiate");
    assert_eq!(pop.tenant_id, owner);

    // ...and can complete it: a submitted POP per tenant is allowed on the same id.
    let mut owner_submitted = submitted(pop);
    owner_submitted.driver_id = Uuid::new_v4();
    PgPickupRepository::new(pool.clone()).save(&owner_submitted).await
        .expect("owner's submitted POP is not blocked by the squatter's submitted POP");

    // Each tenant sees only its own.
    let pods = PgPodRepository::new(pool.clone());
    assert_eq!(pods.find_by_shipment(owner, shipment).await.unwrap().unwrap().id, pod.id);
    assert_eq!(pods.find_by_shipment(squatter, shipment).await.unwrap().unwrap().id, squat_pod.id);
}

#[tokio::test]
async fn a_foreign_tenant_is_refused_when_the_booking_belongs_to_someone_else() {
    let Some(pool) = pool().await else { return };
    let (owner, attacker) = (Uuid::new_v4(), Uuid::new_v4());
    let shipment = Uuid::new_v4();
    let svc = service_with(&pool, Some(Arc::new(Booking { owner, phone: None })));

    let err = svc.initiate(&driver(), &TenantId::from_uuid(attacker), initiate_pod_cmd(shipment), 14.5995, 120.9842).await
        .expect_err("a POD on someone else's booking");
    assert!(is_shipment_not_found(&err), "{err:?}");
    let err = svc.initiate_pickup(&driver(), &TenantId::from_uuid(attacker), initiate_pop_cmd(shipment)).await
        .expect_err("a POP on someone else's booking");
    assert!(is_shipment_not_found(&err), "{err:?}");

    // Nothing was written under the attacker's tenant.
    assert!(PgPodRepository::new(pool.clone()).find_by_shipment(attacker, shipment).await.unwrap().is_none());
    assert!(PgPickupRepository::new(pool.clone()).find_by_shipment(attacker, shipment).await.unwrap().is_none());

    // The owner proceeds.
    svc.initiate(&driver(), &TenantId::from_uuid(owner), initiate_pod_cmd(shipment), 14.5995, 120.9842).await
        .expect("owner POD");
    svc.initiate_pickup(&driver(), &TenantId::from_uuid(owner), initiate_pop_cmd(shipment)).await.expect("owner POP");
}

fn is_unavailable(e: &AppError) -> bool {
    matches!(e, AppError::ServiceUnavailable(_))
}

/// Nothing was written for this shipment under this tenant.
async fn nothing_written(pool: &PgPool, tenant: Uuid, shipment: Uuid) {
    assert!(PgPodRepository::new(pool.clone()).find_by_shipment(tenant, shipment).await.unwrap().is_none());
    assert!(PgPickupRepository::new(pool.clone()).find_by_shipment(tenant, shipment).await.unwrap().is_none());
}

/// order-intake answered, but unusably: what `context_from_response` produces for
/// a body with no `tenant_id` (an order-intake that predates it).
struct NoTenantId;

#[async_trait]
impl logisticos_pod::domain::repositories::ShipmentBillingContextSource for NoTenantId {
    async fn fetch(
        &self,
        _t: Uuid,
        s: Uuid,
    ) -> anyhow::Result<logisticos_pod::domain::repositories::ShipmentBillingContext> {
        anyhow::bail!("order-intake returned no tenant_id for shipment {s} (deployed version predates tenant_id)")
    }
}

#[tokio::test]
async fn an_unverifiable_shipment_is_refused_with_503_and_nothing_is_written() {
    let Some(pool) = pool().await else { return };
    let tenant = Uuid::new_v4();
    let me = TenantId::from_uuid(tenant);

    let sources: Vec<(&str, Option<Arc<dyn logisticos_pod::domain::repositories::ShipmentBillingContextSource>>)> = vec![
        ("outage", Some(Arc::new(Down))),
        ("no tenant_id", Some(Arc::new(NoTenantId))),
        ("no order-intake configured", None),
    ];
    for (why, source) in sources {
        let svc = service_with(&pool, source);
        let shipment = Uuid::new_v4();
        let err = svc.initiate(&driver(), &me, initiate_pod_cmd(shipment), 14.5995, 120.9842).await
            .expect_err(why);
        assert!(is_unavailable(&err), "POD, {why}: {err:?}");
        let err = svc.initiate_pickup(&driver(), &me, initiate_pop_cmd(shipment)).await.expect_err(why);
        assert!(is_unavailable(&err), "POP, {why}: {err:?}");
        nothing_written(&pool, tenant, shipment).await;
    }
}

#[tokio::test]
async fn the_booking_wins_over_the_device_service_code() {
    let Some(pool) = pool().await else { return };
    let owner = Uuid::new_v4();
    let svc = service_with(&pool, Some(Arc::new(Booking { owner, phone: None })));
    let mut cmd = initiate_pop_cmd(Uuid::new_v4());
    cmd.service_code = "balikbayan".into(); // the device claims Track A; the booking says standard
    let pop = svc.initiate_pickup(&driver(), &TenantId::from_uuid(owner), cmd).await.expect("owner");
    assert_eq!(pop.service_code, "standard");
}

#[tokio::test]
async fn the_delivery_pin_recipient_comes_from_the_callers_own_booking() {
    let Some(pool) = pool().await else { return };
    let (owner, other) = (Uuid::new_v4(), Uuid::new_v4());
    let shipment = Uuid::new_v4();
    let booked = Booking { owner, phone: Some("+639171234567".into()) };

    let svc = service_with(&pool, Some(Arc::new(booked)));
    assert_eq!(
        svc.recipient_phone_for_shipment(&TenantId::from_uuid(owner), shipment).await.expect("own booking"),
        "+639171234567"
    );

    // Another tenant's driver cannot make pod SMS a PIN to this recipient.
    let err = svc.recipient_phone_for_shipment(&TenantId::from_uuid(other), shipment).await
        .expect_err("a foreign tenant must not resolve the recipient");
    assert!(is_shipment_not_found(&err), "{err:?}");

    // Unlike capture, this stays blocking: no verified ownership, no PIN.
    let down = service_with(&pool, Some(Arc::new(Down)));
    let err = down.recipient_phone_for_shipment(&TenantId::from_uuid(owner), shipment).await.unwrap_err();
    assert!(is_unavailable(&err), "{err:?}");
}

mod http {
    use super::*;
    use axum::{body::Body, http::{Request, StatusCode}};
    use tower::ServiceExt;

    fn app(pool: &PgPool) -> axum::Router {
        logisticos_pod::api::http::router(Arc::new(logisticos_pod::api::http::AppState {
            pod_service: Arc::new(service(pool)),
            jwt: Arc::new(logisticos_auth::jwt::JwtService::new("pod-test-secret-long-enough-for-hs256", 3600, 86400)),
        }))
    }

    async fn get(app: axum::Router, uri: String) -> (StatusCode, serde_json::Value) {
        let r = app.oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap()).await.unwrap();
        let st = r.status();
        let b = axum::body::to_bytes(r.into_body(), usize::MAX).await.unwrap();
        (st, serde_json::from_slice(&b).unwrap_or(serde_json::Value::Null))
    }

    #[tokio::test]
    async fn internal_read_without_tenant_is_rejected() {
        let Some(pool) = pool().await else { return };
        let s = Uuid::new_v4();
        let (st, _) = get(app(&pool), format!("/v1/internal/pop-status?shipment_ids={s}")).await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "pop-status needs tenant_id");
        let (st, _) = get(app(&pool), format!("/v1/internal/pop-evidence/{s}")).await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "pop-evidence needs tenant_id");
    }

    #[tokio::test]
    async fn internal_reads_are_bound_to_the_tenant_they_name() {
        let Some(pool) = pool().await else { return };
        let (owner, other) = (Uuid::new_v4(), Uuid::new_v4());
        let pop = submitted(pop_for(owner));
        PgPickupRepository::new(pool.clone()).save(&pop).await.expect("seed");
        let pod = pod_for(owner);
        PgPodRepository::new(pool.clone()).save(&pod).await.expect("seed pod");

        let (st, body) = get(app(&pool), format!("/v1/internal/pop-status?tenant_id={owner}&shipment_ids={}", pop.shipment_id)).await;
        assert_eq!(st, StatusCode::OK, "{body}");
        assert_eq!(body["statuses"][0]["has_completed_pop"], true);
        let (_, body) = get(app(&pool), format!("/v1/internal/pop-status?tenant_id={other}&shipment_ids={}", pop.shipment_id)).await;
        assert_eq!(body["statuses"][0]["has_completed_pop"], false, "another tenant's POP is not this tenant's");

        let (st, body) = get(app(&pool), format!("/v1/internal/pop-evidence/{}?tenant_id={owner}", pod.shipment_id)).await;
        assert_eq!(st, StatusCode::OK);
        assert!(body["pod"].is_object(), "{body}");
        let (_, body) = get(app(&pool), format!("/v1/internal/pop-evidence/{}?tenant_id={other}", pod.shipment_id)).await;
        assert!(body["pod"].is_null(), "{body}");
    }
}
