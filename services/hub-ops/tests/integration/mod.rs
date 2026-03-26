// ============================================================================
// Integration tests for the Hub Operations service.
//
// Strategy:
//   - Build a real Axum router wired to in-memory mock repositories.
//   - Issue a real JWT signed with the test secret so all permission checks pass.
//   - Send requests via tower::ServiceExt::oneshot (no network).
//   - Assert HTTP status codes AND JSON response fields.
//
// The mock repositories use Arc<Mutex<HashMap<Uuid, T>>> for lock-safe
// in-memory storage that mirrors what the DB implementations would do.
// ============================================================================

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

use logisticos_auth::{claims::Claims, jwt::JwtService};
use logisticos_hub_ops::{
    api::http::{AppState, router},
    application::services::{
        HubRepository, HubService, InductionRepository,
    },
    domain::entities::{Hub, HubId, InductionId, InductionStatus, ParcelInduction},
};
use logisticos_types::TenantId;

// ─────────────────────────────────────────────────────────────────────────────
// Mock repositories
// ─────────────────────────────────────────────────────────────────────────────

struct MockHubRepository {
    store: Mutex<HashMap<Uuid, Hub>>,
}

impl MockHubRepository {
    fn new() -> Arc<Self> {
        Arc::new(Self { store: Mutex::new(HashMap::new()) })
    }

    fn new_with(hubs: Vec<Hub>) -> Arc<Self> {
        let mut map = HashMap::new();
        for h in hubs {
            map.insert(h.id.inner(), h);
        }
        Arc::new(Self { store: Mutex::new(map) })
    }
}

#[async_trait::async_trait]
impl HubRepository for MockHubRepository {
    async fn find_by_id(&self, id: &HubId) -> anyhow::Result<Option<Hub>> {
        let store = self.store.lock().unwrap();
        Ok(store.get(&id.inner()).cloned())
    }

    async fn list(&self, tenant_id: &TenantId) -> anyhow::Result<Vec<Hub>> {
        let store = self.store.lock().unwrap();
        Ok(store
            .values()
            .filter(|h| h.tenant_id.inner() == tenant_id.inner())
            .cloned()
            .collect())
    }

    async fn save(&self, hub: &Hub) -> anyhow::Result<()> {
        let mut store = self.store.lock().unwrap();
        store.insert(hub.id.inner(), hub.clone());
        Ok(())
    }
}

struct MockInductionRepository {
    store: Mutex<HashMap<Uuid, ParcelInduction>>,
}

impl MockInductionRepository {
    fn new() -> Arc<Self> {
        Arc::new(Self { store: Mutex::new(HashMap::new()) })
    }
}

#[async_trait::async_trait]
impl InductionRepository for MockInductionRepository {
    async fn find_by_id(&self, id: &InductionId) -> anyhow::Result<Option<ParcelInduction>> {
        let store = self.store.lock().unwrap();
        Ok(store.get(&id.inner()).cloned())
    }

    async fn find_by_shipment(&self, shipment_id: Uuid) -> anyhow::Result<Option<ParcelInduction>> {
        let store = self.store.lock().unwrap();
        Ok(store.values().find(|p| p.shipment_id == shipment_id).cloned())
    }

    async fn list_active(&self, hub_id: &HubId) -> anyhow::Result<Vec<ParcelInduction>> {
        let store = self.store.lock().unwrap();
        Ok(store
            .values()
            .filter(|p| {
                p.hub_id.inner() == hub_id.inner()
                    && p.status != InductionStatus::Dispatched
            })
            .cloned()
            .collect())
    }

    async fn save(&self, induction: &ParcelInduction) -> anyhow::Result<()> {
        let mut store = self.store.lock().unwrap();
        store.insert(induction.id.inner(), induction.clone());
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// JWT / auth helpers
// ─────────────────────────────────────────────────────────────────────────────

const TEST_SECRET: &str = "test-secret-key-for-logisticos-testing";

fn jwt_service() -> JwtService {
    JwtService::new(TEST_SECRET, 3600, 86400)
}

/// Issue a test token that carries all hub-ops relevant permissions.
fn make_token(tenant_id: Uuid) -> String {
    let claims = Claims::new(
        Uuid::new_v4(),
        tenant_id,
        "test-tenant".into(),
        "enterprise".into(),
        "ops@test.local".into(),
        vec!["admin".into()],
        vec![
            "fleet:read".into(),
            "fleet:manage".into(),
            "shipments:read".into(),
            "shipments:update".into(),
            "shipments:create".into(),
        ],
        3600,
    );
    jwt_service().issue_access_token(claims).expect("token issuance must succeed")
}

// ─────────────────────────────────────────────────────────────────────────────
// App factory
// ─────────────────────────────────────────────────────────────────────────────

struct TestApp {
    tenant_id: Uuid,
    hub_repo: Arc<MockHubRepository>,
    induction_repo: Arc<MockInductionRepository>,
    router: axum::Router,
}

impl TestApp {
    fn new() -> Self {
        let tenant_id = Uuid::new_v4();
        let hub_repo = MockHubRepository::new();
        let induction_repo = MockInductionRepository::new();
        let hub_svc = Arc::new(HubService::new(hub_repo.clone(), induction_repo.clone()));
        let state = AppState { hub_svc };
        let router = router(state);
        Self { tenant_id, hub_repo, induction_repo, router }
    }

    fn token(&self) -> String {
        make_token(self.tenant_id)
    }

    fn auth_header(&self) -> (header::HeaderName, String) {
        (header::AUTHORIZATION, format!("Bearer {}", self.token()))
    }

    async fn send(&self, req: Request<Body>) -> (StatusCode, Value) {
        let resp = self.router.clone().oneshot(req).await.expect("handler must not panic");
        let status = resp.status();
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("body read must not fail");
        let json: Value = serde_json::from_slice(&body_bytes)
            .unwrap_or_else(|_| json!({ "raw": String::from_utf8_lossy(&body_bytes).to_string() }));
        (status, json)
    }

    fn get(&self, path: &str) -> Request<Body> {
        let (name, value) = self.auth_header();
        Request::builder()
            .method(Method::GET)
            .uri(path)
            .header(name, value)
            .body(Body::empty())
            .unwrap()
    }

    fn post(&self, path: &str, body: Value) -> Request<Body> {
        let (name, value) = self.auth_header();
        Request::builder()
            .method(Method::POST)
            .uri(path)
            .header(name, value)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    fn patch(&self, path: &str, body: Value) -> Request<Body> {
        let (name, value) = self.auth_header();
        Request::builder()
            .method(Method::PATCH)
            .uri(path)
            .header(name, value)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers: seed a hub that belongs to this tenant
// ─────────────────────────────────────────────────────────────────────────────

fn seed_hub(app: &TestApp, name: &str, capacity: u32) -> Hub {
    let hub = Hub::new(
        TenantId::from_uuid(app.tenant_id),
        name.into(),
        "Test Address, Manila".into(),
        14.5547,
        121.0244,
        capacity,
    );
    app.hub_repo.store.lock().unwrap().insert(hub.id.inner(), hub.clone());
    hub
}

fn seed_inducted_parcel(app: &TestApp, hub: &Hub) -> ParcelInduction {
    let induction = ParcelInduction::new(
        hub.id.clone(),
        hub.tenant_id.clone(),
        Uuid::new_v4(),
        format!("LSPH{:010}", rand_u32()),
        None,
    );
    // Also increment hub load so state is consistent
    app.hub_repo.store.lock().unwrap().get_mut(&hub.id.inner()).map(|h| {
        h.current_load += 1;
    });
    app.induction_repo.store.lock().unwrap().insert(induction.id.inner(), induction.clone());
    induction
}

fn rand_u32() -> u32 {
    // Simple deterministic counter substitute for tests (not crypto-random)
    use std::sync::atomic::{AtomicU32, Ordering};
    static CTR: AtomicU32 = AtomicU32::new(10_000_000);
    CTR.fetch_add(1, Ordering::Relaxed)
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /v1/hubs — list hubs for tenant
// ─────────────────────────────────────────────────────────────────────────────

mod list_hubs {
    use super::*;

    #[tokio::test]
    async fn returns_200_with_empty_list_when_no_hubs() {
        let app = TestApp::new();
        let (status, body) = app.send(app.get("/v1/hubs")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["count"], 0);
        assert!(body["hubs"].is_array());
    }

    #[tokio::test]
    async fn returns_only_hubs_belonging_to_tenant() {
        let app = TestApp::new();
        seed_hub(&app, "Hub A", 100);
        seed_hub(&app, "Hub B", 200);

        // Hub owned by a different tenant — must NOT appear in results
        let other_tenant_hub = Hub::new(
            TenantId::new(), // different tenant
            "Other Tenant Hub".into(),
            "Somewhere".into(),
            0.0, 0.0, 50,
        );
        app.hub_repo.store.lock().unwrap().insert(other_tenant_hub.id.inner(), other_tenant_hub);

        let (status, body) = app.send(app.get("/v1/hubs")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["count"], 2, "Must return only 2 hubs for this tenant");
    }

    #[tokio::test]
    async fn returned_hubs_contain_expected_fields() {
        let app = TestApp::new();
        seed_hub(&app, "Makati Hub", 300);

        let (status, body) = app.send(app.get("/v1/hubs")).await;
        assert_eq!(status, StatusCode::OK);

        let hub = &body["hubs"][0];
        assert!(hub["id"].is_string());
        assert_eq!(hub["name"], "Makati Hub");
        assert_eq!(hub["capacity"], 300);
        assert_eq!(hub["current_load"], 0);
        assert_eq!(hub["is_active"], true);
    }

    #[tokio::test]
    async fn returns_401_without_auth_header() {
        let app = TestApp::new();
        let req = Request::builder()
            .method(Method::GET)
            .uri("/v1/hubs")
            .body(Body::empty())
            .unwrap();
        let (status, _) = app.send(req).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn returns_401_with_invalid_token() {
        let app = TestApp::new();
        let req = Request::builder()
            .method(Method::GET)
            .uri("/v1/hubs")
            .header(header::AUTHORIZATION, "Bearer this.is.not.a.valid.jwt")
            .body(Body::empty())
            .unwrap();
        let (status, _) = app.send(req).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /v1/hubs — create hub
// ─────────────────────────────────────────────────────────────────────────────

mod create_hub {
    use super::*;

    fn create_payload(name: &str, capacity: u32) -> Value {
        json!({
            "name":          name,
            "address":       "1 Ayala Ave, Makati City, Metro Manila",
            "lat":           14.5547,
            "lng":           121.0244,
            "capacity":      capacity,
            "serving_zones": ["ZONE-A", "ZONE-B"]
        })
    }

    #[tokio::test]
    async fn returns_201_with_created_hub() {
        let app = TestApp::new();
        let (status, body) = app.send(app.post("/v1/hubs", create_payload("New Hub", 250))).await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(body["name"], "New Hub");
        assert_eq!(body["capacity"], 250);
    }

    #[tokio::test]
    async fn created_hub_has_zero_initial_load() {
        let app = TestApp::new();
        let (_, body) = app.send(app.post("/v1/hubs", create_payload("Load Hub", 100))).await;
        assert_eq!(body["current_load"], 0);
    }

    #[tokio::test]
    async fn created_hub_is_active() {
        let app = TestApp::new();
        let (_, body) = app.send(app.post("/v1/hubs", create_payload("Active Hub", 100))).await;
        assert_eq!(body["is_active"], true);
    }

    #[tokio::test]
    async fn created_hub_has_serving_zones() {
        let app = TestApp::new();
        let (_, body) = app.send(app.post("/v1/hubs", create_payload("Zone Hub", 100))).await;
        let zones = body["serving_zones"].as_array().expect("serving_zones must be an array");
        assert_eq!(zones.len(), 2);
    }

    #[tokio::test]
    async fn created_hub_appears_in_list() {
        let app = TestApp::new();
        app.send(app.post("/v1/hubs", create_payload("Listed Hub", 100))).await;

        let (status, body) = app.send(app.get("/v1/hubs")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["count"], 1);
        assert_eq!(body["hubs"][0]["name"], "Listed Hub");
    }

    #[tokio::test]
    async fn returns_400_with_missing_required_fields() {
        let app = TestApp::new();
        let (status, _) = app.send(app.post("/v1/hubs", json!({ "name": "Incomplete" }))).await;
        // Axum JSON extractor returns 422 for missing fields
        assert!(
            status == StatusCode::UNPROCESSABLE_ENTITY || status == StatusCode::BAD_REQUEST,
            "Missing required fields must return 4xx, got {}",
            status
        );
    }

    #[tokio::test]
    async fn created_hub_stores_coordinates() {
        let app = TestApp::new();
        let (_, body) = app.send(app.post(
            "/v1/hubs",
            json!({
                "name":          "Coords Hub",
                "address":       "Test",
                "lat":           14.5547,
                "lng":           121.0244,
                "capacity":      100,
                "serving_zones": []
            }),
        ))
        .await;
        assert!((body["lat"].as_f64().unwrap() - 14.5547).abs() < 1e-4);
        assert!((body["lng"].as_f64().unwrap() - 121.0244).abs() < 1e-4);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /v1/hubs/{id} — get hub, 404 when missing
// ─────────────────────────────────────────────────────────────────────────────

mod get_hub {
    use super::*;

    #[tokio::test]
    async fn returns_200_with_hub_when_found() {
        let app = TestApp::new();
        let hub = seed_hub(&app, "Target Hub", 100);
        let path = format!("/v1/hubs/{}", hub.id.inner());
        let (status, body) = app.send(app.get(&path)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["id"].as_str().unwrap(), hub.id.inner().to_string());
        assert_eq!(body["name"], "Target Hub");
    }

    #[tokio::test]
    async fn returns_404_when_hub_not_found() {
        let app = TestApp::new();
        let missing_id = Uuid::new_v4();
        let path = format!("/v1/hubs/{}", missing_id);
        let (status, body) = app.send(app.get(&path)).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn returns_404_for_hub_belonging_to_other_tenant() {
        let app = TestApp::new();
        // Hub belongs to a different tenant — not visible to this caller
        let other_hub = Hub::new(
            TenantId::new(),
            "Other Hub".into(),
            "Unknown".into(),
            0.0, 0.0, 50,
        );
        let other_id = other_hub.id.inner();
        app.hub_repo.store.lock().unwrap().insert(other_id, other_hub);

        let path = format!("/v1/hubs/{}", other_id);
        let (status, _) = app.send(app.get(&path)).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn returns_404_with_random_uuid_that_doesnt_exist() {
        let app = TestApp::new();
        let path = format!("/v1/hubs/{}", Uuid::new_v4());
        let (status, _) = app.send(app.get(&path)).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn returned_hub_has_all_fields() {
        let app = TestApp::new();
        let hub = seed_hub(&app, "Complete Hub", 500);
        let path = format!("/v1/hubs/{}", hub.id.inner());
        let (_, body) = app.send(app.get(&path)).await;
        assert!(body["id"].is_string());
        assert!(body["name"].is_string());
        assert!(body["address"].is_string());
        assert!(body["capacity"].is_number());
        assert!(body["current_load"].is_number());
        assert!(body["is_active"].is_boolean());
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// PATCH /v1/hubs/{id} — update hub
// NOTE: The current HTTP router does not expose a PATCH /v1/hubs/:id endpoint.
// These tests verify the creation + retrieval roundtrip in lieu of a PATCH
// endpoint, and document expected behaviour if the endpoint is added.
// ─────────────────────────────────────────────────────────────────────────────

mod update_hub {
    use super::*;

    #[tokio::test]
    async fn hub_capacity_reflected_in_get_after_creation() {
        let app = TestApp::new();
        // Create with capacity 100
        let (_, created) = app.send(app.post(
            "/v1/hubs",
            json!({
                "name": "Patch Hub",
                "address": "Test",
                "lat": 14.5547,
                "lng": 121.0244,
                "capacity": 100,
                "serving_zones": []
            }),
        ))
        .await;
        let hub_id = created["id"].as_str().unwrap();
        let path = format!("/v1/hubs/{}", hub_id);
        let (status, body) = app.send(app.get(&path)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["capacity"], 100);
    }

    #[tokio::test]
    async fn hub_serving_zones_stored_correctly() {
        let app = TestApp::new();
        let (_, created) = app.send(app.post(
            "/v1/hubs",
            json!({
                "name": "Zone Hub",
                "address": "Test",
                "lat": 14.5547,
                "lng": 121.0244,
                "capacity": 100,
                "serving_zones": ["NORTH", "SOUTH", "EAST"]
            }),
        ))
        .await;
        let zones = created["serving_zones"].as_array().unwrap();
        assert_eq!(zones.len(), 3);
        let zone_strings: Vec<&str> = zones.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(zone_strings.contains(&"NORTH"));
        assert!(zone_strings.contains(&"SOUTH"));
        assert!(zone_strings.contains(&"EAST"));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /v1/inductions — induct a parcel (increments load)
// ─────────────────────────────────────────────────────────────────────────────

mod create_induction {
    use super::*;

    fn induct_payload(hub_id: Uuid) -> Value {
        json!({
            "hub_id":          hub_id,
            "shipment_id":     Uuid::new_v4(),
            "tracking_number": "LSPH0012345678",
            "inducted_by":     null
        })
    }

    #[tokio::test]
    async fn returns_201_with_induction_record() {
        let app = TestApp::new();
        let hub = seed_hub(&app, "Intake Hub", 100);
        let (status, body) = app.send(app.post("/v1/inductions", induct_payload(hub.id.inner()))).await;
        assert_eq!(status, StatusCode::CREATED);
        assert!(body["id"].is_string());
        assert_eq!(body["status"], "inducted");
    }

    #[tokio::test]
    async fn induction_increments_hub_load() {
        let app = TestApp::new();
        let hub = seed_hub(&app, "Load Hub", 100);

        app.send(app.post("/v1/inductions", induct_payload(hub.id.inner()))).await;

        let path = format!("/v1/hubs/{}/capacity", hub.id.inner());
        let (_, body) = app.send(app.get(&path)).await;
        assert_eq!(body["current_load"], 1, "Inducting one parcel must increment hub load to 1");
    }

    #[tokio::test]
    async fn multiple_inductions_accumulate_load() {
        let app = TestApp::new();
        let hub = seed_hub(&app, "Multi Hub", 100);

        for _ in 0..3 {
            app.send(app.post(
                "/v1/inductions",
                json!({
                    "hub_id":          hub.id.inner(),
                    "shipment_id":     Uuid::new_v4(),
                    "tracking_number": format!("LSPH{:010}", rand_u32()),
                    "inducted_by":     null
                }),
            ))
            .await;
        }

        let path = format!("/v1/hubs/{}/capacity", hub.id.inner());
        let (_, body) = app.send(app.get(&path)).await;
        assert_eq!(body["current_load"], 3);
    }

    #[tokio::test]
    async fn returns_404_when_hub_does_not_exist() {
        let app = TestApp::new();
        let (status, body) = app
            .send(app.post("/v1/inductions", induct_payload(Uuid::new_v4())))
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn induction_is_idempotent_for_same_shipment() {
        let app = TestApp::new();
        let hub = seed_hub(&app, "Idem Hub", 100);
        let shipment_id = Uuid::new_v4();
        let payload = json!({
            "hub_id":          hub.id.inner(),
            "shipment_id":     shipment_id,
            "tracking_number": "LSPH0012345678",
            "inducted_by":     null
        });

        let (s1, b1) = app.send(app.post("/v1/inductions", payload.clone())).await;
        let (s2, b2) = app.send(app.post("/v1/inductions", payload.clone())).await;

        assert_eq!(s1, StatusCode::CREATED);
        // Second induction must be idempotent — same ID returned
        assert!(s2 == StatusCode::CREATED || s2 == StatusCode::OK);
        assert_eq!(b1["id"], b2["id"], "Idempotent induction must return same record ID");
    }

    #[tokio::test]
    async fn induction_returns_correct_hub_id() {
        let app = TestApp::new();
        let hub = seed_hub(&app, "Hub ID Test", 100);
        let (_, body) = app.send(app.post("/v1/inductions", induct_payload(hub.id.inner()))).await;
        assert_eq!(
            body["hub_id"].as_str().unwrap(),
            hub.id.inner().to_string(),
            "Returned induction must reference the correct hub_id"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /v1/inductions/:id — get induction record
// ─────────────────────────────────────────────────────────────────────────────

mod get_induction {
    use super::*;

    #[tokio::test]
    async fn returns_200_with_induction_record() {
        let app = TestApp::new();
        let hub = seed_hub(&app, "Get Hub", 100);
        let induction = seed_inducted_parcel(&app, &hub);
        let path = format!("/v1/inductions/{}", induction.id.inner());
        let (status, body) = app.send(app.get(&path)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["id"].as_str().unwrap(), induction.id.inner().to_string());
    }

    #[tokio::test]
    async fn returns_404_when_induction_not_found() {
        let app = TestApp::new();
        // Seed a hub so the tenant check passes, but no induction exists
        seed_hub(&app, "Empty Hub", 100);
        let path = format!("/v1/inductions/{}", Uuid::new_v4());
        let (status, body) = app.send(app.get(&path)).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn returned_induction_has_inducted_status() {
        let app = TestApp::new();
        let hub = seed_hub(&app, "Status Hub", 100);
        let induction = seed_inducted_parcel(&app, &hub);
        let path = format!("/v1/inductions/{}", induction.id.inner());
        let (_, body) = app.send(app.get(&path)).await;
        assert_eq!(body["status"], "inducted");
    }

    #[tokio::test]
    async fn returned_induction_has_tracking_number() {
        let app = TestApp::new();
        let hub = seed_hub(&app, "Track Hub", 100);
        let induction = seed_inducted_parcel(&app, &hub);
        let path = format!("/v1/inductions/{}", induction.id.inner());
        let (_, body) = app.send(app.get(&path)).await;
        assert!(
            body["tracking_number"].as_str().is_some(),
            "tracking_number must be a string"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /v1/inductions/:id/sort — assign zone + bay
// ─────────────────────────────────────────────────────────────────────────────

mod sort_induction {
    use super::*;

    fn sort_payload(zone: &str, bay: &str) -> Value {
        json!({ "zone": zone, "bay": bay })
    }

    #[tokio::test]
    async fn returns_200_with_sorted_status() {
        let app = TestApp::new();
        let hub = seed_hub(&app, "Sort Hub", 100);
        let induction = seed_inducted_parcel(&app, &hub);
        let path = format!("/v1/inductions/{}/sort", induction.id.inner());
        let (status, body) = app.send(app.post(&path, sort_payload("ZONE-A", "BAY-01"))).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "sorted");
    }

    #[tokio::test]
    async fn sorted_induction_has_zone_set() {
        let app = TestApp::new();
        let hub = seed_hub(&app, "Zone Sort Hub", 100);
        let induction = seed_inducted_parcel(&app, &hub);
        let path = format!("/v1/inductions/{}/sort", induction.id.inner());
        let (_, body) = app.send(app.post(&path, sort_payload("ZONE-NORTH", "BAY-07"))).await;
        assert_eq!(body["zone"], "ZONE-NORTH");
    }

    #[tokio::test]
    async fn sorted_induction_has_bay_set() {
        let app = TestApp::new();
        let hub = seed_hub(&app, "Bay Sort Hub", 100);
        let induction = seed_inducted_parcel(&app, &hub);
        let path = format!("/v1/inductions/{}/sort", induction.id.inner());
        let (_, body) = app.send(app.post(&path, sort_payload("ZONE-A", "BAY-03"))).await;
        assert_eq!(body["bay"], "BAY-03");
    }

    #[tokio::test]
    async fn sorted_induction_has_sorted_at_timestamp() {
        let app = TestApp::new();
        let hub = seed_hub(&app, "Timestamp Hub", 100);
        let induction = seed_inducted_parcel(&app, &hub);
        let path = format!("/v1/inductions/{}/sort", induction.id.inner());
        let (_, body) = app.send(app.post(&path, sort_payload("ZONE-A", "BAY-01"))).await;
        assert!(
            body["sorted_at"].is_string(),
            "sorted_at must be set after sorting, got: {}",
            body["sorted_at"]
        );
    }

    #[tokio::test]
    async fn sort_returns_404_for_unknown_induction() {
        let app = TestApp::new();
        let path = format!("/v1/inductions/{}/sort", Uuid::new_v4());
        let (status, _) = app.send(app.post(&path, sort_payload("ZONE-A", "BAY-01"))).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn sort_returns_422_when_zone_is_empty() {
        let app = TestApp::new();
        let hub = seed_hub(&app, "Empty Zone Hub", 100);
        let induction = seed_inducted_parcel(&app, &hub);
        let path = format!("/v1/inductions/{}/sort", induction.id.inner());
        let (status, _) = app.send(app.post(&path, json!({ "zone": "", "bay": "BAY-01" }))).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn sort_returns_422_when_bay_is_empty() {
        let app = TestApp::new();
        let hub = seed_hub(&app, "Empty Bay Hub", 100);
        let induction = seed_inducted_parcel(&app, &hub);
        let path = format!("/v1/inductions/{}/sort", induction.id.inner());
        let (status, _) = app
            .send(app.post(&path, json!({ "zone": "ZONE-A", "bay": "   " })))
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /v1/inductions/:id/dispatch — mark dispatched
// ─────────────────────────────────────────────────────────────────────────────

mod dispatch_induction {
    use super::*;

    fn sorted_parcel_in(app: &TestApp, hub: &Hub) -> ParcelInduction {
        let mut induction = seed_inducted_parcel(app, hub);
        induction.sort_to("ZONE-A".into(), "BAY-01".into());
        app.induction_repo
            .store
            .lock()
            .unwrap()
            .insert(induction.id.inner(), induction.clone());
        induction
    }

    #[tokio::test]
    async fn returns_200_with_dispatched_status() {
        let app = TestApp::new();
        let hub = seed_hub(&app, "Dispatch Hub", 100);
        let induction = sorted_parcel_in(&app, &hub);
        let path = format!("/v1/inductions/{}/dispatch", induction.id.inner());
        let (status, body) = app.send(app.post(&path, json!({}))).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "dispatched");
    }

    #[tokio::test]
    async fn dispatch_sets_dispatched_at() {
        let app = TestApp::new();
        let hub = seed_hub(&app, "Dispatch Time Hub", 100);
        let induction = sorted_parcel_in(&app, &hub);
        let path = format!("/v1/inductions/{}/dispatch", induction.id.inner());
        let (_, body) = app.send(app.post(&path, json!({}))).await;
        assert!(
            body["dispatched_at"].is_string(),
            "dispatched_at must be set after dispatch, got: {}",
            body["dispatched_at"]
        );
    }

    #[tokio::test]
    async fn dispatch_decrements_hub_load() {
        let app = TestApp::new();
        let hub = seed_hub(&app, "Load Decrement Hub", 100);
        let induction = sorted_parcel_in(&app, &hub);

        // Verify hub load before dispatch
        let cap_path = format!("/v1/hubs/{}/capacity", hub.id.inner());
        let (_, before) = app.send(app.get(&cap_path)).await;
        let load_before = before["current_load"].as_u64().unwrap();

        let disp_path = format!("/v1/inductions/{}/dispatch", induction.id.inner());
        app.send(app.post(&disp_path, json!({}))).await;

        let (_, after) = app.send(app.get(&cap_path)).await;
        let load_after = after["current_load"].as_u64().unwrap();

        assert_eq!(
            load_after,
            load_before.saturating_sub(1),
            "Hub load must decrease by 1 after dispatch"
        );
    }

    #[tokio::test]
    async fn dispatch_returns_404_for_unknown_induction() {
        let app = TestApp::new();
        let path = format!("/v1/inductions/{}/dispatch", Uuid::new_v4());
        let (status, _) = app.send(app.post(&path, json!({}))).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn dispatched_parcel_no_longer_in_active_manifest() {
        let app = TestApp::new();
        let hub = seed_hub(&app, "Manifest Hub", 100);
        let induction = sorted_parcel_in(&app, &hub);

        // Dispatch the parcel
        let disp_path = format!("/v1/inductions/{}/dispatch", induction.id.inner());
        app.send(app.post(&disp_path, json!({}))).await;

        // Check manifest — dispatched parcels are excluded from list_active
        let manifest_path = format!("/v1/hubs/{}/manifest", hub.id.inner());
        let (status, body) = app.send(app.get(&manifest_path)).await;
        assert_eq!(status, StatusCode::OK);
        let parcels = body["parcels"].as_array().unwrap();
        let still_present = parcels
            .iter()
            .any(|p| p["id"].as_str() == Some(&induction.id.inner().to_string()));
        assert!(!still_present, "Dispatched parcel must not appear in the active manifest");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /v1/hubs/{id}/capacity — returns current_load, capacity, pct
// ─────────────────────────────────────────────────────────────────────────────

mod hub_capacity_endpoint {
    use super::*;

    #[tokio::test]
    async fn returns_200_with_capacity_fields() {
        let app = TestApp::new();
        let hub = seed_hub(&app, "Capacity Hub", 200);
        let path = format!("/v1/hubs/{}/capacity", hub.id.inner());
        let (status, body) = app.send(app.get(&path)).await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["capacity"].is_number());
        assert!(body["current_load"].is_number());
        assert!(body["capacity_pct"].is_number());
        assert!(body["is_over_capacity"].is_boolean());
    }

    #[tokio::test]
    async fn reports_zero_pct_for_empty_hub() {
        let app = TestApp::new();
        let hub = seed_hub(&app, "Empty Capacity Hub", 100);
        let path = format!("/v1/hubs/{}/capacity", hub.id.inner());
        let (_, body) = app.send(app.get(&path)).await;
        assert_eq!(body["current_load"], 0);
        assert!((body["capacity_pct"].as_f64().unwrap() - 0.0).abs() < 0.01);
        assert_eq!(body["is_over_capacity"], false);
    }

    #[tokio::test]
    async fn reports_correct_pct_after_inductions() {
        let app = TestApp::new();
        let hub = seed_hub(&app, "Pct Hub", 4);

        // Induct 2 of 4 = 50%
        for _ in 0..2 {
            app.send(app.post(
                "/v1/inductions",
                json!({
                    "hub_id":          hub.id.inner(),
                    "shipment_id":     Uuid::new_v4(),
                    "tracking_number": format!("LSPH{:010}", rand_u32()),
                    "inducted_by":     null
                }),
            ))
            .await;
        }

        let path = format!("/v1/hubs/{}/capacity", hub.id.inner());
        let (_, body) = app.send(app.get(&path)).await;
        assert_eq!(body["current_load"], 2);
        let pct = body["capacity_pct"].as_f64().unwrap();
        assert!((pct - 50.0).abs() < 0.5, "Expected ~50% capacity, got {}", pct);
    }

    #[tokio::test]
    async fn reports_is_over_capacity_true_when_full() {
        let app = TestApp::new();
        let hub = seed_hub(&app, "Full Hub", 1);

        app.send(app.post(
            "/v1/inductions",
            json!({
                "hub_id":          hub.id.inner(),
                "shipment_id":     Uuid::new_v4(),
                "tracking_number": "LSPH0099887766",
                "inducted_by":     null
            }),
        ))
        .await;

        let path = format!("/v1/hubs/{}/capacity", hub.id.inner());
        let (_, body) = app.send(app.get(&path)).await;
        assert_eq!(body["is_over_capacity"], true);
    }

    #[tokio::test]
    async fn returns_404_for_nonexistent_hub() {
        let app = TestApp::new();
        let path = format!("/v1/hubs/{}/capacity", Uuid::new_v4());
        let (status, _) = app.send(app.get(&path)).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn capacity_response_includes_hub_name() {
        let app = TestApp::new();
        let hub = seed_hub(&app, "Named Hub", 100);
        let path = format!("/v1/hubs/{}/capacity", hub.id.inner());
        let (_, body) = app.send(app.get(&path)).await;
        assert_eq!(body["hub_name"], "Named Hub");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 409 when inducting parcel into a full hub
// ─────────────────────────────────────────────────────────────────────────────

mod induction_capacity_enforcement {
    use super::*;

    #[tokio::test]
    async fn returns_422_when_inducing_into_full_hub() {
        // The domain returns BusinessRule error which maps to 422
        let app = TestApp::new();
        let hub = seed_hub(&app, "Capacity Hub", 1);

        // Fill to capacity
        app.send(app.post(
            "/v1/inductions",
            json!({
                "hub_id":          hub.id.inner(),
                "shipment_id":     Uuid::new_v4(),
                "tracking_number": "LSPH0011111111",
                "inducted_by":     null
            }),
        ))
        .await;

        // This one must fail
        let (status, body) = app.send(app.post(
            "/v1/inductions",
            json!({
                "hub_id":          hub.id.inner(),
                "shipment_id":     Uuid::new_v4(),
                "tracking_number": "LSPH0022222222",
                "inducted_by":     null
            }),
        ))
        .await;

        // BusinessRule maps to 422; the test accepts 409 or 422 since callers
        // may treat capacity-exceeded as a conflict or validation error.
        assert!(
            status == StatusCode::UNPROCESSABLE_ENTITY || status == StatusCode::CONFLICT,
            "Inducing into a full hub must return 422 or 409, got {}",
            status
        );
        let error_code = body["error"]["code"].as_str().unwrap_or("");
        assert!(
            error_code == "BUSINESS_RULE_VIOLATION" || error_code == "CONFLICT",
            "Error code must indicate a capacity violation, got: {}",
            error_code
        );
    }

    #[tokio::test]
    async fn capacity_is_enforced_at_exact_limit() {
        let app = TestApp::new();
        let hub = seed_hub(&app, "Boundary Hub", 2);

        for i in 0..2u32 {
            let (status, _) = app.send(app.post(
                "/v1/inductions",
                json!({
                    "hub_id":          hub.id.inner(),
                    "shipment_id":     Uuid::new_v4(),
                    "tracking_number": format!("LSPH00000{:05}", i),
                    "inducted_by":     null
                }),
            ))
            .await;
            assert_eq!(
                status,
                StatusCode::CREATED,
                "Induction {} of 2 must succeed",
                i + 1
            );
        }

        // Third must fail
        let (status, _) = app.send(app.post(
            "/v1/inductions",
            json!({
                "hub_id":          hub.id.inner(),
                "shipment_id":     Uuid::new_v4(),
                "tracking_number": "LSPH0099999999",
                "inducted_by":     null
            }),
        ))
        .await;
        assert!(
            status == StatusCode::UNPROCESSABLE_ENTITY || status == StatusCode::CONFLICT,
            "Third induction into capacity-2 hub must be rejected, got {}",
            status
        );
    }

    #[tokio::test]
    async fn dispatch_then_reinduct_succeeds() {
        // Dispatching a parcel frees capacity — subsequent induction must succeed.
        let app = TestApp::new();
        let hub = seed_hub(&app, "Cycle Hub", 1);

        // Induct and immediately sort
        let (_, ind_body) = app.send(app.post(
            "/v1/inductions",
            json!({
                "hub_id":          hub.id.inner(),
                "shipment_id":     Uuid::new_v4(),
                "tracking_number": "LSPH0011111111",
                "inducted_by":     null
            }),
        ))
        .await;
        let ind_id = ind_body["id"].as_str().unwrap();

        let sort_path = format!("/v1/inductions/{}/sort", ind_id);
        app.send(app.post(&sort_path, json!({ "zone": "ZONE-A", "bay": "BAY-01" }))).await;

        let disp_path = format!("/v1/inductions/{}/dispatch", ind_id);
        app.send(app.post(&disp_path, json!({}))).await;

        // Now re-induct — should succeed
        let (status, _) = app.send(app.post(
            "/v1/inductions",
            json!({
                "hub_id":          hub.id.inner(),
                "shipment_id":     Uuid::new_v4(),
                "tracking_number": "LSPH0022222222",
                "inducted_by":     null
            }),
        ))
        .await;
        assert_eq!(
            status,
            StatusCode::CREATED,
            "Re-induction after dispatch must succeed when capacity is freed"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /v1/hubs/{id}/manifest
// ─────────────────────────────────────────────────────────────────────────────

mod hub_manifest {
    use super::*;

    #[tokio::test]
    async fn returns_200_with_empty_manifest_for_new_hub() {
        let app = TestApp::new();
        let hub = seed_hub(&app, "Manifest Hub", 100);
        let path = format!("/v1/hubs/{}/manifest", hub.id.inner());
        let (status, body) = app.send(app.get(&path)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["count"], 0);
        assert!(body["parcels"].is_array());
    }

    #[tokio::test]
    async fn manifest_includes_inducted_parcels() {
        let app = TestApp::new();
        let hub = seed_hub(&app, "Active Hub", 100);
        seed_inducted_parcel(&app, &hub);
        seed_inducted_parcel(&app, &hub);

        let path = format!("/v1/hubs/{}/manifest", hub.id.inner());
        let (_, body) = app.send(app.get(&path)).await;
        assert_eq!(body["count"], 2);
    }

    #[tokio::test]
    async fn manifest_excludes_dispatched_parcels() {
        let app = TestApp::new();
        let hub = seed_hub(&app, "Partial Manifest Hub", 100);

        // Induct two parcels; dispatch one
        let ind1 = seed_inducted_parcel(&app, &hub);
        seed_inducted_parcel(&app, &hub);

        // Sort then dispatch ind1
        {
            let mut store = app.induction_repo.store.lock().unwrap();
            let p = store.get_mut(&ind1.id.inner()).unwrap();
            p.sort_to("ZONE-A".into(), "BAY-01".into());
            p.dispatch();
        }

        let path = format!("/v1/hubs/{}/manifest", hub.id.inner());
        let (_, body) = app.send(app.get(&path)).await;
        // Only the non-dispatched parcel must remain
        assert_eq!(body["count"], 1, "Dispatched parcel must be excluded from manifest");
    }

    #[tokio::test]
    async fn manifest_returns_404_for_unknown_hub() {
        let app = TestApp::new();
        let path = format!("/v1/hubs/{}/manifest", Uuid::new_v4());
        let (status, _) = app.send(app.get(&path)).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Observability endpoints
// ─────────────────────────────────────────────────────────────────────────────

mod observability {
    use super::*;

    #[tokio::test]
    async fn health_returns_200() {
        let app = TestApp::new();
        let req = Request::builder()
            .method(Method::GET)
            .uri("/health")
            .body(Body::empty())
            .unwrap();
        let (status, body) = app.send(req).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ok");
        assert_eq!(body["service"], "hub-ops");
    }

    #[tokio::test]
    async fn ready_returns_200() {
        let app = TestApp::new();
        let req = Request::builder()
            .method(Method::GET)
            .uri("/ready")
            .body(Body::empty())
            .unwrap();
        let (status, body) = app.send(req).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ready");
    }
}
