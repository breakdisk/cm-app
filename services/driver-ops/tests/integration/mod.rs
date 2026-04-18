//! Integration tests for the driver-ops service HTTP API.
//!
//! Builds an Axum router with in-memory mock repositories (no DB, no Kafka)
//! and exercises every public endpoint via `tower::ServiceExt::oneshot`.
//!
//! Test JWT tokens are HS256, signed with the well-known test secret.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
};
use chrono::Utc;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use tower::ServiceExt;
use uuid::Uuid;

use logisticos_auth::{claims::Claims, jwt::JwtService};
use logisticos_types::{Address, Coordinates, DriverId, TenantId};

use logisticos_driver_ops::{
    api::http::{AppState, RosterEvent},
    application::services::{DriverService, LocationService, TaskService},
    domain::{
        entities::{Driver, DriverLocation, DriverStatus, DriverType, DriverTask, TaskStatus, TaskType},
        repositories::{DriverRepository, LocationRepository, TaskRepository},
    },
};

// ─────────────────────────────────────────────────────────────────────────────
// Test constants
// ─────────────────────────────────────────────────────────────────────────────

const JWT_SECRET: &str = "test-secret-key-for-logisticos-testing";

// ─────────────────────────────────────────────────────────────────────────────
// JWT helper
// ─────────────────────────────────────────────────────────────────────────────

fn make_jwt(tenant_id: Uuid) -> String {
    make_jwt_for_user(Uuid::new_v4(), tenant_id)
}

fn make_jwt_for_user(user_id: Uuid, tenant_id: Uuid) -> String {
    let claims = Claims::new(
        user_id,
        tenant_id,
        "test-tenant".into(),
        "business".into(),
        "test@logisticos.io".into(),
        vec!["admin".into()],
        vec![
            "fleet:read".into(),
            "fleet:manage".into(),
            "drivers:read".into(),
            "drivers:manage".into(),
            "drivers:create".into(),
        ],
        3600,
    );
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(JWT_SECRET.as_bytes()),
    )
    .expect("JWT encoding must succeed in tests")
}

// ─────────────────────────────────────────────────────────────────────────────
// MockDriverRepository
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
struct MockDriverRepository {
    store: Arc<Mutex<HashMap<Uuid, Driver>>>,
}

impl MockDriverRepository {
    fn new() -> Self {
        Self::default()
    }

    fn with_driver(self, driver: Driver) -> Self {
        self.store
            .lock()
            .unwrap()
            .insert(driver.id.inner(), driver);
        self
    }
}

#[async_trait::async_trait]
impl DriverRepository for MockDriverRepository {
    async fn find_by_id(&self, id: &DriverId) -> anyhow::Result<Option<Driver>> {
        Ok(self.store.lock().unwrap().get(&id.inner()).cloned())
    }

    async fn find_by_user_id(&self, user_id: Uuid) -> anyhow::Result<Option<Driver>> {
        Ok(self
            .store
            .lock()
            .unwrap()
            .values()
            .find(|d| d.user_id == user_id)
            .cloned())
    }

    async fn list_by_tenant(&self, tenant_id: &TenantId) -> anyhow::Result<Vec<Driver>> {
        Ok(self
            .store
            .lock()
            .unwrap()
            .values()
            .filter(|d| d.tenant_id.inner() == tenant_id.inner())
            .cloned()
            .collect())
    }

    async fn save(&self, driver: &Driver) -> anyhow::Result<()> {
        self.store
            .lock()
            .unwrap()
            .insert(driver.id.inner(), driver.clone());
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// MockTaskRepository
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
struct MockTaskRepository {
    store: Arc<Mutex<HashMap<Uuid, DriverTask>>>,
}

impl MockTaskRepository {
    fn new() -> Self {
        Self::default()
    }

    fn with_task(self, task: DriverTask) -> Self {
        self.store.lock().unwrap().insert(task.id, task);
        self
    }

    fn get(&self, id: Uuid) -> Option<DriverTask> {
        self.store.lock().unwrap().get(&id).cloned()
    }
}

#[async_trait::async_trait]
impl TaskRepository for MockTaskRepository {
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<DriverTask>> {
        Ok(self.store.lock().unwrap().get(&id).cloned())
    }

    async fn list_by_driver(&self, driver_id: &DriverId) -> anyhow::Result<Vec<DriverTask>> {
        Ok(self
            .store
            .lock()
            .unwrap()
            .values()
            .filter(|t| t.driver_id == *driver_id)
            .cloned()
            .collect())
    }

    async fn list_by_route(&self, route_id: Uuid) -> anyhow::Result<Vec<DriverTask>> {
        Ok(self
            .store
            .lock()
            .unwrap()
            .values()
            .filter(|t| t.route_id == route_id)
            .cloned()
            .collect())
    }

    async fn save(&self, task: &DriverTask) -> anyhow::Result<()> {
        self.store.lock().unwrap().insert(task.id, task.clone());
        Ok(())
    }

    async fn bulk_save(&self, tasks: &[DriverTask]) -> anyhow::Result<()> {
        let mut store = self.store.lock().unwrap();
        for task in tasks {
            store.insert(task.id, task.clone());
        }
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// MockLocationRepository
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
struct MockLocationRepository {
    records: Arc<Mutex<Vec<DriverLocation>>>,
}

impl MockLocationRepository {
    fn new() -> Self {
        Self::default()
    }

    fn count(&self) -> usize {
        self.records.lock().unwrap().len()
    }
}

#[async_trait::async_trait]
impl LocationRepository for MockLocationRepository {
    async fn record(&self, location: &DriverLocation) -> anyhow::Result<()> {
        self.records.lock().unwrap().push(location.clone());
        Ok(())
    }

    async fn latest(&self, driver_id: &DriverId) -> anyhow::Result<Option<DriverLocation>> {
        Ok(self
            .records
            .lock()
            .unwrap()
            .iter()
            .filter(|l| l.driver_id == driver_id.inner())
            .last()
            .cloned())
    }

    async fn history(
        &self,
        driver_id: &DriverId,
        from: chrono::DateTime<chrono::Utc>,
        to: chrono::DateTime<chrono::Utc>,
    ) -> anyhow::Result<Vec<DriverLocation>> {
        Ok(self
            .records
            .lock()
            .unwrap()
            .iter()
            .filter(|l| {
                l.driver_id == driver_id.inner()
                    && l.recorded_at >= from
                    && l.recorded_at <= to
            })
            .cloned()
            .collect())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// App builder
// ─────────────────────────────────────────────────────────────────────────────

struct TestApp {
    router: axum::Router,
    tenant_id: Uuid,
    driver_repo: MockDriverRepository,
    task_repo: MockTaskRepository,
    location_repo: MockLocationRepository,
}

impl TestApp {
    fn build() -> Self {
        let tenant_id = Uuid::new_v4();
        let driver_repo = MockDriverRepository::new();
        let task_repo = MockTaskRepository::new();
        let location_repo = MockLocationRepository::new();
        Self::build_with(tenant_id, driver_repo, task_repo, location_repo)
    }

    fn build_with(
        tenant_id: Uuid,
        driver_repo: MockDriverRepository,
        task_repo: MockTaskRepository,
        location_repo: MockLocationRepository,
    ) -> Self {
        let jwt_svc = Arc::new(JwtService::new(JWT_SECRET, 3600, 86400));

        // Use a KafkaProducer pointed at a non-existent broker.
        // Task/location service calls that publish events will fail, but
        // because the failing publish is mapped to AppError::EventPublish
        // those errors surface as 500 only when event publishing is
        // on the critical path.  For the routes under test (task state
        // transitions) the Kafka publish failure causes a 500 instead of 204.
        // To keep tests hermetic we use a null/noop Kafka via a local
        // broadcast that discards every message, which the production code
        // does not provide.  The cleanest approach for integration tests
        // that do NOT want to spin up a real broker is to use a test-only
        // KafkaProducer variant or to skip event-publishing assertions.
        // We therefore accept 500 responses on task mutation tests where
        // Kafka is involved, and verify repository state directly instead.
        let kafka = Arc::new(
            logisticos_events::producer::KafkaProducer::new("localhost:9099")
                .expect("mock kafka construction must not require a live broker"),
        );

        let driver_repo_arc: Arc<dyn DriverRepository> = Arc::new(driver_repo.clone());
        let task_repo_arc: Arc<dyn TaskRepository> = Arc::new(task_repo.clone());
        let location_repo_arc: Arc<dyn LocationRepository> = Arc::new(location_repo.clone());

        let driver_svc = Arc::new(DriverService::new(Arc::clone(&driver_repo_arc)));
        let task_svc = Arc::new(TaskService::new(
            Arc::clone(&task_repo_arc),
            Arc::clone(&driver_repo_arc),
            Arc::clone(&kafka),
        ));
        let location_svc = Arc::new(LocationService::new(
            Arc::clone(&driver_repo_arc),
            Arc::clone(&location_repo_arc),
            Arc::clone(&kafka),
        ));

        let (roster_tx, _roster_rx) =
            tokio::sync::broadcast::channel::<RosterEvent>(64);

        let state = Arc::new(AppState {
            driver_service: driver_svc,
            task_service: task_svc,
            location_service: location_svc,
            jwt: Arc::clone(&jwt_svc),
            roster_tx,
        });

        let router = logisticos_driver_ops::api::http::router(state);

        Self {
            router,
            tenant_id,
            driver_repo,
            task_repo,
            location_repo,
        }
    }

    fn token(&self) -> String {
        make_jwt(self.tenant_id)
    }

    fn token_for_user(&self, user_id: Uuid) -> String {
        make_jwt_for_user(user_id, self.tenant_id)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Domain helpers
// ─────────────────────────────────────────────────────────────────────────────

fn make_driver(tenant_id: Uuid, user_id: Uuid) -> Driver {
    let now = Utc::now();
    Driver {
        id: DriverId::new(),
        tenant_id: TenantId::from_uuid(tenant_id),
        user_id,
        first_name: "Juan".into(),
        last_name: "dela Cruz".into(),
        phone: "+639171234567".into(),
        status: DriverStatus::Offline,
        current_location: None,
        last_location_at: None,
        vehicle_id: None,
        active_route_id: None,
        is_active: true,
        driver_type: DriverType::FullTime,
        per_delivery_rate_cents: 0,
        cod_commission_rate_bps: 0,
        zone: None,
        vehicle_type: None,
        created_at: now,
        updated_at: now,
    }
}

fn make_pending_task(driver_id: &DriverId, task_type: TaskType) -> DriverTask {
    DriverTask {
        id: Uuid::new_v4(),
        driver_id: driver_id.clone(),
        route_id: Uuid::new_v4(),
        shipment_id: Uuid::new_v4(),
        task_type,
        sequence: 1,
        status: TaskStatus::Pending,
        address: Address {
            line1: "123 Rizal Street".into(),
            line2: None,
            barangay: Some("Poblacion".into()),
            city: "Makati".into(),
            province: "Metro Manila".into(),
            postal_code: "1210".into(),
            country_code: "PH".into(),
            coordinates: Some(Coordinates {
                lat: 14.5547,
                lng: 121.0244,
            }),
        },
        customer_name: "Maria Santos".into(),
        customer_phone: "+639171234567".into(),
        cod_amount_cents: None,
        special_instructions: None,
        pod_id: None,
        started_at: None,
        completed_at: None,
        failed_reason: None,
    }
}

fn make_inprogress_task(driver_id: &DriverId, task_type: TaskType) -> DriverTask {
    let mut task = make_pending_task(driver_id, task_type);
    task.start();
    task
}

// ─────────────────────────────────────────────────────────────────────────────
// Request helpers
// ─────────────────────────────────────────────────────────────────────────────

fn json_request(method: Method, uri: &str, token: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn get_request(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

fn put_request(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method(Method::PUT)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests: GET /v1/drivers
// ─────────────────────────────────────────────────────────────────────────────

mod list_drivers {
    use super::*;

    #[tokio::test]
    async fn returns_empty_list_when_no_drivers_exist() {
        let app = TestApp::build();
        let token = app.token();

        let response = app
            .router
            .oneshot(get_request("/v1/drivers", &token))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let json = body_json(response).await;
        assert_eq!(json["data"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn returns_drivers_belonging_to_caller_tenant() {
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let driver = make_driver(tenant_id, user_id);
        let driver_repo = MockDriverRepository::new().with_driver(driver.clone());

        let app = TestApp::build_with(
            tenant_id,
            driver_repo,
            MockTaskRepository::new(),
            MockLocationRepository::new(),
        );
        let token = app.token();

        let response = app
            .router
            .oneshot(get_request("/v1/drivers", &token))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let json = body_json(response).await;
        let data = json["data"].as_array().unwrap();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0]["first_name"], "Juan");
        assert_eq!(data[0]["last_name"], "dela Cruz");
    }

    #[tokio::test]
    async fn does_not_return_drivers_from_other_tenants() {
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        let driver_b = make_driver(tenant_b, Uuid::new_v4());
        let driver_repo = MockDriverRepository::new().with_driver(driver_b);

        let app = TestApp::build_with(
            tenant_a,
            driver_repo,
            MockTaskRepository::new(),
            MockLocationRepository::new(),
        );
        let token = app.token();

        let response = app
            .router
            .oneshot(get_request("/v1/drivers", &token))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let json = body_json(response).await;
        assert_eq!(
            json["data"].as_array().unwrap().len(),
            0,
            "tenant_a must not see tenant_b's drivers"
        );
    }

    #[tokio::test]
    async fn returns_multiple_drivers_for_same_tenant() {
        let tenant_id = Uuid::new_v4();
        let d1 = make_driver(tenant_id, Uuid::new_v4());
        let d2 = make_driver(tenant_id, Uuid::new_v4());
        let d3 = make_driver(tenant_id, Uuid::new_v4());

        let driver_repo = MockDriverRepository::new()
            .with_driver(d1)
            .with_driver(d2)
            .with_driver(d3);

        let app = TestApp::build_with(
            tenant_id,
            driver_repo,
            MockTaskRepository::new(),
            MockLocationRepository::new(),
        );
        let token = app.token();

        let response = app
            .router
            .oneshot(get_request("/v1/drivers", &token))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            body_json(response).await["data"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
    }

    #[tokio::test]
    async fn requires_bearer_token() {
        let app = TestApp::build();
        let response = app
            .router
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/v1/drivers")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn rejects_invalid_jwt() {
        let app = TestApp::build();
        let response = app
            .router
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/v1/drivers")
                    .header(header::AUTHORIZATION, "Bearer not.a.valid.token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests: POST /v1/drivers
// ─────────────────────────────────────────────────────────────────────────────

mod register_driver {
    use super::*;

    #[tokio::test]
    async fn creates_driver_and_returns_driver_id() {
        let app = TestApp::build();
        let token = app.token();

        let body = serde_json::json!({
            "user_id": Uuid::new_v4(),
            "first_name": "Pedro",
            "last_name": "Reyes",
            "phone": "+639181234567",
            "vehicle_id": null
        });

        let response = app
            .router
            .oneshot(json_request(Method::POST, "/v1/drivers", &token, body))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let json = body_json(response).await;
        assert!(
            json["data"]["driver_id"].is_string(),
            "Response must include driver_id string"
        );
    }

    #[tokio::test]
    async fn is_idempotent_for_same_user_id() {
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let existing_driver = make_driver(tenant_id, user_id);
        let existing_driver_id = existing_driver.id.inner();

        let driver_repo = MockDriverRepository::new().with_driver(existing_driver);
        let app = TestApp::build_with(
            tenant_id,
            driver_repo,
            MockTaskRepository::new(),
            MockLocationRepository::new(),
        );
        let token = app.token();

        let body = serde_json::json!({
            "user_id": user_id,
            "first_name": "Different Name",
            "last_name": "Different Last",
            "phone": "+639181234567"
        });

        let response = app
            .router
            .oneshot(json_request(Method::POST, "/v1/drivers", &token, body))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let json = body_json(response).await;
        assert_eq!(
            json["data"]["driver_id"].as_str().unwrap(),
            existing_driver_id.to_string(),
            "Idempotent registration must return the existing driver_id"
        );
    }

    #[tokio::test]
    async fn creates_driver_with_vehicle_id() {
        let app = TestApp::build();
        let token = app.token();
        let vehicle_id = Uuid::new_v4();

        let body = serde_json::json!({
            "user_id": Uuid::new_v4(),
            "first_name": "Ana",
            "last_name": "Gomez",
            "phone": "+639221234567",
            "vehicle_id": vehicle_id
        });

        let response = app
            .router
            .oneshot(json_request(Method::POST, "/v1/drivers", &token, body))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(body_json(response).await["data"]["driver_id"].is_string());
    }

    #[tokio::test]
    async fn requires_fleet_manage_permission() {
        let tenant_id = Uuid::new_v4();
        let read_only_claims = Claims::new(
            Uuid::new_v4(),
            tenant_id,
            "test-tenant".into(),
            "starter".into(),
            "readonly@logisticos.io".into(),
            vec!["dispatcher".into()],
            vec!["fleet:read".into()], // no fleet:manage
            3600,
        );
        let token = encode(
            &Header::new(Algorithm::HS256),
            &read_only_claims,
            &EncodingKey::from_secret(JWT_SECRET.as_bytes()),
        )
        .unwrap();

        let app = TestApp::build_with(
            tenant_id,
            MockDriverRepository::new(),
            MockTaskRepository::new(),
            MockLocationRepository::new(),
        );

        let body = serde_json::json!({
            "user_id": Uuid::new_v4(),
            "first_name": "Ana",
            "last_name": "Gomez",
            "phone": "+639221234567"
        });

        let response = app
            .router
            .oneshot(json_request(Method::POST, "/v1/drivers", &token, body))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn driver_initial_status_is_offline() {
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let driver_repo = MockDriverRepository::new();

        let app = TestApp::build_with(
            tenant_id,
            driver_repo.clone(),
            MockTaskRepository::new(),
            MockLocationRepository::new(),
        );
        let token = app.token();

        let body = serde_json::json!({
            "user_id": user_id,
            "first_name": "Carlos",
            "last_name": "Mendoza",
            "phone": "+639301234567"
        });

        app.router
            .oneshot(json_request(Method::POST, "/v1/drivers", &token, body))
            .await
            .unwrap();

        let saved = driver_repo
            .store
            .lock()
            .unwrap()
            .values()
            .find(|d| d.user_id == user_id)
            .cloned()
            .unwrap();
        assert_eq!(
            saved.status,
            DriverStatus::Offline,
            "Newly registered driver must be Offline"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests: GET /v1/drivers/:id
// ─────────────────────────────────────────────────────────────────────────────

mod get_driver {
    use super::*;

    #[tokio::test]
    async fn returns_driver_by_id() {
        let tenant_id = Uuid::new_v4();
        let driver = make_driver(tenant_id, Uuid::new_v4());
        let driver_id = driver.id.inner();

        let driver_repo = MockDriverRepository::new().with_driver(driver);
        let app = TestApp::build_with(
            tenant_id,
            driver_repo,
            MockTaskRepository::new(),
            MockLocationRepository::new(),
        );
        let token = app.token();

        let response = app
            .router
            .oneshot(get_request(&format!("/v1/drivers/{driver_id}"), &token))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let json = body_json(response).await;
        assert_eq!(json["data"]["first_name"], "Juan");
        assert_eq!(json["data"]["last_name"], "dela Cruz");
        assert_eq!(json["data"]["phone"], "+639171234567");
    }

    #[tokio::test]
    async fn returns_404_for_nonexistent_driver() {
        let app = TestApp::build();
        let token = app.token();

        let response = app
            .router
            .oneshot(get_request(
                &format!("/v1/drivers/{}", Uuid::new_v4()),
                &token,
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let json = body_json(response).await;
        assert_eq!(json["error"]["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn returns_404_for_driver_in_different_tenant() {
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        let driver = make_driver(tenant_b, Uuid::new_v4());
        let driver_id = driver.id.inner();

        let driver_repo = MockDriverRepository::new().with_driver(driver);
        let app = TestApp::build_with(
            tenant_a, // token will be for tenant_a
            driver_repo,
            MockTaskRepository::new(),
            MockLocationRepository::new(),
        );
        let token = app.token();

        let response = app
            .router
            .oneshot(get_request(&format!("/v1/drivers/{driver_id}"), &token))
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "Cross-tenant driver access must return 404 (not 403) for information hiding"
        );
    }

    #[tokio::test]
    async fn response_includes_status_and_is_active() {
        let tenant_id = Uuid::new_v4();
        let driver = make_driver(tenant_id, Uuid::new_v4());
        let driver_id = driver.id.inner();

        let driver_repo = MockDriverRepository::new().with_driver(driver);
        let app = TestApp::build_with(
            tenant_id,
            driver_repo,
            MockTaskRepository::new(),
            MockLocationRepository::new(),
        );
        let token = app.token();

        let response = app
            .router
            .oneshot(get_request(&format!("/v1/drivers/{driver_id}"), &token))
            .await
            .unwrap();

        let json = body_json(response).await;
        assert!(json["data"]["status"].is_string());
        assert_eq!(json["data"]["is_active"], true);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests: PUT /v1/drivers/me/online and /v1/drivers/me/offline
// ─────────────────────────────────────────────────────────────────────────────

mod driver_status_transitions {
    use super::*;

    #[tokio::test]
    async fn go_online_changes_status_to_available() {
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let driver = make_driver(tenant_id, user_id);
        assert_eq!(driver.status, DriverStatus::Offline);
        let driver_id = driver.id.inner();

        let driver_repo = MockDriverRepository::new().with_driver(driver);
        let app = TestApp::build_with(
            tenant_id,
            driver_repo.clone(),
            MockTaskRepository::new(),
            MockLocationRepository::new(),
        );
        let token = app.token_for_user(user_id);

        let response = app
            .router
            .oneshot(put_request("/v1/drivers/me/online", &token))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let saved = driver_repo
            .store
            .lock()
            .unwrap()
            .get(&driver_id)
            .cloned()
            .unwrap();
        assert_eq!(saved.status, DriverStatus::Available);
    }

    #[tokio::test]
    async fn go_online_is_idempotent_when_already_available() {
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let mut driver = make_driver(tenant_id, user_id);
        driver.go_online();

        let driver_repo = MockDriverRepository::new().with_driver(driver);
        let app = TestApp::build_with(
            tenant_id,
            driver_repo,
            MockTaskRepository::new(),
            MockLocationRepository::new(),
        );
        let token = app.token_for_user(user_id);

        let response = app
            .router
            .oneshot(put_request("/v1/drivers/me/online", &token))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn go_offline_changes_status_to_offline() {
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let mut driver = make_driver(tenant_id, user_id);
        driver.go_online();
        let driver_id = driver.id.inner();

        let driver_repo = MockDriverRepository::new().with_driver(driver);
        let app = TestApp::build_with(
            tenant_id,
            driver_repo.clone(),
            MockTaskRepository::new(),
            MockLocationRepository::new(),
        );
        let token = app.token_for_user(user_id);

        let response = app
            .router
            .oneshot(put_request("/v1/drivers/me/offline", &token))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let saved = driver_repo
            .store
            .lock()
            .unwrap()
            .get(&driver_id)
            .cloned()
            .unwrap();
        assert_eq!(saved.status, DriverStatus::Offline);
    }

    #[tokio::test]
    async fn go_offline_rejected_with_active_route() {
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let mut driver = make_driver(tenant_id, user_id);
        driver.go_online();
        driver.active_route_id = Some(Uuid::new_v4());

        let driver_repo = MockDriverRepository::new().with_driver(driver);
        let app = TestApp::build_with(
            tenant_id,
            driver_repo,
            MockTaskRepository::new(),
            MockLocationRepository::new(),
        );
        let token = app.token_for_user(user_id);

        let response = app
            .router
            .oneshot(put_request("/v1/drivers/me/offline", &token))
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "Cannot go offline with an active route"
        );
        let json = body_json(response).await;
        assert_eq!(json["error"]["code"], "BUSINESS_RULE_VIOLATION");
    }

    #[tokio::test]
    async fn status_endpoints_require_auth() {
        let app = TestApp::build();
        for uri in &["/v1/drivers/me/online", "/v1/drivers/me/offline"] {
            let response = app
                .router
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::PUT)
                        .uri(*uri)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                StatusCode::UNAUTHORIZED,
                "PUT {uri} must require auth"
            );
        }
    }

    #[tokio::test]
    async fn go_online_returns_404_when_no_driver_profile() {
        // user_id in token does not have a driver profile
        let tenant_id = Uuid::new_v4();
        let unknown_user = Uuid::new_v4();
        let app = TestApp::build_with(
            tenant_id,
            MockDriverRepository::new(),
            MockTaskRepository::new(),
            MockLocationRepository::new(),
        );
        let token = make_jwt_for_user(unknown_user, tenant_id);

        let response = app
            .router
            .oneshot(put_request("/v1/drivers/me/online", &token))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests: POST /v1/location
// ─────────────────────────────────────────────────────────────────────────────

mod update_location {
    use super::*;

    fn fresh_location_body() -> serde_json::Value {
        let recorded_at = Utc::now().to_rfc3339();
        serde_json::json!({
            "lat": 14.5547,
            "lng": 121.0244,
            "accuracy_m": 8.5,
            "speed_kmh": 45.0,
            "heading": 180.0,
            "battery_pct": 72,
            "recorded_at": recorded_at
        })
    }

    #[tokio::test]
    async fn persists_location_record_for_known_driver() {
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let driver = make_driver(tenant_id, user_id);

        let driver_repo = MockDriverRepository::new().with_driver(driver);
        let location_repo = MockLocationRepository::new();

        let app = TestApp::build_with(
            tenant_id,
            driver_repo,
            MockTaskRepository::new(),
            location_repo.clone(),
        );
        let token = app.token_for_user(user_id);

        let response = app
            .router
            .oneshot(json_request(
                Method::POST,
                "/v1/location",
                &token,
                fresh_location_body(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(location_repo.count(), 1, "One location record must be stored");
    }

    #[tokio::test]
    async fn updates_driver_current_coordinates() {
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let driver = make_driver(tenant_id, user_id);
        let driver_id = driver.id.inner();

        let driver_repo = MockDriverRepository::new().with_driver(driver);

        let app = TestApp::build_with(
            tenant_id,
            driver_repo.clone(),
            MockTaskRepository::new(),
            MockLocationRepository::new(),
        );
        let token = app.token_for_user(user_id);

        app.router
            .oneshot(json_request(
                Method::POST,
                "/v1/location",
                &token,
                fresh_location_body(),
            ))
            .await
            .unwrap();

        let updated_driver = driver_repo
            .store
            .lock()
            .unwrap()
            .get(&driver_id)
            .cloned()
            .unwrap();

        let loc = updated_driver
            .current_location
            .expect("current_location must be set after update");
        assert!((loc.lat - 14.5547).abs() < 0.0001);
        assert!((loc.lng - 121.0244).abs() < 0.0001);
        assert!(updated_driver.last_location_at.is_some());
    }

    #[tokio::test]
    async fn rejects_location_fix_older_than_five_minutes() {
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let driver = make_driver(tenant_id, user_id);

        let driver_repo = MockDriverRepository::new().with_driver(driver);
        let app = TestApp::build_with(
            tenant_id,
            driver_repo,
            MockTaskRepository::new(),
            MockLocationRepository::new(),
        );
        let token = app.token_for_user(user_id);

        let stale_time = (Utc::now() - chrono::Duration::minutes(10)).to_rfc3339();
        let stale_body = serde_json::json!({
            "lat": 14.5547,
            "lng": 121.0244,
            "recorded_at": stale_time
        });

        let response = app
            .router
            .oneshot(json_request(
                Method::POST,
                "/v1/location",
                &token,
                stale_body,
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let json = body_json(response).await;
        assert_eq!(json["error"]["code"], "BUSINESS_RULE_VIOLATION");
    }

    #[tokio::test]
    async fn rejects_speed_above_200_kmh() {
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let driver = make_driver(tenant_id, user_id);

        let driver_repo = MockDriverRepository::new().with_driver(driver);
        let app = TestApp::build_with(
            tenant_id,
            driver_repo,
            MockTaskRepository::new(),
            MockLocationRepository::new(),
        );
        let token = app.token_for_user(user_id);

        let body = serde_json::json!({
            "lat": 14.5547,
            "lng": 121.0244,
            "speed_kmh": 201.0,
            "recorded_at": Utc::now().to_rfc3339()
        });

        let response = app
            .router
            .oneshot(json_request(Method::POST, "/v1/location", &token, body))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn accepts_speed_at_exactly_200_kmh() {
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let driver = make_driver(tenant_id, user_id);

        let driver_repo = MockDriverRepository::new().with_driver(driver);
        let app = TestApp::build_with(
            tenant_id,
            driver_repo,
            MockTaskRepository::new(),
            MockLocationRepository::new(),
        );
        let token = app.token_for_user(user_id);

        let body = serde_json::json!({
            "lat": 14.5547,
            "lng": 121.0244,
            "speed_kmh": 200.0,
            "recorded_at": Utc::now().to_rfc3339()
        });

        let response = app
            .router
            .oneshot(json_request(Method::POST, "/v1/location", &token, body))
            .await
            .unwrap();

        // 200 km/h is on the boundary and IS plausible (rule: > 200 is rejected)
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn returns_404_when_no_driver_profile_for_user() {
        let tenant_id = Uuid::new_v4();
        let unknown_user = Uuid::new_v4();

        let app = TestApp::build_with(
            tenant_id,
            MockDriverRepository::new(),
            MockTaskRepository::new(),
            MockLocationRepository::new(),
        );
        let token = make_jwt_for_user(unknown_user, tenant_id);

        let response = app
            .router
            .oneshot(json_request(
                Method::POST,
                "/v1/location",
                &token,
                fresh_location_body(),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests: GET /v1/tasks
// ─────────────────────────────────────────────────────────────────────────────

mod list_my_tasks {
    use super::*;

    #[tokio::test]
    async fn returns_pending_and_inprogress_tasks() {
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let driver = make_driver(tenant_id, user_id);
        let driver_id = driver.id.clone();

        let pending = make_pending_task(&driver_id, TaskType::Delivery);
        let in_progress = make_inprogress_task(&driver_id, TaskType::Pickup);
        let mut completed = make_pending_task(&driver_id, TaskType::Delivery);
        completed.status = TaskStatus::Completed;
        let mut failed = make_pending_task(&driver_id, TaskType::Delivery);
        failed.status = TaskStatus::Failed;

        let task_repo = MockTaskRepository::new()
            .with_task(pending)
            .with_task(in_progress)
            .with_task(completed)
            .with_task(failed);

        let app = TestApp::build_with(
            tenant_id,
            MockDriverRepository::new().with_driver(driver),
            task_repo,
            MockLocationRepository::new(),
        );
        let token = app.token_for_user(user_id);

        let response = app
            .router
            .oneshot(get_request("/v1/tasks", &token))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let json = body_json(response).await;
        let data = json["data"].as_array().unwrap();
        assert_eq!(
            data.len(),
            2,
            "Only pending + in-progress tasks should be returned"
        );
    }

    #[tokio::test]
    async fn returns_empty_list_when_no_tasks() {
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        let app = TestApp::build_with(
            tenant_id,
            MockDriverRepository::new(),
            MockTaskRepository::new(),
            MockLocationRepository::new(),
        );
        let token = make_jwt_for_user(user_id, tenant_id);

        let response = app
            .router
            .oneshot(get_request("/v1/tasks", &token))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            body_json(response).await["data"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }

    #[tokio::test]
    async fn task_summary_includes_required_fields() {
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let driver = make_driver(tenant_id, user_id);
        let driver_id = driver.id.clone();
        let mut task = make_pending_task(&driver_id, TaskType::Delivery);
        task.cod_amount_cents = Some(25000);

        let app = TestApp::build_with(
            tenant_id,
            MockDriverRepository::new().with_driver(driver),
            MockTaskRepository::new().with_task(task.clone()),
            MockLocationRepository::new(),
        );
        let token = app.token_for_user(user_id);

        let response = app
            .router
            .oneshot(get_request("/v1/tasks", &token))
            .await
            .unwrap();

        let json = body_json(response).await;
        let first = &json["data"][0];

        assert!(first["task_id"].is_string(), "task_id must be present");
        assert!(first["shipment_id"].is_string(), "shipment_id must be present");
        assert_eq!(first["sequence"], 1);
        assert_eq!(first["status"], "pending");
        assert_eq!(first["task_type"], "delivery");
        assert_eq!(first["customer_name"], "Maria Santos");
        assert_eq!(first["cod_amount_cents"], 25000);
        assert!(first["address"].is_string(), "address must be a formatted string");
    }

    #[tokio::test]
    async fn driver_cannot_see_tasks_of_other_drivers() {
        let tenant_id = Uuid::new_v4();
        let user_b_id = Uuid::new_v4();
        let driver_b = make_driver(tenant_id, user_b_id);
        let task_b = make_pending_task(&driver_b.id.clone(), TaskType::Delivery);

        let app = TestApp::build_with(
            tenant_id,
            MockDriverRepository::new().with_driver(driver_b),
            MockTaskRepository::new().with_task(task_b),
            MockLocationRepository::new(),
        );
        // Token is for a different user (user_a)
        let user_a_id = Uuid::new_v4();
        let token = make_jwt_for_user(user_a_id, tenant_id);

        let response = app
            .router
            .oneshot(get_request("/v1/tasks", &token))
            .await
            .unwrap();

        let json = body_json(response).await;
        assert_eq!(
            json["data"].as_array().unwrap().len(),
            0,
            "Driver A must not see driver B's tasks"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests: PUT /v1/tasks/:id/start
// ─────────────────────────────────────────────────────────────────────────────

mod start_task {
    use super::*;

    #[tokio::test]
    async fn transitions_pending_to_inprogress() {
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let driver = make_driver(tenant_id, user_id);
        let driver_id = driver.id.clone();
        let task = make_pending_task(&driver_id, TaskType::Delivery);
        let task_id = task.id;

        let task_repo = MockTaskRepository::new().with_task(task);
        let app = TestApp::build_with(
            tenant_id,
            MockDriverRepository::new().with_driver(driver),
            task_repo.clone(),
            MockLocationRepository::new(),
        );
        let token = app.token_for_user(user_id);

        let response = app
            .router
            .oneshot(put_request(&format!("/v1/tasks/{task_id}/start"), &token))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let saved = task_repo.get(task_id).unwrap();
        assert_eq!(saved.status, TaskStatus::InProgress);
        assert!(saved.started_at.is_some(), "started_at must be set");
    }

    #[tokio::test]
    async fn returns_404_for_nonexistent_task() {
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        let app = TestApp::build_with(
            tenant_id,
            MockDriverRepository::new(),
            MockTaskRepository::new(),
            MockLocationRepository::new(),
        );
        let token = make_jwt_for_user(user_id, tenant_id);

        let response = app
            .router
            .oneshot(put_request(
                &format!("/v1/tasks/{}/start", Uuid::new_v4()),
                &token,
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn returns_403_when_driver_does_not_own_task() {
        let tenant_id = Uuid::new_v4();
        let owner_user = Uuid::new_v4();
        let intruder_user = Uuid::new_v4();

        let owner_driver = make_driver(tenant_id, owner_user);
        let task = make_pending_task(&owner_driver.id.clone(), TaskType::Pickup);
        let task_id = task.id;

        let app = TestApp::build_with(
            tenant_id,
            MockDriverRepository::new().with_driver(owner_driver),
            MockTaskRepository::new().with_task(task),
            MockLocationRepository::new(),
        );
        let token = make_jwt_for_user(intruder_user, tenant_id);

        let response = app
            .router
            .oneshot(put_request(&format!("/v1/tasks/{task_id}/start"), &token))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn cannot_start_an_already_inprogress_task() {
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let driver = make_driver(tenant_id, user_id);
        let driver_id = driver.id.clone();
        let task = make_inprogress_task(&driver_id, TaskType::Delivery);
        let task_id = task.id;

        let app = TestApp::build_with(
            tenant_id,
            MockDriverRepository::new().with_driver(driver),
            MockTaskRepository::new().with_task(task),
            MockLocationRepository::new(),
        );
        let token = app.token_for_user(user_id);

        let response = app
            .router
            .oneshot(put_request(&format!("/v1/tasks/{task_id}/start"), &token))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn cannot_start_a_completed_task() {
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let driver = make_driver(tenant_id, user_id);
        let driver_id = driver.id.clone();
        let mut task = make_inprogress_task(&driver_id, TaskType::Pickup);
        task.complete(None).unwrap();
        let task_id = task.id;

        let app = TestApp::build_with(
            tenant_id,
            MockDriverRepository::new().with_driver(driver),
            MockTaskRepository::new().with_task(task),
            MockLocationRepository::new(),
        );
        let token = app.token_for_user(user_id);

        let response = app
            .router
            .oneshot(put_request(&format!("/v1/tasks/{task_id}/start"), &token))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests: PUT /v1/tasks/:id/complete
// ─────────────────────────────────────────────────────────────────────────────

mod complete_task {
    use super::*;

    #[tokio::test]
    async fn completes_pickup_task_without_pod() {
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let driver = make_driver(tenant_id, user_id);
        let driver_id = driver.id.clone();
        let task = make_inprogress_task(&driver_id, TaskType::Pickup);
        let task_id = task.id;

        let task_repo = MockTaskRepository::new().with_task(task);
        let app = TestApp::build_with(
            tenant_id,
            MockDriverRepository::new().with_driver(driver),
            task_repo.clone(),
            MockLocationRepository::new(),
        );
        let token = app.token_for_user(user_id);

        let body = serde_json::json!({
            "task_id": task_id,
            "pod_id": null,
            "cod_collected_cents": null
        });

        let response = app
            .router
            .oneshot(json_request(
                Method::PUT,
                &format!("/v1/tasks/{task_id}/complete"),
                &token,
                body,
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let saved = task_repo.get(task_id).unwrap();
        assert_eq!(saved.status, TaskStatus::Completed);
        assert!(saved.completed_at.is_some());
    }

    #[tokio::test]
    async fn completes_delivery_task_with_pod() {
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let driver = make_driver(tenant_id, user_id);
        let driver_id = driver.id.clone();
        let task = make_inprogress_task(&driver_id, TaskType::Delivery);
        let task_id = task.id;
        let pod_id = Uuid::new_v4();

        let task_repo = MockTaskRepository::new().with_task(task);
        let app = TestApp::build_with(
            tenant_id,
            MockDriverRepository::new().with_driver(driver),
            task_repo.clone(),
            MockLocationRepository::new(),
        );
        let token = app.token_for_user(user_id);

        let body = serde_json::json!({
            "task_id": task_id,
            "pod_id": pod_id,
            "cod_collected_cents": null
        });

        let response = app
            .router
            .oneshot(json_request(
                Method::PUT,
                &format!("/v1/tasks/{task_id}/complete"),
                &token,
                body,
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let saved = task_repo.get(task_id).unwrap();
        assert_eq!(saved.status, TaskStatus::Completed);
        assert_eq!(saved.pod_id, Some(pod_id));
    }

    #[tokio::test]
    async fn rejects_delivery_completion_without_pod() {
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let driver = make_driver(tenant_id, user_id);
        let driver_id = driver.id.clone();
        let task = make_inprogress_task(&driver_id, TaskType::Delivery);
        let task_id = task.id;

        let app = TestApp::build_with(
            tenant_id,
            MockDriverRepository::new().with_driver(driver),
            MockTaskRepository::new().with_task(task),
            MockLocationRepository::new(),
        );
        let token = app.token_for_user(user_id);

        let body = serde_json::json!({
            "task_id": task_id,
            "pod_id": null,
            "cod_collected_cents": null
        });

        let response = app
            .router
            .oneshot(json_request(
                Method::PUT,
                &format!("/v1/tasks/{task_id}/complete"),
                &token,
                body,
            ))
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "Delivery task requires POD"
        );
    }

    #[tokio::test]
    async fn cannot_complete_a_pending_task() {
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let driver = make_driver(tenant_id, user_id);
        let driver_id = driver.id.clone();
        let task = make_pending_task(&driver_id, TaskType::Pickup);
        let task_id = task.id;

        let app = TestApp::build_with(
            tenant_id,
            MockDriverRepository::new().with_driver(driver),
            MockTaskRepository::new().with_task(task),
            MockLocationRepository::new(),
        );
        let token = app.token_for_user(user_id);

        let body = serde_json::json!({
            "task_id": task_id,
            "pod_id": null,
            "cod_collected_cents": null
        });

        let response = app
            .router
            .oneshot(json_request(
                Method::PUT,
                &format!("/v1/tasks/{task_id}/complete"),
                &token,
                body,
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn path_id_takes_precedence_over_body_task_id() {
        // The handler overwrites cmd.task_id with the path param.
        // Supply a wrong UUID in the body — the correct path UUID should be used.
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let driver = make_driver(tenant_id, user_id);
        let driver_id = driver.id.clone();
        let task = make_inprogress_task(&driver_id, TaskType::Pickup);
        let real_task_id = task.id;

        let task_repo = MockTaskRepository::new().with_task(task);
        let app = TestApp::build_with(
            tenant_id,
            MockDriverRepository::new().with_driver(driver),
            task_repo.clone(),
            MockLocationRepository::new(),
        );
        let token = app.token_for_user(user_id);

        let body = serde_json::json!({
            "task_id": Uuid::new_v4(),  // wrong body UUID — must be ignored
            "pod_id": null,
            "cod_collected_cents": null
        });

        let response = app
            .router
            .oneshot(json_request(
                Method::PUT,
                &format!("/v1/tasks/{real_task_id}/complete"),
                &token,
                body,
            ))
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::NO_CONTENT,
            "Path :id must override body task_id"
        );
        let saved = task_repo.get(real_task_id).unwrap();
        assert_eq!(saved.status, TaskStatus::Completed);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests: PUT /v1/tasks/:id/fail
// ─────────────────────────────────────────────────────────────────────────────

mod fail_task {
    use super::*;

    #[tokio::test]
    async fn fails_inprogress_task_and_stores_reason() {
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let driver = make_driver(tenant_id, user_id);
        let driver_id = driver.id.clone();
        let task = make_inprogress_task(&driver_id, TaskType::Delivery);
        let task_id = task.id;

        let task_repo = MockTaskRepository::new().with_task(task);
        let app = TestApp::build_with(
            tenant_id,
            MockDriverRepository::new().with_driver(driver),
            task_repo.clone(),
            MockLocationRepository::new(),
        );
        let token = app.token_for_user(user_id);

        let fail_reason = "Customer not home — door tag left";
        let body = serde_json::json!({
            "task_id": task_id,
            "reason": fail_reason
        });

        let response = app
            .router
            .oneshot(json_request(
                Method::PUT,
                &format!("/v1/tasks/{task_id}/fail"),
                &token,
                body,
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let saved = task_repo.get(task_id).unwrap();
        assert_eq!(saved.status, TaskStatus::Failed);
        assert_eq!(
            saved.failed_reason.as_deref(),
            Some(fail_reason),
            "Failure reason must be stored verbatim"
        );
        assert!(saved.completed_at.is_some());
    }

    #[tokio::test]
    async fn fails_pending_task() {
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let driver = make_driver(tenant_id, user_id);
        let driver_id = driver.id.clone();
        let task = make_pending_task(&driver_id, TaskType::Delivery);
        let task_id = task.id;

        let task_repo = MockTaskRepository::new().with_task(task);
        let app = TestApp::build_with(
            tenant_id,
            MockDriverRepository::new().with_driver(driver),
            task_repo.clone(),
            MockLocationRepository::new(),
        );
        let token = app.token_for_user(user_id);

        let body = serde_json::json!({
            "task_id": task_id,
            "reason": "Address does not exist on delivery route"
        });

        let response = app
            .router
            .oneshot(json_request(
                Method::PUT,
                &format!("/v1/tasks/{task_id}/fail"),
                &token,
                body,
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(task_repo.get(task_id).unwrap().status, TaskStatus::Failed);
    }

    #[tokio::test]
    async fn cannot_fail_completed_task() {
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let driver = make_driver(tenant_id, user_id);
        let driver_id = driver.id.clone();
        let mut task = make_inprogress_task(&driver_id, TaskType::Pickup);
        task.complete(None).unwrap();
        let task_id = task.id;

        let app = TestApp::build_with(
            tenant_id,
            MockDriverRepository::new().with_driver(driver),
            MockTaskRepository::new().with_task(task),
            MockLocationRepository::new(),
        );
        let token = app.token_for_user(user_id);

        let body = serde_json::json!({
            "task_id": task_id,
            "reason": "Attempting to fail a completed task"
        });

        let response = app
            .router
            .oneshot(json_request(
                Method::PUT,
                &format!("/v1/tasks/{task_id}/fail"),
                &token,
                body,
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn returns_403_when_task_belongs_to_other_driver() {
        let tenant_id = Uuid::new_v4();
        let owner_user = Uuid::new_v4();
        let attacker_user = Uuid::new_v4();

        let owner_driver = make_driver(tenant_id, owner_user);
        let task = make_inprogress_task(&owner_driver.id.clone(), TaskType::Delivery);
        let task_id = task.id;

        let app = TestApp::build_with(
            tenant_id,
            MockDriverRepository::new().with_driver(owner_driver),
            MockTaskRepository::new().with_task(task),
            MockLocationRepository::new(),
        );
        let token = make_jwt_for_user(attacker_user, tenant_id);

        let body = serde_json::json!({
            "task_id": task_id,
            "reason": "Unauthorized fail attempt"
        });

        let response = app
            .router
            .oneshot(json_request(
                Method::PUT,
                &format!("/v1/tasks/{task_id}/fail"),
                &token,
                body,
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn returns_404_for_nonexistent_task() {
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let nonexistent_id = Uuid::new_v4();

        let app = TestApp::build_with(
            tenant_id,
            MockDriverRepository::new(),
            MockTaskRepository::new(),
            MockLocationRepository::new(),
        );
        let token = make_jwt_for_user(user_id, tenant_id);

        let body = serde_json::json!({
            "task_id": nonexistent_id,
            "reason": "Some reason"
        });

        let response = app
            .router
            .oneshot(json_request(
                Method::PUT,
                &format!("/v1/tasks/{nonexistent_id}/fail"),
                &token,
                body,
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests: Health / Readiness
// ─────────────────────────────────────────────────────────────────────────────

mod health {
    use super::*;

    #[tokio::test]
    async fn health_endpoint_responds_ok() {
        let app = TestApp::build();
        let response = app
            .router
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn ready_endpoint_responds_ok() {
        let app = TestApp::build();
        let response = app
            .router
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn health_does_not_require_auth() {
        // Health endpoint must be publicly accessible for k8s probes
        let app = TestApp::build();
        let response = app
            .router
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "/health must not require a Bearer token"
        );
    }
}
