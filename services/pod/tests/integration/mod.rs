// Integration tests for the Proof of Delivery (POD) service.
//
// Strategy: Mock all external dependencies (repos, storage, SMS, Kafka).
// Tests exercise the real PodService logic end-to-end without external I/O.
// HTTP layer tests use the real Axum router with mock state injected.

use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use async_trait::async_trait;
use axum::{body::Body, http::{header, Method, Request, StatusCode}, Router};
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;
use logisticos_auth::{claims::Claims, jwt::JwtService};
use logisticos_types::{DriverId, TenantId};

use logisticos_pod::{
    api::http::{router, AppState},
    application::services::PodService,
    domain::{
        entities::{OtpCode, ProofOfDelivery, PodStatus},
        repositories::{OtpRepository, PodRepository},
    },
    infrastructure::external::{
        storage::StorageAdapter,
        sms::SmsAdapter,
    },
};
use logisticos_events::producer::KafkaProducer;

// ─────────────────────────────────────────────────────────────────────────────
// Mock repositories
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Default, Clone)]
struct InMemoryPodRepo {
    store: Arc<Mutex<HashMap<Uuid, ProofOfDelivery>>>,
}

#[async_trait]
impl PodRepository for InMemoryPodRepo {
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<ProofOfDelivery>> {
        Ok(self.store.lock().unwrap().get(&id).cloned())
    }
    async fn find_by_shipment(&self, sid: Uuid) -> anyhow::Result<Option<ProofOfDelivery>> {
        Ok(self.store.lock().unwrap().values()
            .find(|p| p.shipment_id == sid).cloned())
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
    async fn find_active_by_shipment(&self, sid: Uuid) -> anyhow::Result<Option<OtpCode>> {
        let now = chrono::Utc::now();
        Ok(self.store.lock().unwrap().values()
            .find(|o| o.shipment_id == sid && !o.is_used && o.expires_at > now)
            .cloned())
    }
    async fn save(&self, otp: &OtpCode) -> anyhow::Result<()> {
        self.store.lock().unwrap().insert(otp.id, otp.clone());
        Ok(())
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
    async fn presign_upload(&self, key: &str, _ct: &str, _ttl: u32) -> anyhow::Result<String> {
        let url = format!("https://s3.test/{}", key);
        self.upload_urls.lock().unwrap().push(url.clone());
        Ok(url)
    }
    async fn presign_download(&self, key: &str, _ttl: u32) -> anyhow::Result<String> {
        Ok(format!("https://s3.test/dl/{}", key))
    }
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
        }
    }

    fn build_app(&self) -> Router {
        let kafka = Arc::new(KafkaProducer::noop());
        let svc = Arc::new(PodService::new(
            Arc::new(self.pod_repo.clone()),
            Arc::new(self.otp_repo.clone()),
            Arc::new(self.storage.clone()),
            Arc::new(self.sms.clone()),
            kafka,
        ));
        let jwt_svc = Arc::new(JwtService::new("test-secret-key-for-logisticos-testing"));
        let state = Arc::new(AppState { pod_service: svc, jwt: jwt_svc.clone() });
        router(state)
    }

    fn make_jwt(&self) -> String {
        let svc = JwtService::new("test-secret-key-for-logisticos-testing");
        let claims = Claims::new(self.driver_id, self.tenant_id,
            vec!["pod:write".into(), "pod:read".into()]);
        svc.encode(&claims).unwrap()
    }

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
        "Test".into(), 14.5995, 120.9842, true,
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
    let saved = h.pod_repo.find_by_id(pod_id).await.unwrap().unwrap();
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
        "Test".into(), 14.5995, 120.9842, true,
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
        "Test".into(), 14.5995, 120.9842, true,
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
        "Test".into(), 14.5995, 120.9842, true,
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

    let saved = h.pod_repo.find_by_id(pod_id).await.unwrap().unwrap();
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
        "Test".into(), 14.5995, 120.9842, true,
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
        "Test".into(), 14.5995, 120.9842, true,
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
        "Test".into(), 14.5995, 120.9842, true,
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
    let saved = h.pod_repo.find_by_id(pod_id).await.unwrap().unwrap();
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
        "Test".into(), 14.5995, 120.9842, true,
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
        "recipient_phone": "+639171234567"
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
    assert_eq!(sent[0].0, "+639171234567");
    assert!(sent[0].1.contains("LogisticOS delivery code"));
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

    let active_otp = h.otp_repo.find_active_by_shipment(sid).await.unwrap();
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
        "Test".into(), 14.5995, 120.9842, true,
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
