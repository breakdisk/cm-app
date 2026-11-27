use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use async_trait::async_trait;
use axum::{body::Body, http::{header, Method, Request, StatusCode}, Router};
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;
use logisticos_auth::{claims::Claims, jwt::JwtService};

use logisticos_pod::{
    api::http::{router, AppState},
    application::services::PodService,
    domain::{
        entities::{OtpCode, PodStatus, PopStatus, ProofOfDelivery, ProofOfPickup},
        repositories::{
            OtpRepository, PickupRepository, PodRepository, ShipmentBillingContext,
            ShipmentBillingContextSource, TelemetryEntry, TelemetryRepository,
        },
        value_objects::delivery_pin::DeliveryPinPolicy,
    },
    infrastructure::external::{
        sms::SmsAdapter,
        storage::{PresignedUpload, StorageAdapter},
    },
};
use logisticos_events::producer::KafkaProducer;

const JWT_SECRET: &str = "test-secret-key-for-logisticos-testing";
/// The recipient phone order-intake holds on every booking in this suite.
const BOOKING_PHONE: &str = "+639171234567";

// ─────────────────────────────────────────────────────────────────────────────
// Mock repositories
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Default, Clone)]
struct InMemoryPodRepo {
    store: Arc<Mutex<HashMap<Uuid, ProofOfDelivery>>>,
}

#[async_trait]
impl PodRepository for InMemoryPodRepo {
    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<Option<ProofOfDelivery>> {
        Ok(self.store.lock().unwrap().get(&id).filter(|p| p.tenant_id == tenant_id).cloned())
    }
    async fn find_by_shipment(&self, tenant_id: Uuid, sid: Uuid) -> anyhow::Result<Option<ProofOfDelivery>> {
        Ok(self.store.lock().unwrap().values()
            .find(|p| p.tenant_id == tenant_id && p.shipment_id == sid).cloned())
    }
    async fn save(&self, pod: &ProofOfDelivery) -> anyhow::Result<()> {
        self.store.lock().unwrap().insert(pod.id, pod.clone());
        Ok(())
    }
}

#[derive(Default, Clone)]
struct InMemoryOtpRepo {
    store: Arc<Mutex<HashMap<Uuid, OtpCode>>>,
}

#[async_trait]
impl OtpRepository for InMemoryOtpRepo {
    async fn find_active_by_shipment(&self, sid: Uuid, tenant_id: Uuid) -> anyhow::Result<Option<OtpCode>> {
        let now = chrono::Utc::now();
        Ok(self.store.lock().unwrap().values()
            .filter(|o| o.tenant_id == tenant_id && o.shipment_id == sid && !o.is_used && o.expires_at > now)
            .max_by_key(|o| o.created_at)
            .cloned())
    }
    async fn save(&self, otp: &OtpCode) -> anyhow::Result<()> {
        self.store.lock().unwrap().insert(otp.id, otp.clone());
        Ok(())
    }
    async fn claim_attempt(&self, id: Uuid, max_attempts: i32) -> anyhow::Result<Option<i32>> {
        let mut store = self.store.lock().unwrap();
        Ok(store.get_mut(&id).and_then(|o| {
            (o.failed_attempts < max_attempts).then(|| {
                o.failed_attempts += 1;
                o.failed_attempts
            })
        }))
    }
    async fn release_attempt(&self, id: Uuid) -> anyhow::Result<()> {
        if let Some(o) = self.store.lock().unwrap().get_mut(&id) {
            o.failed_attempts = (o.failed_attempts - 1).max(0);
        }
        Ok(())
    }
}

/// Nothing in this suite drives the POP routes; these only satisfy `PodService::new`.
#[derive(Default, Clone)]
struct InMemoryPickupRepo {
    store: Arc<Mutex<HashMap<Uuid, ProofOfPickup>>>,
}

#[async_trait]
impl PickupRepository for InMemoryPickupRepo {
    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<Option<ProofOfPickup>> {
        Ok(self.store.lock().unwrap().get(&id).filter(|p| p.tenant_id == tenant_id).cloned())
    }
    async fn find_by_shipment(&self, tenant_id: Uuid, sid: Uuid) -> anyhow::Result<Option<ProofOfPickup>> {
        Ok(self.store.lock().unwrap().values()
            .find(|p| p.tenant_id == tenant_id && p.shipment_id == sid).cloned())
    }
    async fn find_completed_by_shipment(&self, tenant_id: Uuid, sid: Uuid) -> anyhow::Result<Option<ProofOfPickup>> {
        Ok(self.store.lock().unwrap().values()
            .find(|p| p.tenant_id == tenant_id && p.shipment_id == sid && p.status == PopStatus::Submitted)
            .cloned())
    }
    async fn save(&self, pop: &ProofOfPickup) -> anyhow::Result<()> {
        self.store.lock().unwrap().insert(pop.id, pop.clone());
        Ok(())
    }
}

#[derive(Default, Clone)]
struct InMemoryTelemetry {
    entries: Arc<Mutex<Vec<TelemetryEntry>>>,
}

#[async_trait]
impl TelemetryRepository for InMemoryTelemetry {
    async fn append(&self, entry: TelemetryEntry) -> anyhow::Result<()> {
        self.entries.lock().unwrap().push(entry);
        Ok(())
    }
    async fn list_for_shipment(&self, tenant_id: Uuid, shipment_id: Uuid, limit: u32) -> anyhow::Result<Vec<TelemetryEntry>> {
        Ok(self.entries.lock().unwrap().iter()
            .filter(|e| e.tenant_id == tenant_id && e.shipment_id == shipment_id)
            .take(limit as usize).cloned().collect())
    }
}

/// order-intake's answer: every shipment is the caller's, with a recipient phone.
struct BookingSource;

#[async_trait]
impl ShipmentBillingContextSource for BookingSource {
    async fn fetch(&self, _tenant_id: Uuid, _shipment_id: Uuid) -> anyhow::Result<ShipmentBillingContext> {
        Ok(ShipmentBillingContext {
            service_code:         "standard".into(),
            declared_value_cents: None,
            customer_phone:       Some(BOOKING_PHONE.into()),
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Mock adapters
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Default, Clone)]
struct MockStorage {
    upload_urls: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl StorageAdapter for MockStorage {
    async fn presign_upload(&self, key: &str, _ct: &str, _ttl: u32) -> anyhow::Result<PresignedUpload> {
        let url = format!("https://s3.test/{}", key);
        self.upload_urls.lock().unwrap().push(url.clone());
        Ok(PresignedUpload { url, headers: Default::default() })
    }
    async fn presign_download(&self, key: &str, _ttl: u32) -> anyhow::Result<String> {
        Ok(format!("https://s3.test/dl/{}", key))
    }
    async fn object_exists(&self, _key: &str) -> anyhow::Result<bool> { Ok(true) }
    async fn delete(&self, _key: &str) -> anyhow::Result<()> { Ok(()) }
}

#[derive(Default, Clone)]
struct MockSms {
    sent: Arc<Mutex<Vec<(String, String)>>>,
}

#[async_trait]
impl SmsAdapter for MockSms {
    async fn send(&self, to: &str, body: &str) -> anyhow::Result<()> {
        self.sent.lock().unwrap().push((to.into(), body.into()));
        Ok(())
    }
}

/// rdkafka's in-process mock broker. Leaked so it outlives every router built from it.
fn mock_brokers() -> String {
    let cluster = rdkafka::mocking::MockCluster::new(1).expect("mock kafka cluster");
    let brokers = cluster.bootstrap_servers();
    assert!(!brokers.is_empty());
    Box::leak(Box::new(cluster));
    brokers
}

// ─────────────────────────────────────────────────────────────────────────────
// App builder
// ─────────────────────────────────────────────────────────────────────────────

struct TestHarness {
    pod_repo:  InMemoryPodRepo,
    otp_repo:  InMemoryOtpRepo,
    storage:   MockStorage,
    sms:       MockSms,
    tenant_id: Uuid,
    driver_id: Uuid,
    /// Delivery-PIN policy under test. The default makes the PIN mandatory at
    /// submit; most tests here exercise the evidence rules and switch it off.
    pin_policy: DeliveryPinPolicy,
}

impl TestHarness {
    fn new() -> Self {
        Self {
            pod_repo:  Default::default(),
            otp_repo:  Default::default(),
            storage:   Default::default(),
            sms:       Default::default(),
            tenant_id: Uuid::new_v4(),
            driver_id: Uuid::new_v4(),
            pin_policy: DeliveryPinPolicy { required: false, ..Default::default() },
        }
    }

    fn with_pin_required(mut self) -> Self {
        self.pin_policy = DeliveryPinPolicy::default();
        self
    }

    fn build_app(&self) -> Router {
        let kafka = Arc::new(KafkaProducer::new(&mock_brokers()).expect("kafka producer"));
        let svc = Arc::new(PodService::new(
            Arc::new(self.pod_repo.clone()),
            Arc::new(self.otp_repo.clone()),
            Arc::new(InMemoryPickupRepo::default()),
            Arc::new(InMemoryTelemetry::default()),
            Arc::new(self.storage.clone()),
            Arc::new(self.storage.clone()),
            Arc::new(self.sms.clone()),
            kafka,
            Some(Arc::new(BookingSource)),
            self.pin_policy.clone(),
        ));
        let jwt_svc = Arc::new(JwtService::new(JWT_SECRET, 3600, 86400));
        let state = Arc::new(AppState { pod_service: svc, jwt: jwt_svc });
        // `router` mounts `require_auth` over the /v1 routes itself, as bootstrap uses it.
        router(state)
    }

    fn make_jwt_as(&self, user_id: Uuid, roles: &[&str]) -> String {
        let svc = JwtService::new(JWT_SECRET, 3600, 86400);
        let claims = Claims::new(
            user_id,
            self.tenant_id,
            "test".into(),
            "starter".into(),
            "driver@example.com".into(),
            roles.iter().map(|r| r.to_string()).collect(),
            vec!["pod:write".into(), "pod:read".into()],
            3600,
        );
        svc.issue_access_token(claims).unwrap()
    }

    fn make_jwt(&self) -> String { self.make_jwt_as(self.driver_id, &["driver"]) }

    fn bearer(&self) -> String { format!("Bearer {}", self.make_jwt()) }
    fn jbody(v: &Value) -> Body { Body::from(serde_json::to_vec(v).unwrap()) }

    async fn call(&self, req: Request<Body>) -> (StatusCode, Value) {
        let app = self.build_app();
        let r = app.oneshot(req).await.unwrap();
        let s = r.status();
        let b = axum::body::to_bytes(r.into_body(), usize::MAX).await.unwrap();
        let v: Value = serde_json::from_slice(&b).unwrap_or(Value::Null);
        (s, v)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /v1/pods  (initiate)
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn initiate_pod_within_geofence() {
    let h = TestHarness::new();
    // Driver at exactly the delivery address — geofence verified
    let payload = serde_json::json!({
        "shipment_id": Uuid::new_v4(),
        "task_id": Uuid::new_v4(),
        "recipient_name": "Juan dela Cruz",
        "capture_lat": 14.5995,
        "capture_lng": 120.9842,
        "delivery_lat": 14.5995,
        "delivery_lng": 120.9842
    });
    let req = Request::builder().method(Method::POST).uri("/v1/pods")
        .header(header::AUTHORIZATION, h.bearer())
        .header(header::CONTENT_TYPE, "application/json")
        .body(TestHarness::jbody(&payload)).unwrap();
    let (s, b) = h.call(req).await;
    assert_eq!(s, StatusCode::OK);
    assert!(b["data"]["pod_id"].is_string());
    assert_eq!(b["data"]["geofence_verified"], true);
    assert_eq!(b["data"]["status"], "draft");
}

#[tokio::test]
async fn initiate_pod_outside_geofence() {
    let h = TestHarness::new();
    // Driver ~5km away from delivery address
    let payload = serde_json::json!({
        "shipment_id": Uuid::new_v4(),
        "task_id": Uuid::new_v4(),
        "recipient_name": "Juan dela Cruz",
        "capture_lat": 14.5995,
        "capture_lng": 120.9842,
        "delivery_lat": 14.6500,  // ~5km north
        "delivery_lng": 120.9842
    });
    let req = Request::builder().method(Method::POST).uri("/v1/pods")
        .header(header::AUTHORIZATION, h.bearer())
        .header(header::CONTENT_TYPE, "application/json")
        .body(TestHarness::jbody(&payload)).unwrap();
    let (s, b) = h.call(req).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["data"]["geofence_verified"], false);
}

#[tokio::test]
async fn initiate_pod_idempotent_for_same_shipment() {
    let h = TestHarness::new();
    let sid = Uuid::new_v4();
    let payload = serde_json::json!({
        "shipment_id": sid, "task_id": Uuid::new_v4(),
        "recipient_name": "Maria Santos",
        "capture_lat": 14.5995, "capture_lng": 120.9842,
        "delivery_lat": 14.5995, "delivery_lng": 120.9842
    });

    // First call
    let req1 = Request::builder().method(Method::POST).uri("/v1/pods")
        .header(header::AUTHORIZATION, h.bearer())
        .header(header::CONTENT_TYPE, "application/json")
        .body(TestHarness::jbody(&payload)).unwrap();
    let (s1, b1) = h.call(req1).await;
    assert_eq!(s1, StatusCode::OK);
    let pod_id_1 = b1["data"]["pod_id"].as_str().unwrap().to_string();

    // Second call for same shipment — returns existing POD
    let req2 = Request::builder().method(Method::POST).uri("/v1/pods")
        .header(header::AUTHORIZATION, h.bearer())
        .header(header::CONTENT_TYPE, "application/json")
        .body(TestHarness::jbody(&payload)).unwrap();
    let (s2, b2) = h.call(req2).await;
    assert_eq!(s2, StatusCode::OK);
    assert_eq!(b2["data"]["pod_id"].as_str().unwrap(), pod_id_1);
    // Only one POD in the repo
    assert_eq!(h.pod_repo.store.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn initiate_pod_requires_auth() {
    let h = TestHarness::new();
    let payload = serde_json::json!({
        "shipment_id": Uuid::new_v4(), "task_id": Uuid::new_v4(),
        "recipient_name": "Test", "capture_lat": 14.5, "capture_lng": 121.0,
        "delivery_lat": 14.5, "delivery_lng": 121.0
    });
    let req = Request::builder().method(Method::POST).uri("/v1/pods")
        .header(header::CONTENT_TYPE, "application/json")
        .body(TestHarness::jbody(&payload)).unwrap();
    let r = h.build_app().oneshot(req).await.unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
}

// ─────────────────────────────────────────────────────────────────────────────
// PUT /v1/pods/:id/signature
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn attach_signature_to_pod() {
    let h = TestHarness::new();
    let pod = ProofOfDelivery::new(
        h.tenant_id, Uuid::new_v4(), Uuid::new_v4(), h.driver_id,
        "Test".into(), 14.5995, 120.9842, true, false, None, true, true,
    );
    let pod_id = pod.id;
    h.pod_repo.save(&pod).await.unwrap();

    let payload = serde_json::json!({ "signature_data": "data:image/png;base64,abc123" });
    let req = Request::builder().method(Method::PUT)
        .uri(format!("/v1/pods/{}/signature", pod_id))
        .header(header::AUTHORIZATION, h.bearer())
        .header(header::CONTENT_TYPE, "application/json")
        .body(TestHarness::jbody(&payload)).unwrap();
    let (s, _) = h.call(req).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    // Verify it was saved
    let saved = h.pod_repo.find_by_id(h.tenant_id, pod_id).await.unwrap().unwrap();
    assert!(saved.signature_data.is_some());
}

#[tokio::test]
async fn attach_signature_fails_when_pod_not_found() {
    let h = TestHarness::new();
    let payload = serde_json::json!({ "signature_data": "data:image/png;base64,abc" });
    let req = Request::builder().method(Method::PUT)
        .uri(format!("/v1/pods/{}/signature", Uuid::new_v4()))
        .header(header::AUTHORIZATION, h.bearer())
        .header(header::CONTENT_TYPE, "application/json")
        .body(TestHarness::jbody(&payload)).unwrap();
    let (s, _) = h.call(req).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /v1/pods/:id/upload-url
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_upload_url_returns_presigned_url() {
    let h = TestHarness::new();
    let pod = ProofOfDelivery::new(
        h.tenant_id, Uuid::new_v4(), Uuid::new_v4(), h.driver_id,
        "Test".into(), 14.5995, 120.9842, true, false, None, true, true,
    );
    let pod_id = pod.id;
    h.pod_repo.save(&pod).await.unwrap();

    let payload = serde_json::json!({ "content_type": "image/jpeg" });
    let req = Request::builder().method(Method::POST)
        .uri(format!("/v1/pods/{}/upload-url", pod_id))
        .header(header::AUTHORIZATION, h.bearer())
        .header(header::CONTENT_TYPE, "application/json")
        .body(TestHarness::jbody(&payload)).unwrap();
    let (s, b) = h.call(req).await;
    assert_eq!(s, StatusCode::OK);
    assert!(b["data"]["upload_url"].as_str().unwrap().starts_with("https://"));
    assert!(b["data"]["s3_key"].is_string());
}

#[tokio::test]
async fn get_upload_url_rejects_invalid_content_type() {
    let h = TestHarness::new();
    let pod = ProofOfDelivery::new(
        h.tenant_id, Uuid::new_v4(), Uuid::new_v4(), h.driver_id,
        "Test".into(), 14.5995, 120.9842, true, false, None, true, true,
    );
    let pod_id = pod.id;
    h.pod_repo.save(&pod).await.unwrap();

    let payload = serde_json::json!({ "content_type": "application/pdf" });
    let req = Request::builder().method(Method::POST)
        .uri(format!("/v1/pods/{}/upload-url", pod_id))
        .header(header::AUTHORIZATION, h.bearer())
        .header(header::CONTENT_TYPE, "application/json")
        .body(TestHarness::jbody(&payload)).unwrap();
    let (s, _) = h.call(req).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /v1/pods/:id/photos
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn attach_photo_to_pod() {
    let h = TestHarness::new();
    let pod = ProofOfDelivery::new(
        h.tenant_id, Uuid::new_v4(), Uuid::new_v4(), h.driver_id,
        "Test".into(), 14.5995, 120.9842, true, false, None, true, true,
    );
    let pod_id = pod.id;
    h.pod_repo.save(&pod).await.unwrap();

    let payload = serde_json::json!({
        "pod_id": pod_id,
        "s3_key": "pod/test/photo.jpg",
        "content_type": "image/jpeg",
        "size_bytes": 204800
    });
    let req = Request::builder().method(Method::POST)
        .uri(format!("/v1/pods/{}/photos", pod_id))
        .header(header::AUTHORIZATION, h.bearer())
        .header(header::CONTENT_TYPE, "application/json")
        .body(TestHarness::jbody(&payload)).unwrap();
    let (s, _) = h.call(req).await;
    assert_eq!(s, StatusCode::NO_CONTENT);

    let saved = h.pod_repo.find_by_id(h.tenant_id, pod_id).await.unwrap().unwrap();
    assert_eq!(saved.photos.len(), 1);
    assert_eq!(saved.photos[0].content_type, "image/jpeg");
}

// ─────────────────────────────────────────────────────────────────────────────
// PUT /v1/pods/:id/submit
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn submit_pod_with_signature() {
    let h = TestHarness::new();
    let mut pod = ProofOfDelivery::new(
        h.tenant_id, Uuid::new_v4(), Uuid::new_v4(), h.driver_id,
        "Test".into(), 14.5995, 120.9842, true, false, None, true, true,
    );
    pod.attach_signature("data:image/png;base64,signaturedata".into());
    let pod_id = pod.id;
    h.pod_repo.save(&pod).await.unwrap();

    let payload = serde_json::json!({
        "pod_id": pod_id,
        "cod_collected_cents": null,
        "otp_code": null
    });
    let req = Request::builder().method(Method::PUT)
        .uri(format!("/v1/pods/{}/submit", pod_id))
        .header(header::AUTHORIZATION, h.bearer())
        .header(header::CONTENT_TYPE, "application/json")
        .body(TestHarness::jbody(&payload)).unwrap();
    let (s, b) = h.call(req).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["data"]["status"], "submitted");
}

#[tokio::test]
async fn submit_pod_fails_without_evidence() {
    let h = TestHarness::new();
    let pod = ProofOfDelivery::new(
        h.tenant_id, Uuid::new_v4(), Uuid::new_v4(), h.driver_id,
        "Test".into(), 14.5995, 120.9842, true, false, None, true, true,
    );
    let pod_id = pod.id;
    h.pod_repo.save(&pod).await.unwrap();

    let payload = serde_json::json!({
        "pod_id": pod_id, "cod_collected_cents": null, "otp_code": null
    });
    let req = Request::builder().method(Method::PUT)
        .uri(format!("/v1/pods/{}/submit", pod_id))
        .header(header::AUTHORIZATION, h.bearer())
        .header(header::CONTENT_TYPE, "application/json")
        .body(TestHarness::jbody(&payload)).unwrap();
    let (s, _) = h.call(req).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn submit_pod_with_cod_collection() {
    let h = TestHarness::new();
    let mut pod = ProofOfDelivery::new(
        h.tenant_id, Uuid::new_v4(), Uuid::new_v4(), h.driver_id,
        "Test".into(), 14.5995, 120.9842, true, false, None, true, true,
    );
    pod.attach_signature("sig".into());
    let pod_id = pod.id;
    h.pod_repo.save(&pod).await.unwrap();

    let payload = serde_json::json!({
        "pod_id": pod_id,
        "cod_collected_cents": 25000,
        "otp_code": null
    });
    let req = Request::builder().method(Method::PUT)
        .uri(format!("/v1/pods/{}/submit", pod_id))
        .header(header::AUTHORIZATION, h.bearer())
        .header(header::CONTENT_TYPE, "application/json")
        .body(TestHarness::jbody(&payload)).unwrap();
    let (s, _) = h.call(req).await;
    assert_eq!(s, StatusCode::OK);
    let saved = h.pod_repo.find_by_id(h.tenant_id, pod_id).await.unwrap().unwrap();
    assert_eq!(saved.cod_collected_cents, Some(25000));
    assert_eq!(saved.status, PodStatus::Submitted);
}

#[tokio::test]
async fn submit_pod_fails_when_driver_mismatch() {
    let h = TestHarness::new();
    let other_driver = Uuid::new_v4();
    let mut pod = ProofOfDelivery::new(
        h.tenant_id, Uuid::new_v4(), Uuid::new_v4(),
        other_driver,  // different driver
        "Test".into(), 14.5995, 120.9842, true, false, None, true, true,
    );
    pod.attach_signature("sig".into());
    let pod_id = pod.id;
    h.pod_repo.save(&pod).await.unwrap();

    let payload = serde_json::json!({
        "pod_id": pod_id, "cod_collected_cents": null, "otp_code": null
    });
    let req = Request::builder().method(Method::PUT)
        .uri(format!("/v1/pods/{}/submit", pod_id))
        .header(header::AUTHORIZATION, h.bearer())  // h.driver_id != other_driver
        .header(header::CONTENT_TYPE, "application/json")
        .body(TestHarness::jbody(&payload)).unwrap();
    let (s, _) = h.call(req).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /v1/otps/generate
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn generate_otp_sends_sms_and_returns_otp_id() {
    let h = TestHarness::new();
    let sid = Uuid::new_v4();
    let payload = serde_json::json!({
        "shipment_id": sid,
        // Ignored: the PIN goes to the phone on the booking, never a caller-supplied one.
        "recipient_phone": "+639990000000"
    });
    let req = Request::builder().method(Method::POST).uri("/v1/otps/generate")
        .header(header::AUTHORIZATION, h.bearer())
        .header(header::CONTENT_TYPE, "application/json")
        .body(TestHarness::jbody(&payload)).unwrap();
    let (s, b) = h.call(req).await;
    assert_eq!(s, StatusCode::OK);
    assert!(b["data"]["otp_id"].is_string());

    // SMS was sent
    let sent = h.sms.sent.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, BOOKING_PHONE);
    assert!(sent[0].1.contains("LogisticOS delivery PIN"));
}

#[tokio::test]
async fn generate_otp_persisted_in_repo() {
    let h = TestHarness::new();
    let sid = Uuid::new_v4();
    let payload = serde_json::json!({
        "shipment_id": sid,
        "recipient_phone": "+639181234567"
    });
    let req = Request::builder().method(Method::POST).uri("/v1/otps/generate")
        .header(header::AUTHORIZATION, h.bearer())
        .header(header::CONTENT_TYPE, "application/json")
        .body(TestHarness::jbody(&payload)).unwrap();
    let (s, _) = h.call(req).await;
    assert_eq!(s, StatusCode::OK);

    let active_otp = h.otp_repo.find_active_by_shipment(sid, h.tenant_id).await.unwrap();
    assert!(active_otp.is_some());
    assert!(!active_otp.unwrap().is_used);
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /v1/pods/:id
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_pod_returns_pod_data() {
    let h = TestHarness::new();
    let pod = ProofOfDelivery::new(
        h.tenant_id, Uuid::new_v4(), Uuid::new_v4(), h.driver_id,
        "Test".into(), 14.5995, 120.9842, true, false, None, true, true,
    );
    let pod_id = pod.id;
    h.pod_repo.save(&pod).await.unwrap();

    let req = Request::builder().method(Method::GET)
        .uri(format!("/v1/pods/{}", pod_id))
        .header(header::AUTHORIZATION, h.bearer())
        .body(Body::empty()).unwrap();
    let (s, b) = h.call(req).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["data"]["pod_id"], pod_id.to_string());
}

// ─────────────────────────────────────────────────────────────────────────────
// Health check (no auth required)
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn health_check_no_auth_required() {
    let h = TestHarness::new();
    let req = Request::builder().method(Method::GET).uri("/health")
        .body(Body::empty()).unwrap();
    let r = h.build_app().oneshot(req).await.unwrap();
    assert_eq!(r.status(), StatusCode::OK);
}

// ─────────────────────────────────────────────────────────────────────────────
// Delivery PIN policy (required by default)
// ─────────────────────────────────────────────────────────────────────────────

fn submit_req(h: &TestHarness, pod_id: Uuid, otp: Value) -> Request<Body> {
    let payload = serde_json::json!({ "pod_id": pod_id, "cod_collected_cents": null, "otp_code": otp });
    Request::builder().method(Method::PUT)
        .uri(format!("/v1/pods/{}/submit", pod_id))
        .header(header::AUTHORIZATION, h.bearer())
        .header(header::CONTENT_TYPE, "application/json")
        .body(TestHarness::jbody(&payload)).unwrap()
}

#[tokio::test]
async fn submit_pod_is_refused_without_a_pin_when_the_policy_requires_one() {
    let h = TestHarness::new().with_pin_required();
    let mut pod = ProofOfDelivery::new(
        h.tenant_id, Uuid::new_v4(), Uuid::new_v4(), h.driver_id,
        "Test".into(), 14.5995, 120.9842, true, false, None, true, true,
    );
    pod.attach_signature("sig".into());
    let pod_id = pod.id;
    h.pod_repo.save(&pod).await.unwrap();

    let (s, _) = h.call(submit_req(&h, pod_id, Value::Null)).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    let saved = h.pod_repo.find_by_id(h.tenant_id, pod_id).await.unwrap().unwrap();
    assert_eq!(saved.status, PodStatus::Draft, "a refused submit must leave the POD a draft");
}

#[tokio::test]
async fn submit_pod_accepts_the_pin_issued_for_the_shipment() {
    let h = TestHarness::new().with_pin_required();
    let sid = Uuid::new_v4();
    let mut pod = ProofOfDelivery::new(
        h.tenant_id, sid, Uuid::new_v4(), h.driver_id,
        "Test".into(), 14.5995, 120.9842, true, false, None, true, true,
    );
    pod.attach_signature("sig".into());
    let pod_id = pod.id;
    h.pod_repo.save(&pod).await.unwrap();

    // The code is shown only to non-driver roles; an admin issues it.
    let admin = h.make_jwt_as(Uuid::new_v4(), &["admin"]);
    let gen = Request::builder().method(Method::POST).uri("/v1/otps/generate")
        .header(header::AUTHORIZATION, format!("Bearer {admin}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(TestHarness::jbody(&serde_json::json!({ "shipment_id": sid }))).unwrap();
    let (s, b) = h.call(gen).await;
    assert_eq!(s, StatusCode::OK);
    let code = b["data"]["code"].as_str().expect("admin sees the code").to_string();

    let (s, _) = h.call(submit_req(&h, pod_id, Value::String("000000".into()))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "a wrong PIN must be refused");

    let (s, b) = h.call(submit_req(&h, pod_id, Value::String(code))).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["data"]["status"], "submitted");
    let saved = h.pod_repo.find_by_id(h.tenant_id, pod_id).await.unwrap().unwrap();
    assert!(saved.otp_verified);
}
