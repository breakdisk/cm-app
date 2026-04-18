/// Integration tests for the dispatch service.
///
/// All tests use in-memory mock repositories — no real database or Kafka broker
/// is required. Each test builds its own `Router` via `build_test_app()` so
/// there is zero shared mutable state between test cases.
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
    Router,
};
use chrono::Utc;
use tower::ServiceExt; // for `oneshot`
use uuid::Uuid;

use async_trait::async_trait;
use logisticos_auth::{
    claims::Claims,
    jwt::JwtService,
    rbac::permissions,
};
use logisticos_types::{Coordinates, DriverId, RouteId, TenantId, VehicleId};

use logisticos_dispatch::{
    api::http::{router, AppState},
    application::services::DriverAssignmentService,
    domain::{
        entities::{
            route::{DeliveryStop, Route, RouteStatus, StopType},
            driver_assignment::{AssignmentStatus, DriverAssignment},
        },
        repositories::{
            AvailableDriver, DriverAssignmentRepository, DriverAvailabilityRepository,
            RouteRepository,
        },
    },
    infrastructure::db::{
        DispatchQueueRepository, DispatchQueueRow,
        DriverProfilesRepository, DriverProfileRow,
    },
};

// ---------------------------------------------------------------------------
// Mock Kafka producer
// ---------------------------------------------------------------------------

/// Build a noop `KafkaProducer` backed by rdkafka's in-process `MockCluster`.
///
/// `DriverAssignmentService` accepts `Arc<KafkaProducer>` (a concrete struct),
/// so we cannot substitute a trait object. rdkafka's `MockCluster` (enabled via
/// the `"mock"` feature) provides a fully in-process broker whose
/// `bootstrap_servers()` address we pass to `KafkaProducer::new()`.  All
/// publish calls complete instantly without network I/O.
///
/// The dispatch service `Cargo.toml` `[dev-dependencies]` must include:
///   `rdkafka = { workspace = true, features = ["mock"] }`
// ---------------------------------------------------------------------------

// The `DriverAssignmentService` holds a concrete `Arc<KafkaProducer>`.
// We construct the mock cluster using rdkafka's built-in mock support.
fn create_noop_kafka() -> Arc<logisticos_events::producer::KafkaProducer> {
    use rdkafka::mocking::MockCluster;
    let cluster = MockCluster::new(1).expect("mock kafka cluster");
    let brokers = cluster.bootstrap_servers();
    // Leak the cluster so it outlives the producer for the duration of the test.
    // This is acceptable for short-lived tests — each test creates its own cluster.
    Box::leak(Box::new(cluster));
    Arc::new(
        logisticos_events::producer::KafkaProducer::new(&brokers)
            .expect("noop kafka producer"),
    )
}

// ---------------------------------------------------------------------------
// In-memory mock repositories
// ---------------------------------------------------------------------------

/// In-memory route repository keyed by (TenantId, RouteId).
#[derive(Default, Clone)]
struct MockRouteRepo {
    store: Arc<Mutex<HashMap<Uuid, Route>>>,
}

#[async_trait]
impl RouteRepository for MockRouteRepo {
    async fn find_by_id(&self, id: &RouteId) -> anyhow::Result<Option<Route>> {
        let guard = self.store.lock().unwrap();
        Ok(guard.get(&id.inner()).cloned())
    }

    async fn find_active_by_driver(&self, driver_id: &DriverId) -> anyhow::Result<Option<Route>> {
        let guard = self.store.lock().unwrap();
        let route = guard.values().find(|r| {
            r.driver_id.inner() == driver_id.inner()
                && matches!(r.status, RouteStatus::Planned | RouteStatus::InProgress)
        });
        Ok(route.cloned())
    }

    async fn list_by_tenant(&self, tenant_id: &TenantId) -> anyhow::Result<Vec<Route>> {
        let guard = self.store.lock().unwrap();
        let routes = guard
            .values()
            .filter(|r| r.tenant_id.inner() == tenant_id.inner())
            .cloned()
            .collect();
        Ok(routes)
    }

    async fn save(&self, route: &Route) -> anyhow::Result<()> {
        let mut guard = self.store.lock().unwrap();
        guard.insert(route.id.inner(), route.clone());
        Ok(())
    }
}

/// In-memory driver assignment repository.
#[derive(Default, Clone)]
struct MockAssignmentRepo {
    store: Arc<Mutex<HashMap<Uuid, DriverAssignment>>>,
}

#[async_trait]
impl DriverAssignmentRepository for MockAssignmentRepo {
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<DriverAssignment>> {
        let guard = self.store.lock().unwrap();
        Ok(guard.get(&id).cloned())
    }

    async fn find_active_by_driver(
        &self,
        driver_id: &DriverId,
    ) -> anyhow::Result<Option<DriverAssignment>> {
        let guard = self.store.lock().unwrap();
        let assignment = guard
            .values()
            .find(|a| a.driver_id.inner() == driver_id.inner() && a.is_active());
        Ok(assignment.cloned())
    }

    async fn save(&self, assignment: &DriverAssignment) -> anyhow::Result<()> {
        let mut guard = self.store.lock().unwrap();
        guard.insert(assignment.id, assignment.clone());
        Ok(())
    }
}

/// In-memory driver availability repository.
/// The `drivers` vec is injected at construction time so each test can
/// control the pool of available drivers.
#[derive(Default, Clone)]
struct MockDriverAvailRepo {
    drivers: Arc<Mutex<Vec<AvailableDriver>>>,
}

impl MockDriverAvailRepo {
    fn with_drivers(drivers: Vec<AvailableDriver>) -> Self {
        Self {
            drivers: Arc::new(Mutex::new(drivers)),
        }
    }
}

#[async_trait]
impl DriverAvailabilityRepository for MockDriverAvailRepo {
    async fn find_available_near(
        &self,
        _tenant_id: &TenantId,
        coords: Coordinates,
        radius_km: f64,
    ) -> anyhow::Result<Vec<AvailableDriver>> {
        let guard = self.drivers.lock().unwrap();
        let nearby = guard
            .iter()
            .filter(|d| d.location.distance_km(&coords) <= radius_km)
            .cloned()
            .collect();
        Ok(nearby)
    }
}

// ---------------------------------------------------------------------------
// Mock repositories — dispatch queue and driver profiles
// ---------------------------------------------------------------------------

#[derive(Default, Clone)]
struct MockDispatchQueueRepo {
    store: Arc<Mutex<HashMap<Uuid, DispatchQueueRow>>>,
}

#[async_trait]
impl DispatchQueueRepository for MockDispatchQueueRepo {
    async fn upsert(&self, row: &DispatchQueueRow) -> anyhow::Result<()> {
        let mut guard = self.store.lock().unwrap();
        guard.insert(row.shipment_id, row.clone());
        Ok(())
    }

    async fn find_by_shipment(&self, shipment_id: Uuid) -> anyhow::Result<Option<DispatchQueueRow>> {
        let guard = self.store.lock().unwrap();
        Ok(guard.get(&shipment_id).cloned())
    }

    async fn list_pending(&self, tenant_id: Uuid) -> anyhow::Result<Vec<DispatchQueueRow>> {
        let guard = self.store.lock().unwrap();
        let rows = guard
            .values()
            .filter(|r| r.tenant_id == tenant_id && r.status == "pending")
            .cloned()
            .collect();
        Ok(rows)
    }

    async fn mark_dispatched(&self, shipment_id: Uuid) -> anyhow::Result<()> {
        let mut guard = self.store.lock().unwrap();
        if let Some(row) = guard.get_mut(&shipment_id) {
            row.status = "dispatched".to_string();
            Ok(())
        } else {
            anyhow::bail!("mark_dispatched: shipment_id {} not found in mock", shipment_id)
        }
    }
}

#[derive(Default, Clone)]
struct MockDriverProfilesRepo;

#[async_trait]
impl DriverProfilesRepository for MockDriverProfilesRepo {
    async fn upsert(&self, _row: &DriverProfileRow) -> anyhow::Result<()> {
        Ok(())
    }

    async fn list_by_tenant(&self, _tenant_id: Uuid) -> anyhow::Result<Vec<DriverProfileRow>> {
        Ok(vec![])
    }
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

const TEST_JWT_SECRET: &str = "test-secret-for-integration-tests-must-be-long-enough";
const TEST_TENANT_ID: Uuid = Uuid::from_u128(0x_dead_beef_0000_0000_0000_0000_0000_0001);

/// Mint a HS256 JWT for the given user_id with admin permissions.
fn mint_jwt_token(user_id: Uuid, permissions: Vec<String>) -> String {
    let jwt = JwtService::new(TEST_JWT_SECRET, 3600, 86400);
    let claims = Claims::new(
        user_id,
        TEST_TENANT_ID,
        "test-tenant".to_owned(),
        "business".to_owned(),
        "test@logisticos.io".to_owned(),
        vec!["admin".to_owned()],
        permissions,
        3600,
    );
    jwt.issue_access_token(claims)
        .expect("test JWT must be mintable")
}

/// Convenience: mint a token with all dispatch permissions.
fn dispatcher_token(user_id: Uuid) -> String {
    mint_jwt_token(
        user_id,
        vec![
            permissions::DISPATCH_ASSIGN.to_owned(),
            permissions::DISPATCH_VIEW.to_owned(),
            permissions::DISPATCH_REROUTE.to_owned(),
        ],
    )
}

/// Convenience: mint a token for a driver (DISPATCH_VIEW only).
fn driver_token(driver_id: Uuid) -> String {
    mint_jwt_token(driver_id, vec![permissions::DISPATCH_VIEW.to_owned()])
}

/// Build a test `Router` wired up to the provided mock repositories.
fn build_test_app(
    route_repo: MockRouteRepo,
    assignment_repo: MockAssignmentRepo,
    avail_repo: MockDriverAvailRepo,
) -> Router {
    build_test_app_with_queue(
        route_repo,
        assignment_repo,
        avail_repo,
        MockDispatchQueueRepo::default(),
    )
}

/// Build a test `Router` with a custom dispatch queue (for quick_dispatch tests).
fn build_test_app_with_queue(
    route_repo: MockRouteRepo,
    assignment_repo: MockAssignmentRepo,
    avail_repo: MockDriverAvailRepo,
    queue_repo: MockDispatchQueueRepo,
) -> Router {
    let jwt = Arc::new(JwtService::new(TEST_JWT_SECRET, 3600, 86400));
    let kafka = create_noop_kafka();
    let queue_arc: Arc<dyn DispatchQueueRepository> = Arc::new(queue_repo);
    let profiles_arc: Arc<dyn DriverProfilesRepository> = Arc::new(MockDriverProfilesRepo);

    let dispatch_service = Arc::new(DriverAssignmentService::new(
        Arc::new(route_repo) as _,
        Arc::new(assignment_repo) as _,
        Arc::new(avail_repo) as _,
        kafka,
        None,               // compliance_cache: None = always assignable in tests
        Arc::clone(&queue_arc),
    ));

    let state = Arc::new(AppState {
        dispatch_service,
        jwt,
        queue_repo:   queue_arc,
        drivers_repo: profiles_arc,
    });
    router(state)
}

/// Build a test app with empty repositories and a configurable driver pool.
fn build_app_with_drivers(drivers: Vec<AvailableDriver>) -> Router {
    build_test_app(
        MockRouteRepo::default(),
        MockAssignmentRepo::default(),
        MockDriverAvailRepo::with_drivers(drivers),
    )
}

/// Build a `Route` fixture in `Planned` status with one stop.
fn planned_route_with_stop(tenant_id: Uuid, driver_id: Uuid, vehicle_id: Uuid) -> Route {
    let stop = DeliveryStop {
        sequence: 1,
        shipment_id: Uuid::new_v4(),
        address: logisticos_types::Address {
            line1: "123 Test St".to_owned(),
            line2: None,
            barangay: None,
            city: "Manila".to_owned(),
            province: "Metro Manila".to_owned(),
            postal_code: "1000".to_owned(),
            country_code: "PH".to_owned(),
            coordinates: Some(Coordinates { lat: 14.5995, lng: 120.9842 }),
        },
        time_window_start: None,
        time_window_end: None,
        estimated_arrival: None,
        actual_arrival: None,
        stop_type: StopType::Delivery,
    };
    Route {
        id: RouteId::new(),
        tenant_id: TenantId::from_uuid(tenant_id),
        driver_id: DriverId::from_uuid(driver_id),
        vehicle_id: VehicleId::from_uuid(vehicle_id),
        stops: vec![stop],
        status: RouteStatus::Planned,
        total_distance_km: 5.2,
        estimated_duration_minutes: 30,
        created_at: Utc::now(),
        started_at: None,
        completed_at: None,
    }
}

/// Helper: send a request and return the response.
async fn send(app: Router, req: Request<Body>) -> axum::response::Response {
    app.oneshot(req).await.expect("service call must not fail")
}

/// Helper: build a JSON request.
fn json_request(method: Method, uri: &str, body: serde_json::Value, token: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {}", token))
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

/// Helper: build an empty-body authenticated request.
fn auth_request(method: Method, uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap()
}

/// Deserialise the response body to `serde_json::Value`.
async fn body_json(resp: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("response body must be readable");
    serde_json::from_slice(&bytes).expect("response body must be valid JSON")
}

// ===========================================================================
// Tests: Route creation  (POST /v1/routes)
// ===========================================================================

mod route_creation {
    use super::*;

    #[tokio::test]
    async fn create_route_returns_201_with_route_id() {
        let driver_id = Uuid::new_v4();
        let vehicle_id = Uuid::new_v4();
        let token = dispatcher_token(Uuid::new_v4());

        let app = build_app_with_drivers(vec![]);

        let body = serde_json::json!({
            "driver_id":    driver_id,
            "vehicle_id":   vehicle_id,
            "shipment_ids": []
        });

        let resp = send(
            app,
            json_request(Method::POST, "/v1/routes", body, &token),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::OK); // handler returns 200 with JSON data
        let json = body_json(resp).await;
        let route_id = json["data"]["route_id"].as_str().expect("route_id must be present");
        Uuid::parse_str(route_id).expect("route_id must be a valid UUID");
        assert_eq!(json["data"]["status"], "planned");
    }

    #[tokio::test]
    async fn create_route_conflicts_when_driver_has_active_route() {
        let driver_id = Uuid::new_v4();
        let vehicle_id = Uuid::new_v4();
        let token = dispatcher_token(Uuid::new_v4());

        // Pre-seed an active route for this driver.
        let route_repo = MockRouteRepo::default();
        let existing = planned_route_with_stop(TEST_TENANT_ID, driver_id, vehicle_id);
        {
            let mut guard = route_repo.store.lock().unwrap();
            guard.insert(existing.id.inner(), existing);
        }

        let app = build_test_app(
            route_repo,
            MockAssignmentRepo::default(),
            MockDriverAvailRepo::default(),
        );

        let body = serde_json::json!({
            "driver_id":    driver_id,
            "vehicle_id":   vehicle_id,
            "shipment_ids": []
        });

        let resp = send(
            app,
            json_request(Method::POST, "/v1/routes", body, &token),
        )
        .await;

        // BusinessRule error → 422 (UNPROCESSABLE_ENTITY) per AppError mapping.
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let json = body_json(resp).await;
        assert_eq!(json["error"]["code"], "BUSINESS_RULE_VIOLATION");
    }

    #[tokio::test]
    async fn create_route_requires_auth() {
        let app = build_app_with_drivers(vec![]);

        let body = serde_json::json!({
            "driver_id":    Uuid::new_v4(),
            "vehicle_id":   Uuid::new_v4(),
            "shipment_ids": []
        });

        let resp = send(
            app,
            Request::builder()
                .method(Method::POST)
                .uri("/v1/routes")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}

// ===========================================================================
// Tests: Driver auto-assignment  (POST /v1/routes/:id/assign)
// ===========================================================================

mod auto_assign {
    use super::*;

    /// Creates a route in the route_repo and returns the repo + route_id.
    fn seeded_route_repo(
        driver_id: Uuid,
        vehicle_id: Uuid,
    ) -> (MockRouteRepo, Uuid) {
        let repo = MockRouteRepo::default();
        let route = planned_route_with_stop(TEST_TENANT_ID, driver_id, vehicle_id);
        let route_id = route.id.inner();
        {
            let mut guard = repo.store.lock().unwrap();
            guard.insert(route_id, route);
        }
        (repo, route_id)
    }

    #[tokio::test]
    async fn assigns_nearest_available_driver_within_25km() {
        let vehicle_id = Uuid::new_v4();
        let route_driver = Uuid::new_v4(); // the driver already assigned to the route fixture
        let available_driver_id = Uuid::new_v4();
        let token = dispatcher_token(Uuid::new_v4());

        let (route_repo, route_id) = seeded_route_repo(route_driver, vehicle_id);

        // Place an available driver 5km from Manila (14.5995, 120.9842)
        let drivers = vec![AvailableDriver {
            driver_id: DriverId::from_uuid(available_driver_id),
            name: "Juan Dela Cruz".to_owned(),
            distance_km: 5.0,
            location: Coordinates { lat: 14.6400, lng: 120.9900 },
            active_stop_count: 0,
            vehicle_type: None,
        }];

        let app = build_test_app(
            route_repo,
            MockAssignmentRepo::default(),
            MockDriverAvailRepo::with_drivers(drivers),
        );

        let resp = send(
            app,
            json_request(
                Method::POST,
                &format!("/v1/routes/{route_id}/assign"),
                serde_json::json!({}),
                &token,
            ),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(
            json["data"]["driver_id"].as_str().unwrap(),
            available_driver_id.to_string()
        );
        assert!(json["data"]["assignment_id"].as_str().is_some());
        assert_eq!(json["data"]["status"], "pending");
    }

    #[tokio::test]
    async fn returns_422_when_no_drivers_in_radius() {
        let vehicle_id = Uuid::new_v4();
        let route_driver = Uuid::new_v4();
        let token = dispatcher_token(Uuid::new_v4());

        let (route_repo, route_id) = seeded_route_repo(route_driver, vehicle_id);

        // Place the only driver well outside 25km radius.
        let drivers = vec![AvailableDriver {
            driver_id: DriverId::from_uuid(Uuid::new_v4()),
            name: "Far Driver".to_owned(),
            distance_km: 50.0,
            location: Coordinates { lat: 15.5000, lng: 120.9842 }, // ~100km north
            active_stop_count: 0,
            vehicle_type: None,
        }];

        let app = build_test_app(
            route_repo,
            MockAssignmentRepo::default(),
            MockDriverAvailRepo::with_drivers(drivers),
        );

        let resp = send(
            app,
            json_request(
                Method::POST,
                &format!("/v1/routes/{route_id}/assign"),
                serde_json::json!({}),
                &token,
            ),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let json = body_json(resp).await;
        assert_eq!(json["error"]["code"], "BUSINESS_RULE_VIOLATION");
    }

    #[tokio::test]
    async fn returns_404_for_nonexistent_route() {
        let token = dispatcher_token(Uuid::new_v4());
        let nonexistent_id = Uuid::new_v4();

        let app = build_app_with_drivers(vec![]);

        let resp = send(
            app,
            json_request(
                Method::POST,
                &format!("/v1/routes/{nonexistent_id}/assign"),
                serde_json::json!({}),
                &token,
            ),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn selects_closest_when_multiple_drivers_available() {
        let vehicle_id = Uuid::new_v4();
        let route_driver = Uuid::new_v4();
        let close_driver_id = Uuid::new_v4();
        let far_driver_id = Uuid::new_v4();
        let token = dispatcher_token(Uuid::new_v4());

        let (route_repo, route_id) = seeded_route_repo(route_driver, vehicle_id);

        // Scoring formula: distance_km * 0.7 + stop_load * 0.3
        // close: 2.0 * 0.7 + 0 * 0.3 = 1.4  (wins)
        // far:   8.0 * 0.7 + 0 * 0.3 = 5.6
        let drivers = vec![
            AvailableDriver {
                driver_id: DriverId::from_uuid(far_driver_id),
                name: "Far Driver".to_owned(),
                distance_km: 8.0,
                location: Coordinates { lat: 14.6700, lng: 120.9842 },
                active_stop_count: 0,
            vehicle_type: None,
            },
            AvailableDriver {
                driver_id: DriverId::from_uuid(close_driver_id),
                name: "Close Driver".to_owned(),
                distance_km: 2.0,
                location: Coordinates { lat: 14.6100, lng: 120.9842 },
                active_stop_count: 0,
            vehicle_type: None,
            },
        ];

        let app = build_test_app(
            route_repo,
            MockAssignmentRepo::default(),
            MockDriverAvailRepo::with_drivers(drivers),
        );

        let resp = send(
            app,
            json_request(
                Method::POST,
                &format!("/v1/routes/{route_id}/assign"),
                serde_json::json!({}),
                &token,
            ),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(
            json["data"]["driver_id"].as_str().unwrap(),
            close_driver_id.to_string()
        );
    }

    #[tokio::test]
    async fn honours_preferred_driver_id() {
        let vehicle_id = Uuid::new_v4();
        let route_driver = Uuid::new_v4();
        let preferred_driver_id = Uuid::new_v4();
        let token = dispatcher_token(Uuid::new_v4());

        let (route_repo, route_id) = seeded_route_repo(route_driver, vehicle_id);

        // Only the preferred driver is in the pool; they are within radius.
        let drivers = vec![AvailableDriver {
            driver_id: DriverId::from_uuid(preferred_driver_id),
            name: "Preferred Driver".to_owned(),
            distance_km: 3.0,
            location: Coordinates { lat: 14.6200, lng: 120.9842 },
            active_stop_count: 0,
            vehicle_type: None,
        }];

        let app = build_test_app(
            route_repo,
            MockAssignmentRepo::default(),
            MockDriverAvailRepo::with_drivers(drivers),
        );

        let resp = send(
            app,
            json_request(
                Method::POST,
                &format!("/v1/routes/{route_id}/assign"),
                serde_json::json!({ "preferred_driver_id": preferred_driver_id }),
                &token,
            ),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(
            json["data"]["driver_id"].as_str().unwrap(),
            preferred_driver_id.to_string()
        );
    }
}

// ===========================================================================
// Tests: Route status transitions
// ===========================================================================

mod route_status_transitions {
    use super::*;

    /// Seed a route with an accepted assignment in the repos.
    /// Returns (route_repo, assignment_repo, route_id, assignment_id, driver_id).
    fn seed_route_with_accepted_assignment(
        status: RouteStatus,
    ) -> (MockRouteRepo, MockAssignmentRepo, Uuid, Uuid, Uuid) {
        let driver_id = Uuid::new_v4();
        let vehicle_id = Uuid::new_v4();

        let route_repo = MockRouteRepo::default();
        let assignment_repo = MockAssignmentRepo::default();

        let mut route = planned_route_with_stop(TEST_TENANT_ID, driver_id, vehicle_id);
        route.status = status;
        if matches!(status, RouteStatus::InProgress) {
            route.started_at = Some(Utc::now());
        }
        let route_id = route.id.inner();

        let mut assignment =
            DriverAssignment::new(
                TenantId::from_uuid(TEST_TENANT_ID),
                DriverId::from_uuid(driver_id),
                route.id.clone(),
            );
        assignment.accept().unwrap();
        let assignment_id = assignment.id;

        {
            let mut rg = route_repo.store.lock().unwrap();
            rg.insert(route_id, route);
        }
        {
            let mut ag = assignment_repo.store.lock().unwrap();
            ag.insert(assignment_id, assignment);
        }

        (route_repo, assignment_repo, route_id, assignment_id, driver_id)
    }

    // -----------------------------------------------------------------------
    // Accept assignment → route InProgress
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn accept_assignment_transitions_route_to_in_progress() {
        let driver_id = Uuid::new_v4();
        let vehicle_id = Uuid::new_v4();
        let token_driver = driver_token(driver_id);

        let route_repo = MockRouteRepo::default();
        let assignment_repo = MockAssignmentRepo::default();

        let route = planned_route_with_stop(TEST_TENANT_ID, driver_id, vehicle_id);
        let route_id = route.id.inner();
        {
            route_repo.store.lock().unwrap().insert(route_id, route.clone());
        }

        let assignment = DriverAssignment::new(
            TenantId::from_uuid(TEST_TENANT_ID),
            DriverId::from_uuid(driver_id),
            route.id.clone(),
        );
        let assignment_id = assignment.id;
        {
            assignment_repo.store.lock().unwrap().insert(assignment_id, assignment);
        }

        let app = build_test_app(
            route_repo.clone(),
            assignment_repo.clone(),
            MockDriverAvailRepo::default(),
        );

        let resp = send(
            app,
            auth_request(Method::PUT, &format!("/v1/assignments/{assignment_id}/accept"), &token_driver),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // Verify route status changed to InProgress in the repo.
        let guard = route_repo.store.lock().unwrap();
        let updated_route = guard.get(&route_id).unwrap();
        assert_eq!(updated_route.status, RouteStatus::InProgress);
        assert!(updated_route.started_at.is_some());
    }

    #[tokio::test]
    async fn accept_returns_404_for_nonexistent_assignment() {
        let driver_id = Uuid::new_v4();
        let token = driver_token(driver_id);
        let nonexistent_id = Uuid::new_v4();

        let app = build_test_app(
            MockRouteRepo::default(),
            MockAssignmentRepo::default(),
            MockDriverAvailRepo::default(),
        );

        let resp = send(
            app,
            auth_request(
                Method::PUT,
                &format!("/v1/assignments/{nonexistent_id}/accept"),
                &token,
            ),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn accept_twice_returns_422() {
        let driver_id = Uuid::new_v4();
        let vehicle_id = Uuid::new_v4();
        let token = driver_token(driver_id);

        let route_repo = MockRouteRepo::default();
        let assignment_repo = MockAssignmentRepo::default();

        let route = planned_route_with_stop(TEST_TENANT_ID, driver_id, vehicle_id);
        let route_id = route.id.inner();
        route_repo.store.lock().unwrap().insert(route_id, route.clone());

        // Already accepted assignment.
        let mut assignment = DriverAssignment::new(
            TenantId::from_uuid(TEST_TENANT_ID),
            DriverId::from_uuid(driver_id),
            route.id.clone(),
        );
        assignment.accept().unwrap();
        let assignment_id = assignment.id;
        assignment_repo.store.lock().unwrap().insert(assignment_id, assignment);

        let app = build_test_app(route_repo, assignment_repo, MockDriverAvailRepo::default());

        let resp = send(
            app,
            auth_request(
                Method::PUT,
                &format!("/v1/assignments/{assignment_id}/accept"),
                &token,
            ),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let json = body_json(resp).await;
        assert_eq!(json["error"]["code"], "BUSINESS_RULE_VIOLATION");
    }

    // -----------------------------------------------------------------------
    // Reject assignment
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn reject_assignment_stores_rejection_reason() {
        let driver_id = Uuid::new_v4();
        let vehicle_id = Uuid::new_v4();
        let token = driver_token(driver_id);

        let route_repo = MockRouteRepo::default();
        let assignment_repo = MockAssignmentRepo::default();

        let route = planned_route_with_stop(TEST_TENANT_ID, driver_id, vehicle_id);
        route_repo.store.lock().unwrap().insert(route.id.inner(), route.clone());

        let assignment = DriverAssignment::new(
            TenantId::from_uuid(TEST_TENANT_ID),
            DriverId::from_uuid(driver_id),
            route.id,
        );
        let assignment_id = assignment.id;
        assignment_repo.store.lock().unwrap().insert(assignment_id, assignment);

        let app = build_test_app(
            route_repo,
            assignment_repo.clone(),
            MockDriverAvailRepo::default(),
        );

        let resp = send(
            app,
            json_request(
                Method::PUT,
                &format!("/v1/assignments/{assignment_id}/reject"),
                serde_json::json!({ "reason": "Fuel issue" }),
                &token,
            ),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // Verify assignment status is Rejected and reason is stored.
        let guard = assignment_repo.store.lock().unwrap();
        let updated = guard.get(&assignment_id).unwrap();
        assert_eq!(updated.status, AssignmentStatus::Rejected);
        assert_eq!(
            updated.rejection_reason.as_deref(),
            Some("Fuel issue")
        );
    }

    #[tokio::test]
    async fn reject_returns_404_for_nonexistent_assignment() {
        let driver_id = Uuid::new_v4();
        let token = driver_token(driver_id);

        let app = build_test_app(
            MockRouteRepo::default(),
            MockAssignmentRepo::default(),
            MockDriverAvailRepo::default(),
        );

        let resp = send(
            app,
            json_request(
                Method::PUT,
                &format!("/v1/assignments/{}/reject", Uuid::new_v4()),
                serde_json::json!({ "reason": "Too far" }),
                &token,
            ),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn reject_by_wrong_driver_returns_403() {
        let driver_id = Uuid::new_v4();
        let other_driver_id = Uuid::new_v4(); // impersonator
        let vehicle_id = Uuid::new_v4();
        let token = driver_token(other_driver_id); // token for a different driver

        let route_repo = MockRouteRepo::default();
        let assignment_repo = MockAssignmentRepo::default();

        let route = planned_route_with_stop(TEST_TENANT_ID, driver_id, vehicle_id);
        route_repo.store.lock().unwrap().insert(route.id.inner(), route.clone());

        let assignment = DriverAssignment::new(
            TenantId::from_uuid(TEST_TENANT_ID),
            DriverId::from_uuid(driver_id),
            route.id,
        );
        let assignment_id = assignment.id;
        assignment_repo.store.lock().unwrap().insert(assignment_id, assignment);

        let app = build_test_app(route_repo, assignment_repo, MockDriverAvailRepo::default());

        let resp = send(
            app,
            json_request(
                Method::PUT,
                &format!("/v1/assignments/{assignment_id}/reject"),
                serde_json::json!({ "reason": "Impersonation attempt" }),
                &token,
            ),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    // -----------------------------------------------------------------------
    // Reject → auto-re-assign: after rejection a new assignment should be
    // creatable for a different driver (business contract check).
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn after_rejection_new_assignment_can_be_created() {
        let driver_id = Uuid::new_v4();
        let second_driver_id = Uuid::new_v4();
        let vehicle_id = Uuid::new_v4();
        let dispatcher_token = dispatcher_token(Uuid::new_v4());
        let driver_token = driver_token(driver_id);

        let route_repo = MockRouteRepo::default();
        let assignment_repo = MockAssignmentRepo::default();

        // Planned route with one stop.
        let route = planned_route_with_stop(TEST_TENANT_ID, driver_id, vehicle_id);
        let route_id = route.id.inner();
        route_repo.store.lock().unwrap().insert(route_id, route.clone());

        // Original assignment for driver_id.
        let original_assignment = DriverAssignment::new(
            TenantId::from_uuid(TEST_TENANT_ID),
            DriverId::from_uuid(driver_id),
            route.id.clone(),
        );
        let original_assignment_id = original_assignment.id;
        assignment_repo.store.lock().unwrap().insert(original_assignment_id, original_assignment);

        // Second available driver is nearby.
        let avail_repo = MockDriverAvailRepo::with_drivers(vec![AvailableDriver {
            driver_id: DriverId::from_uuid(second_driver_id),
            name: "Second Driver".to_owned(),
            distance_km: 4.0,
            location: Coordinates { lat: 14.6200, lng: 120.9842 },
            active_stop_count: 0,
            vehicle_type: None,
        }]);

        let app = build_test_app(route_repo.clone(), assignment_repo.clone(), avail_repo);

        // Step 1: Driver rejects the assignment.
        let reject_resp = send(
            app.clone(),
            json_request(
                Method::PUT,
                &format!("/v1/assignments/{original_assignment_id}/reject"),
                serde_json::json!({ "reason": "Vehicle breakdown" }),
                &driver_token,
            ),
        )
        .await;
        assert_eq!(reject_resp.status(), StatusCode::NO_CONTENT);

        // Step 2: Dispatcher triggers a new auto-assignment for the same route.
        let reassign_resp = send(
            app,
            json_request(
                Method::POST,
                &format!("/v1/routes/{route_id}/assign"),
                serde_json::json!({}),
                &dispatcher_token,
            ),
        )
        .await;
        assert_eq!(reassign_resp.status(), StatusCode::OK);
        let json = body_json(reassign_resp).await;
        let new_driver = json["data"]["driver_id"].as_str().unwrap();
        assert_eq!(new_driver, second_driver_id.to_string());
    }
}

// ===========================================================================
// Tests: List routes  (GET /v1/routes)
// ===========================================================================

mod list_routes {
    use super::*;

    #[tokio::test]
    async fn returns_routes_for_calling_tenant() {
        let driver_id = Uuid::new_v4();
        let vehicle_id = Uuid::new_v4();
        let token = dispatcher_token(Uuid::new_v4());

        let route_repo = MockRouteRepo::default();
        let route = planned_route_with_stop(TEST_TENANT_ID, driver_id, vehicle_id);
        {
            route_repo.store.lock().unwrap().insert(route.id.inner(), route);
        }

        let app = build_test_app(
            route_repo,
            MockAssignmentRepo::default(),
            MockDriverAvailRepo::default(),
        );

        let resp = send(app, auth_request(Method::GET, "/v1/routes", &token)).await;

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        let routes = json["data"].as_array().expect("data must be an array");
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0]["status"], "Planned");
    }

    #[tokio::test]
    async fn returns_empty_list_when_no_routes() {
        let token = dispatcher_token(Uuid::new_v4());
        let app = build_app_with_drivers(vec![]);

        let resp = send(app, auth_request(Method::GET, "/v1/routes", &token)).await;

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        let routes = json["data"].as_array().expect("data must be an array");
        assert!(routes.is_empty());
    }

    #[tokio::test]
    async fn returns_401_without_token() {
        let app = build_app_with_drivers(vec![]);

        let resp = send(
            app,
            Request::builder()
                .method(Method::GET)
                .uri("/v1/routes")
                .body(Body::empty())
                .unwrap(),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}

// ===========================================================================
// Tests: Get single route  (GET /v1/routes/:id)
// ===========================================================================

mod get_route {
    use super::*;

    #[tokio::test]
    async fn returns_route_by_id() {
        let driver_id = Uuid::new_v4();
        let vehicle_id = Uuid::new_v4();
        let token = dispatcher_token(Uuid::new_v4());

        let route_repo = MockRouteRepo::default();
        let route = planned_route_with_stop(TEST_TENANT_ID, driver_id, vehicle_id);
        let route_id = route.id.inner();
        {
            route_repo.store.lock().unwrap().insert(route_id, route);
        }

        let app = build_test_app(
            route_repo,
            MockAssignmentRepo::default(),
            MockDriverAvailRepo::default(),
        );

        let resp = send(
            app,
            auth_request(Method::GET, &format!("/v1/routes/{route_id}"), &token),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(
            json["data"]["id"].as_str().unwrap(),
            route_id.to_string()
        );
    }

    #[tokio::test]
    async fn returns_404_for_nonexistent_route() {
        let token = dispatcher_token(Uuid::new_v4());
        let app = build_app_with_drivers(vec![]);
        let fake_id = Uuid::new_v4();

        let resp = send(
            app,
            auth_request(Method::GET, &format!("/v1/routes/{fake_id}"), &token),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn returns_404_for_route_belonging_to_different_tenant() {
        let other_tenant_id = Uuid::new_v4(); // different from TEST_TENANT_ID
        let driver_id = Uuid::new_v4();
        let vehicle_id = Uuid::new_v4();
        let token = dispatcher_token(Uuid::new_v4()); // token has TEST_TENANT_ID

        let route_repo = MockRouteRepo::default();
        let mut route = planned_route_with_stop(other_tenant_id, driver_id, vehicle_id);
        route.tenant_id = TenantId::from_uuid(other_tenant_id);
        let route_id = route.id.inner();
        {
            route_repo.store.lock().unwrap().insert(route_id, route);
        }

        let app = build_test_app(
            route_repo,
            MockAssignmentRepo::default(),
            MockDriverAvailRepo::default(),
        );

        let resp = send(
            app,
            auth_request(Method::GET, &format!("/v1/routes/{route_id}"), &token),
        )
        .await;

        // Tenant isolation: route exists but belongs to another tenant → 404.
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}

// ===========================================================================
// Tests: Health endpoint  (GET /health)
// ===========================================================================

mod health {
    use super::*;

    #[tokio::test]
    async fn health_returns_200() {
        let app = build_app_with_drivers(vec![]);

        let resp = send(
            app,
            Request::builder()
                .method(Method::GET)
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::OK);
    }
}

// ===========================================================================
// Tests: Quick dispatch  (POST /v1/queue/:shipment_id/dispatch)
// ===========================================================================

mod quick_dispatch {
    use super::*;
    use logisticos_auth::rbac::permissions;

    fn make_queue_item(tenant_id: Uuid, shipment_id: Uuid) -> DispatchQueueRow {
        DispatchQueueRow {
            id:                   Uuid::new_v4(),
            tenant_id,
            shipment_id,
            customer_name:        "Test Customer".into(),
            customer_phone:       "+63912345678".into(),
            customer_email:       None,
            tracking_number:      None,
            dest_address_line1:   "456 Delivery St".into(),
            dest_city:            "Manila".into(),
            dest_province:        "Metro Manila".into(),
            dest_postal_code:     "1000".into(),
            dest_lat:             Some(14.5995),
            dest_lng:             Some(120.9842),
            origin_address_line1: "123 Pickup Ave".into(),
            origin_city:          "Quezon City".into(),
            origin_province:      "Metro Manila".into(),
            origin_postal_code:   "1100".into(),
            origin_lat:           Some(14.6760),
            origin_lng:           Some(121.0437),
            cod_amount_cents:     None,
            special_instructions: None,
            service_type:         "standard".into(),
            status:               "pending".into(),
        }
    }

    #[tokio::test]
    async fn dispatches_shipment_with_origin_and_marks_queue_item_dispatched() {
        let shipment_id = Uuid::new_v4();
        let driver_id   = Uuid::new_v4();
        let token = mint_jwt_token(
            Uuid::new_v4(),
            vec![permissions::DISPATCH_ASSIGN.to_owned()],
        );

        let queue_repo = MockDispatchQueueRepo::default();
        queue_repo.store.lock().unwrap().insert(
            shipment_id,
            make_queue_item(TEST_TENANT_ID, shipment_id),
        );

        let driver = AvailableDriver {
            driver_id:         DriverId::from_uuid(driver_id),
            name:              String::new(),
            location:          Coordinates { lat: 14.60, lng: 120.98 },
            active_stop_count: 0,
            vehicle_type:      None,
            distance_km:       0.5,
        };

        let app = build_test_app_with_queue(
            MockRouteRepo::default(),
            MockAssignmentRepo::default(),
            MockDriverAvailRepo::with_drivers(vec![driver]),
            queue_repo.clone(),
        );

        let resp = send(
            app,
            json_request(
                Method::POST,
                &format!("/v1/queue/{shipment_id}/dispatch"),
                serde_json::json!({}),
                &token,
            ),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert!(json["data"]["assignment_id"].as_str().is_some(), "assignment_id missing");
        assert_eq!(json["data"]["driver_id"].as_str().unwrap(), driver_id.to_string());

        // Queue item must be marked dispatched
        let status = queue_repo.store.lock().unwrap()
            .get(&shipment_id)
            .map(|r| r.status.clone());
        assert_eq!(status, Some("dispatched".into()));
    }

    #[tokio::test]
    async fn dispatches_shipment_without_origin_succeeds_delivery_only() {
        let shipment_id = Uuid::new_v4();
        let driver_id   = Uuid::new_v4();
        let token = mint_jwt_token(
            Uuid::new_v4(),
            vec![permissions::DISPATCH_ASSIGN.to_owned()],
        );

        let queue_repo = MockDispatchQueueRepo::default();
        let mut item = make_queue_item(TEST_TENANT_ID, shipment_id);
        item.origin_address_line1 = String::new();
        item.origin_lat           = None;
        item.origin_lng           = None;
        queue_repo.store.lock().unwrap().insert(shipment_id, item);

        let driver = AvailableDriver {
            driver_id:         DriverId::from_uuid(driver_id),
            name:              String::new(),
            location:          Coordinates { lat: 14.60, lng: 120.98 },
            active_stop_count: 0,
            vehicle_type:      None,
            distance_km:       0.5,
        };

        let app = build_test_app_with_queue(
            MockRouteRepo::default(),
            MockAssignmentRepo::default(),
            MockDriverAvailRepo::with_drivers(vec![driver]),
            queue_repo.clone(),
        );

        let resp = send(
            app,
            json_request(
                Method::POST,
                &format!("/v1/queue/{shipment_id}/dispatch"),
                serde_json::json!({}),
                &token,
            ),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn returns_404_when_shipment_not_in_queue() {
        let token = mint_jwt_token(
            Uuid::new_v4(),
            vec![permissions::DISPATCH_ASSIGN.to_owned()],
        );
        let unknown_id = Uuid::new_v4();

        let app = build_test_app_with_queue(
            MockRouteRepo::default(),
            MockAssignmentRepo::default(),
            MockDriverAvailRepo::default(),
            MockDispatchQueueRepo::default(),
        );

        let resp = send(
            app,
            json_request(
                Method::POST,
                &format!("/v1/queue/{unknown_id}/dispatch"),
                serde_json::json!({}),
                &token,
            ),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn returns_422_when_no_driver_available() {
        let shipment_id = Uuid::new_v4();
        let token = mint_jwt_token(
            Uuid::new_v4(),
            vec![permissions::DISPATCH_ASSIGN.to_owned()],
        );

        let queue_repo = MockDispatchQueueRepo::default();
        queue_repo.store.lock().unwrap().insert(
            shipment_id,
            make_queue_item(TEST_TENANT_ID, shipment_id),
        );

        // No drivers in the pool
        let app = build_test_app_with_queue(
            MockRouteRepo::default(),
            MockAssignmentRepo::default(),
            MockDriverAvailRepo::default(),
            queue_repo,
        );

        let resp = send(
            app,
            json_request(
                Method::POST,
                &format!("/v1/queue/{shipment_id}/dispatch"),
                serde_json::json!({}),
                &token,
            ),
        )
        .await;

        // "No available drivers nearby" is a BusinessRule → 422
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}
