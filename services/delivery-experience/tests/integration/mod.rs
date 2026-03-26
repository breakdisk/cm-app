// Integration tests for the delivery-experience service HTTP API.
//
// Tests are organized around the route surface:
//  - Public routes (no auth):
//      GET /track/:tracking_number               — public tracking page
//      GET /track/:tracking_number               — 404 for unknown number
//  - Authenticated routes (Bearer JWT required):
//      GET /v1/tracking/:shipment_id             — full detail by shipment id
//      GET /v1/tracking                          — list with pagination
//
// The following endpoints are tested via inline handler wrappers mounted on a
// parallel test router, because the production router does not expose them yet
// as distinct routes (they are covered by internal Kafka projection consumers):
//  - PATCH /shipments/:id/status                — push status update (internal)
//  - POST /shipments/:id/events                 — add status event (internal)
//  - GET /track/:tracking_number/driver-location — live driver position
//  - GET /track/:tracking_number/timeline        — full event history
//  - POST /track/:tracking_number/reschedule     — customer reschedules
//  - POST /track/:tracking_number/feedback       — customer submits 1-5 star rating
//  - PATCH /preferences/:shipment_id            — update delivery preferences
//
// Strategy:
//  1. Build `MockTrackingRepository` keyed by tracking_number (String).
//  2. Wire the real `TrackingService` on top of the mock.
//  3. Mount the production Axum router (`api::http::router()`) plus additional
//     test-only routes that exercise the write/projection path.
//  4. JWT tokens are issued with the test secret used across the project.
//  5. Public endpoints carry no Authorization header.
//  6. Internal/authenticated endpoints carry `Authorization: Bearer <token>`.

#![allow(clippy::arc_with_non_send_sync)]

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
    Router,
};
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

use async_trait::async_trait;
use chrono::Utc;

use logisticos_auth::{claims::Claims, jwt::JwtService};
use logisticos_errors::AppError;
use logisticos_types::TenantId;

use logisticos_delivery_experience::{
    application::services::TrackingService,
    domain::{
        entities::{TrackingRecord, TrackingStatus},
        repositories::TrackingRepository,
    },
    AppState,
};

// ─────────────────────────────────────────────────────────────────────────────
// JWT test helpers
// ─────────────────────────────────────────────────────────────────────────────

const TEST_JWT_SECRET: &str = "test-secret-key-for-logisticos-testing";

fn build_jwt_service() -> JwtService {
    JwtService::new(TEST_JWT_SECRET, 3600, 86400)
}

/// Issue a JWT with `tracking:write` and `shipments:read` permissions
/// for the given tenant/user.
fn make_internal_token(tenant_id: Uuid, user_id: Uuid) -> String {
    let svc = build_jwt_service();
    let claims = Claims::new(
        user_id,
        tenant_id,
        "test-tenant".into(),
        "business".into(),
        "ops@test.logisticos.io".into(),
        vec!["admin".into()],
        vec![
            "tracking:write".into(),
            "shipments:read".into(),
            "shipments:update".into(),
            "*".into(), // superadmin wildcard for test convenience
        ],
        3600,
    );
    svc.issue_access_token(claims).expect("JWT issue failed")
}

/// Issue a JWT scoped only to shipment reads (no write).
fn make_read_only_token(tenant_id: Uuid) -> String {
    let svc = build_jwt_service();
    let claims = Claims::new(
        Uuid::new_v4(),
        tenant_id,
        "test-tenant".into(),
        "starter".into(),
        "merchant@test.logisticos.io".into(),
        vec!["merchant".into()],
        vec!["shipments:read".into()],
        3600,
    );
    svc.issue_access_token(claims).expect("JWT issue failed")
}

/// Issue a JWT for a different tenant (cross-tenant isolation test).
fn make_other_tenant_token() -> String {
    let svc = build_jwt_service();
    let claims = Claims::new(
        Uuid::new_v4(),
        Uuid::new_v4(), // different tenant
        "other-tenant".into(),
        "growth".into(),
        "other@example.com".into(),
        vec!["merchant".into()],
        vec!["shipments:read".into()],
        3600,
    );
    svc.issue_access_token(claims).expect("JWT issue failed")
}

fn auth_header(token: &str) -> (&'static str, String) {
    ("authorization", format!("Bearer {}", token))
}

// ─────────────────────────────────────────────────────────────────────────────
// MockTrackingRepository — in-memory, keyed by tracking_number
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Default, Clone)]
struct MockTrackingRepo {
    /// Primary index: tracking_number → record
    by_number: Arc<Mutex<HashMap<String, TrackingRecord>>>,
    /// Secondary index: shipment_id → tracking_number (for O(1) lookup)
    by_shipment: Arc<Mutex<HashMap<Uuid, String>>>,
}

impl MockTrackingRepo {
    fn new() -> Self {
        Self::default()
    }

    fn seed(&self, record: TrackingRecord) {
        let tn = record.tracking_number.clone();
        let sid = record.shipment_id;
        self.by_number.lock().unwrap().insert(tn.clone(), record);
        self.by_shipment.lock().unwrap().insert(sid, tn);
    }

    /// Read the current stored state of a record by tracking number.
    fn get_by_number(&self, tn: &str) -> Option<TrackingRecord> {
        self.by_number.lock().unwrap().get(tn).cloned()
    }

    fn count(&self) -> usize {
        self.by_number.lock().unwrap().len()
    }
}

#[async_trait]
impl TrackingRepository for MockTrackingRepo {
    async fn find_by_shipment_id(&self, shipment_id: Uuid) -> anyhow::Result<Option<TrackingRecord>> {
        let guard_ship = self.by_shipment.lock().unwrap();
        let tn = match guard_ship.get(&shipment_id) {
            Some(tn) => tn.clone(),
            None => return Ok(None),
        };
        drop(guard_ship);
        Ok(self.by_number.lock().unwrap().get(&tn).cloned())
    }

    async fn find_by_tracking_number(&self, tracking_number: &str) -> anyhow::Result<Option<TrackingRecord>> {
        Ok(self.by_number.lock().unwrap().get(tracking_number).cloned())
    }

    async fn list_by_tenant(
        &self,
        tenant_id: &TenantId,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<TrackingRecord>> {
        let guard = self.by_number.lock().unwrap();
        let records: Vec<TrackingRecord> = guard
            .values()
            .filter(|r| r.tenant_id.inner() == tenant_id.inner())
            .cloned()
            .collect();
        let offset = offset as usize;
        let limit = limit as usize;
        Ok(records.into_iter().skip(offset).take(limit).collect())
    }

    async fn save(&self, record: &TrackingRecord) -> anyhow::Result<()> {
        let tn = record.tracking_number.clone();
        let sid = record.shipment_id;
        self.by_number.lock().unwrap().insert(tn.clone(), record.clone());
        self.by_shipment.lock().unwrap().insert(sid, tn);
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Test fixtures
// ─────────────────────────────────────────────────────────────────────────────

fn fixed_tenant_id() -> Uuid {
    Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap()
}

fn fixed_user_id() -> Uuid {
    Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").unwrap()
}

fn make_tracking_record(tn: &str, tenant_id: Uuid) -> TrackingRecord {
    TrackingRecord::new(
        Uuid::new_v4(),
        TenantId::from_uuid(tenant_id),
        tn.to_string(),
        "Hub NCR, Pasay City, Metro Manila".into(),
        "456 Rizal St, Marikina City, Metro Manila".into(),
    )
}

fn make_delivered_record(tn: &str, tenant_id: Uuid) -> TrackingRecord {
    let mut record = make_tracking_record(tn, tenant_id);
    record.mark_delivered(
        Uuid::new_v4(),
        "Maria Santos".into(),
        Utc::now(),
    );
    record
}

fn make_out_for_delivery_record(tn: &str, tenant_id: Uuid) -> TrackingRecord {
    let mut record = make_tracking_record(tn, tenant_id);
    record.assign_driver(
        Uuid::new_v4(),
        "Jose Rizal".into(),
        "+63 917 123 4567".into(),
        Some(Utc::now() + chrono::Duration::hours(1)),
    );
    record.transition(
        TrackingStatus::OutForDelivery,
        "Driver is on the way".into(),
        None,
    );
    record.update_driver_position(14.5995, 120.9842);
    record
}

// ─────────────────────────────────────────────────────────────────────────────
// Test app builders
// ─────────────────────────────────────────────────────────────────────────────

struct TestApp {
    router: Router,
    repo: Arc<MockTrackingRepo>,
    tenant_id: Uuid,
    user_id: Uuid,
}

impl TestApp {
    fn build() -> Self {
        let repo = Arc::new(MockTrackingRepo::new());
        let tracking_svc = Arc::new(TrackingService::new(
            repo.clone() as Arc<dyn TrackingRepository>,
        ));
        let state = AppState { tracking_svc };

        // Mount the production router plus the auth middleware for /v1 routes.
        let jwt_svc = Arc::new(build_jwt_service());
        let auth_state = jwt_svc.clone();

        let router = logisticos_delivery_experience::api::http::router()
            .layer(axum::middleware::from_fn_with_state(
                auth_state,
                logisticos_auth::middleware::require_auth,
            ))
            .with_state(state);

        Self {
            router,
            repo,
            tenant_id: fixed_tenant_id(),
            user_id: fixed_user_id(),
        }
    }

    fn internal_token(&self) -> String {
        make_internal_token(self.tenant_id, self.user_id)
    }

    fn read_only_token(&self) -> String {
        make_read_only_token(self.tenant_id)
    }

    fn seed(&self, record: TrackingRecord) {
        self.repo.seed(record);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper: send request and parse response body as JSON
// ─────────────────────────────────────────────────────────────────────────────

async fn send(app: Router, req: Request<Body>) -> (StatusCode, Value) {
    let resp = app.oneshot(req).await.expect("request failed");
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("body read failed");
    let json: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

fn get_req(path: &str) -> Request<Body> {
    Request::builder()
        .method(Method::GET)
        .uri(path)
        .body(Body::empty())
        .unwrap()
}

fn get_req_auth(path: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method(Method::GET)
        .uri(path)
        .header(header::AUTHORIZATION, format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap()
}

fn post_req_json(path: &str, payload: &Value) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(payload).unwrap()))
        .unwrap()
}

fn post_req_json_auth(path: &str, payload: &Value, token: &str) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {}", token))
        .body(Body::from(serde_json::to_vec(payload).unwrap()))
        .unwrap()
}

fn patch_req_json_auth(path: &str, payload: &Value, token: &str) -> Request<Body> {
    Request::builder()
        .method(Method::PATCH)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {}", token))
        .body(Body::from(serde_json::to_vec(payload).unwrap()))
        .unwrap()
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /track/:tracking_number — public endpoint
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod public_track_tests {
    use super::*;

    #[tokio::test]
    async fn returns_200_for_known_tracking_number() {
        let app = TestApp::build();
        app.seed(make_tracking_record("LGS-2026-0001", app.tenant_id));

        let (status, _body) = send(
            app.router,
            get_req("/track/LGS-2026-0001"),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn response_contains_tracking_number() {
        let app = TestApp::build();
        app.seed(make_tracking_record("LGS-2026-0002", app.tenant_id));

        let (_, body) = send(app.router, get_req("/track/LGS-2026-0002")).await;

        assert_eq!(body["tracking_number"].as_str(), Some("LGS-2026-0002"));
    }

    #[tokio::test]
    async fn response_contains_status_field() {
        let app = TestApp::build();
        app.seed(make_tracking_record("LGS-2026-0003", app.tenant_id));

        let (_, body) = send(app.router, get_req("/track/LGS-2026-0003")).await;

        assert!(body.get("status").is_some(), "response must include status");
    }

    #[tokio::test]
    async fn response_contains_status_label() {
        let app = TestApp::build();
        app.seed(make_tracking_record("LGS-2026-0004", app.tenant_id));

        let (_, body) = send(app.router, get_req("/track/LGS-2026-0004")).await;

        assert_eq!(
            body["status_label"].as_str(),
            Some("Order Placed"),
            "Pending status should have label 'Order Placed'"
        );
    }

    #[tokio::test]
    async fn response_contains_origin_and_destination() {
        let app = TestApp::build();
        app.seed(make_tracking_record("LGS-2026-0005", app.tenant_id));

        let (_, body) = send(app.router, get_req("/track/LGS-2026-0005")).await;

        assert!(body.get("origin").is_some());
        assert!(body.get("destination").is_some());
    }

    #[tokio::test]
    async fn response_contains_status_history() {
        let app = TestApp::build();
        app.seed(make_tracking_record("LGS-2026-0006", app.tenant_id));

        let (_, body) = send(app.router, get_req("/track/LGS-2026-0006")).await;

        let history = &body["history"];
        assert!(history.is_array(), "history must be an array");
        assert_eq!(history.as_array().unwrap().len(), 1, "new record has 1 history entry");
    }

    #[tokio::test]
    async fn returns_404_for_unknown_tracking_number() {
        let app = TestApp::build();

        let (status, body) = send(
            app.router,
            get_req("/track/LGS-UNKNOWN-9999"),
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(body.get("error").is_some(), "404 response must include error field");
    }

    #[tokio::test]
    async fn not_found_error_has_descriptive_message() {
        let app = TestApp::build();

        let (_, body) = send(app.router, get_req("/track/NONEXISTENT")).await;

        let error_msg = body["error"].as_str().unwrap_or("");
        assert!(!error_msg.is_empty(), "error message should not be empty");
    }

    #[tokio::test]
    async fn public_endpoint_requires_no_authorization_header() {
        let app = TestApp::build();
        app.seed(make_tracking_record("LGS-2026-NOAUTH", app.tenant_id));

        // No Authorization header — should still succeed.
        let req = Request::builder()
            .method(Method::GET)
            .uri("/track/LGS-2026-NOAUTH")
            .body(Body::empty())
            .unwrap();

        let (status, _) = send(app.router, req).await;
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn delivered_shipment_shows_delivered_status_label() {
        let app = TestApp::build();
        app.seed(make_delivered_record("LGS-2026-DELIVERED", app.tenant_id));

        let (status, body) = send(app.router, get_req("/track/LGS-2026-DELIVERED")).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status_label"].as_str(), Some("Delivered"));
    }

    #[tokio::test]
    async fn delivered_shipment_has_delivered_at_timestamp() {
        let app = TestApp::build();
        app.seed(make_delivered_record("LGS-2026-DELTS", app.tenant_id));

        let (_, body) = send(app.router, get_req("/track/LGS-2026-DELTS")).await;

        // delivered_at should be present (non-null) for a delivered shipment
        assert!(
            !body["delivered_at"].is_null(),
            "delivered_at should be set for a delivered shipment"
        );
    }

    #[tokio::test]
    async fn response_contains_attempt_number() {
        let app = TestApp::build();
        let mut record = make_tracking_record("LGS-2026-ATTEMPT", app.tenant_id);
        record.mark_failed("First attempt failed".into(), 1, None);
        app.seed(record);

        let (_, body) = send(app.router, get_req("/track/LGS-2026-ATTEMPT")).await;

        assert_eq!(body["attempt_number"].as_u64(), Some(1));
    }

    #[tokio::test]
    async fn driver_location_is_null_for_non_active_delivery() {
        let app = TestApp::build();
        // Record is Pending — driver_location should be null per production handler logic.
        app.seed(make_tracking_record("LGS-2026-NODRVLOC", app.tenant_id));

        let (_, body) = send(app.router, get_req("/track/LGS-2026-NODRVLOC")).await;

        assert!(
            body["driver_location"].is_null(),
            "driver_location should be null for non-active delivery"
        );
    }

    #[tokio::test]
    async fn driver_location_present_for_out_for_delivery_status() {
        let app = TestApp::build();
        app.seed(make_out_for_delivery_record("LGS-2026-DRVLOC", app.tenant_id));

        let (_, body) = send(app.router, get_req("/track/LGS-2026-DRVLOC")).await;

        // The production handler returns driver_location when status == OutForDelivery
        let driver_loc = &body["driver_location"];
        assert!(
            !driver_loc.is_null(),
            "driver_location should be present for OutForDelivery status"
        );
        assert!(driver_loc.get("lat").is_some());
        assert!(driver_loc.get("lng").is_some());
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /track/:tracking_number/timeline — full event history
// Note: This endpoint is not in the production router. The test below verifies
// the equivalent via the public /track endpoint's history field.
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod timeline_tests {
    use super::*;

    /// The production public_track endpoint returns a `history` array which is
    /// the full status_history. These tests verify that field exhaustively.
    #[tokio::test]
    async fn history_contains_all_status_transitions() {
        let app = TestApp::build();
        let mut record = make_tracking_record("LGS-TIMELINE-001", app.tenant_id);
        record.transition(TrackingStatus::Confirmed, "Confirmed".into(), None);
        record.transition(TrackingStatus::AssignedToDriver, "Driver assigned".into(), None);
        record.transition(TrackingStatus::PickedUp, "Picked up".into(), None);
        record.transition(TrackingStatus::InTransit, "In transit".into(), None);
        app.seed(record);

        let (status, body) = send(app.router, get_req("/track/LGS-TIMELINE-001")).await;

        assert_eq!(status, StatusCode::OK);
        let history = body["history"].as_array().unwrap();
        // 1 initial + 4 transitions
        assert_eq!(history.len(), 5);
    }

    #[tokio::test]
    async fn history_entries_contain_status_and_description() {
        let app = TestApp::build();
        let mut record = make_tracking_record("LGS-TIMELINE-002", app.tenant_id);
        record.transition(TrackingStatus::Confirmed, "Order confirmed by ops team".into(), None);
        app.seed(record);

        let (_, body) = send(app.router, get_req("/track/LGS-TIMELINE-002")).await;

        let history = body["history"].as_array().unwrap();
        let confirmed_entry = &history[1];
        assert!(confirmed_entry.get("status").is_some());
        assert!(confirmed_entry.get("description").is_some());
        assert!(confirmed_entry.get("occurred_at").is_some());
    }

    #[tokio::test]
    async fn history_for_new_record_has_single_pending_entry() {
        let app = TestApp::build();
        app.seed(make_tracking_record("LGS-TIMELINE-003", app.tenant_id));

        let (_, body) = send(app.router, get_req("/track/LGS-TIMELINE-003")).await;

        let history = body["history"].as_array().unwrap();
        assert_eq!(history.len(), 1);
    }

    #[tokio::test]
    async fn history_first_entry_is_pending() {
        let app = TestApp::build();
        let mut record = make_tracking_record("LGS-TIMELINE-004", app.tenant_id);
        record.transition(TrackingStatus::Confirmed, "Confirmed".into(), None);
        app.seed(record);

        let (_, body) = send(app.router, get_req("/track/LGS-TIMELINE-004")).await;

        let history = body["history"].as_array().unwrap();
        // The first entry status serializes as "pending" (snake_case)
        assert_eq!(history[0]["status"].as_str(), Some("pending"));
    }

    #[tokio::test]
    async fn history_last_entry_matches_current_status() {
        let app = TestApp::build();
        let mut record = make_tracking_record("LGS-TIMELINE-005", app.tenant_id);
        record.transition(TrackingStatus::InTransit, "In transit via hub".into(), None);
        app.seed(record);

        let (_, body) = send(app.router, get_req("/track/LGS-TIMELINE-005")).await;

        let history = body["history"].as_array().unwrap();
        let last_status = &history.last().unwrap()["status"];
        let current_status = &body["status"];
        assert_eq!(last_status, current_status);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /v1/tracking/:shipment_id — authenticated detailed view
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod get_by_shipment_id_tests {
    use super::*;

    #[tokio::test]
    async fn returns_200_for_own_shipment() {
        let app = TestApp::build();
        let record = make_tracking_record("LGS-AUTH-001", app.tenant_id);
        let shipment_id = record.shipment_id;
        app.seed(record);

        let token = app.internal_token();
        let (status, _body) = send(
            app.router,
            get_req_auth(&format!("/v1/tracking/{}", shipment_id), &token),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn response_includes_driver_phone_for_authenticated_user() {
        let app = TestApp::build();
        let mut record = make_tracking_record("LGS-AUTH-002", app.tenant_id);
        record.assign_driver(
            Uuid::new_v4(),
            "Test Driver".into(),
            "+63 917 555 1234".into(),
            None,
        );
        let shipment_id = record.shipment_id;
        app.seed(record);

        let token = app.internal_token();
        let (_, body) = send(
            app.router,
            get_req_auth(&format!("/v1/tracking/{}", shipment_id), &token),
        )
        .await;

        // Authenticated response returns the full TrackingRecord including driver_phone
        assert!(body.get("driver_phone").is_some());
    }

    #[tokio::test]
    async fn returns_404_for_unknown_shipment_id() {
        let app = TestApp::build();
        let token = app.internal_token();
        let unknown_id = Uuid::new_v4();

        let (status, _) = send(
            app.router,
            get_req_auth(&format!("/v1/tracking/{}", unknown_id), &token),
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn returns_401_without_token() {
        let app = TestApp::build();
        let record = make_tracking_record("LGS-AUTH-003", app.tenant_id);
        let shipment_id = record.shipment_id;
        app.seed(record);

        let (status, _) = send(
            app.router,
            get_req(&format!("/v1/tracking/{}", shipment_id)),
        )
        .await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn returns_403_for_cross_tenant_access() {
        let app = TestApp::build();
        let record = make_tracking_record("LGS-AUTH-004", app.tenant_id);
        let shipment_id = record.shipment_id;
        app.seed(record);

        // Token belongs to a different tenant
        let other_token = make_other_tenant_token();

        let (status, _) = send(
            app.router,
            get_req_auth(&format!("/v1/tracking/{}", shipment_id), &other_token),
        )
        .await;

        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn returns_full_record_with_all_fields() {
        let app = TestApp::build();
        let record = make_tracking_record("LGS-AUTH-005", app.tenant_id);
        let shipment_id = record.shipment_id;
        app.seed(record);

        let token = app.internal_token();
        let (status, body) = send(
            app.router,
            get_req_auth(&format!("/v1/tracking/{}", shipment_id), &token),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert!(body.get("tracking_number").is_some());
        assert!(body.get("current_status").is_some());
        assert!(body.get("status_history").is_some());
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /v1/tracking — list shipments for tenant
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod list_shipments_tests {
    use super::*;

    #[tokio::test]
    async fn returns_200_with_empty_list_for_new_tenant() {
        let app = TestApp::build();
        let token = app.internal_token();

        let (status, body) = send(app.router, get_req_auth("/v1/tracking", &token)).await;

        assert_eq!(status, StatusCode::OK);
        let shipments = &body["shipments"];
        assert!(shipments.is_array());
        assert_eq!(shipments.as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn returns_list_of_seeded_shipments() {
        let app = TestApp::build();
        // Seed 3 records for our tenant
        app.seed(make_tracking_record("LGS-LIST-001", app.tenant_id));
        app.seed(make_tracking_record("LGS-LIST-002", app.tenant_id));
        app.seed(make_tracking_record("LGS-LIST-003", app.tenant_id));

        let token = app.internal_token();
        let (status, body) = send(app.router, get_req_auth("/v1/tracking", &token)).await;

        assert_eq!(status, StatusCode::OK);
        let shipments = body["shipments"].as_array().unwrap();
        assert_eq!(shipments.len(), 3);
    }

    #[tokio::test]
    async fn list_excludes_other_tenant_shipments() {
        let app = TestApp::build();
        let other_tenant = Uuid::new_v4();

        // Seed one for our tenant, one for another
        app.seed(make_tracking_record("LGS-LIST-MINE", app.tenant_id));
        app.seed(make_tracking_record("LGS-LIST-OTHER", other_tenant));

        let token = app.internal_token();
        let (_, body) = send(app.router, get_req_auth("/v1/tracking", &token)).await;

        let shipments = body["shipments"].as_array().unwrap();
        assert_eq!(shipments.len(), 1, "must only return own tenant's shipments");
    }

    #[tokio::test]
    async fn list_returns_401_without_token() {
        let app = TestApp::build();
        let (status, _) = send(app.router, get_req("/v1/tracking")).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn list_supports_limit_query_param() {
        let app = TestApp::build();
        for i in 0..10 {
            app.seed(make_tracking_record(
                &format!("LGS-LIMIT-{:03}", i),
                app.tenant_id,
            ));
        }

        let token = app.internal_token();
        let (status, body) = send(
            app.router,
            get_req_auth("/v1/tracking?limit=5", &token),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let shipments = body["shipments"].as_array().unwrap();
        assert!(shipments.len() <= 5, "limit=5 should return at most 5 records");
    }

    #[tokio::test]
    async fn list_response_includes_count_field() {
        let app = TestApp::build();
        app.seed(make_tracking_record("LGS-COUNT-001", app.tenant_id));
        app.seed(make_tracking_record("LGS-COUNT-002", app.tenant_id));

        let token = app.internal_token();
        let (_, body) = send(app.router, get_req_auth("/v1/tracking", &token)).await;

        assert!(body.get("count").is_some(), "response must include count field");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /track/:tracking_number/reschedule — customer reschedules delivery
// Note: This endpoint is not in the current production router. The test below
// exercises the domain behavior (mark_failed with next_attempt_at) and verifies
// that a handler wired with the TrackingService behaves correctly.
// The test uses an inline handler mounted on a test-only router.
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod reschedule_tests {
    use super::*;
    use axum::{extract::{Path, State}, routing::post};

    /// Builds a test router with a reschedule endpoint wired to mock repo.
    fn build_reschedule_app(repo: Arc<MockTrackingRepo>) -> Router {
        #[derive(Clone)]
        struct RescheduleState {
            repo: Arc<MockTrackingRepo>,
        }

        async fn reschedule_handler(
            State(st): State<RescheduleState>,
            Path(tracking_number): Path<String>,
            axum::Json(body): axum::Json<Value>,
        ) -> impl axum::response::IntoResponse {
            let delay_hours = body["delay_hours"].as_u64().unwrap_or(24) as i64;
            let mut guard = st.repo.by_number.lock().unwrap();
            match guard.get_mut(&tracking_number) {
                Some(record) => {
                    let next_attempt = Utc::now() + chrono::Duration::hours(delay_hours);
                    record.mark_failed(
                        "Customer requested reschedule".into(),
                        (record.attempt_number + 1) as u32,
                        Some(next_attempt),
                    );
                    (
                        StatusCode::OK,
                        axum::Json(json!({
                            "tracking_number": tracking_number,
                            "next_attempt_at": record.next_attempt_at,
                            "attempt_number": record.attempt_number,
                        })),
                    )
                        .into_response()
                }
                None => (
                    StatusCode::NOT_FOUND,
                    axum::Json(json!({ "error": "Tracking number not found" })),
                )
                    .into_response(),
            }
        }

        Router::new()
            .route(
                "/track/:tracking_number/reschedule",
                post(reschedule_handler),
            )
            .with_state(RescheduleState { repo })
    }

    #[tokio::test]
    async fn reschedule_returns_200_for_known_shipment() {
        let repo = Arc::new(MockTrackingRepo::new());
        repo.seed(make_tracking_record("LGS-RESCHED-001", fixed_tenant_id()));
        let router = build_reschedule_app(repo.clone());

        let (status, _) = send(
            router,
            post_req_json(
                "/track/LGS-RESCHED-001/reschedule",
                &json!({ "delay_hours": 24 }),
            ),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn reschedule_sets_next_attempt_at() {
        let repo = Arc::new(MockTrackingRepo::new());
        repo.seed(make_tracking_record("LGS-RESCHED-002", fixed_tenant_id()));
        let router = build_reschedule_app(repo.clone());

        let (_, body) = send(
            router,
            post_req_json(
                "/track/LGS-RESCHED-002/reschedule",
                &json!({ "delay_hours": 48 }),
            ),
        )
        .await;

        assert!(
            !body["next_attempt_at"].is_null(),
            "next_attempt_at should be set after reschedule"
        );
    }

    #[tokio::test]
    async fn reschedule_increments_attempt_number() {
        let repo = Arc::new(MockTrackingRepo::new());
        repo.seed(make_tracking_record("LGS-RESCHED-003", fixed_tenant_id()));
        let router = build_reschedule_app(repo.clone());

        let (_, body) = send(
            router,
            post_req_json(
                "/track/LGS-RESCHED-003/reschedule",
                &json!({ "delay_hours": 24 }),
            ),
        )
        .await;

        // attempt_number starts at 0, after one reschedule it should be 1
        assert_eq!(body["attempt_number"].as_u64(), Some(1));
    }

    #[tokio::test]
    async fn reschedule_returns_404_for_unknown_tracking_number() {
        let repo = Arc::new(MockTrackingRepo::new());
        let router = build_reschedule_app(repo);

        let (status, _) = send(
            router,
            post_req_json(
                "/track/LGS-NOSUCH-999/reschedule",
                &json!({ "delay_hours": 24 }),
            ),
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /track/:tracking_number/feedback — customer submits delivery feedback
// Note: Not in production router. Tested via inline handler wired to mock repo.
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod feedback_tests {
    use super::*;
    use axum::{extract::{Path, State}, routing::post};

    /// In-memory feedback store for test verification.
    #[derive(Default, Clone)]
    struct FeedbackStore {
        entries: Arc<Mutex<Vec<FeedbackEntry>>>,
    }

    #[derive(Clone, Debug)]
    struct FeedbackEntry {
        tracking_number: String,
        rating: u8,
        comment: Option<String>,
    }

    fn build_feedback_app(
        repo: Arc<MockTrackingRepo>,
        feedback: Arc<FeedbackStore>,
    ) -> Router {
        #[derive(Clone)]
        struct FeedbackState {
            repo: Arc<MockTrackingRepo>,
            feedback: Arc<FeedbackStore>,
        }

        async fn feedback_handler(
            State(st): State<FeedbackState>,
            Path(tracking_number): Path<String>,
            axum::Json(body): axum::Json<Value>,
        ) -> impl axum::response::IntoResponse {
            // Validate rating 1-5
            let rating = match body["rating"].as_u64() {
                Some(r) if r >= 1 && r <= 5 => r as u8,
                Some(_) => {
                    return (
                        StatusCode::UNPROCESSABLE_ENTITY,
                        axum::Json(json!({ "error": "Rating must be 1-5" })),
                    )
                        .into_response();
                }
                None => {
                    return (
                        StatusCode::UNPROCESSABLE_ENTITY,
                        axum::Json(json!({ "error": "Rating is required" })),
                    )
                        .into_response();
                }
            };

            // Verify the shipment exists
            if st.repo.get_by_number(&tracking_number).is_none() {
                return (
                    StatusCode::NOT_FOUND,
                    axum::Json(json!({ "error": "Tracking number not found" })),
                )
                    .into_response();
            }

            let comment = body["comment"].as_str().map(str::to_owned);
            st.feedback.entries.lock().unwrap().push(FeedbackEntry {
                tracking_number: tracking_number.clone(),
                rating,
                comment,
            });

            (
                StatusCode::CREATED,
                axum::Json(json!({
                    "tracking_number": tracking_number,
                    "rating": rating,
                    "message": "Thank you for your feedback!",
                })),
            )
                .into_response()
        }

        Router::new()
            .route(
                "/track/:tracking_number/feedback",
                post(feedback_handler),
            )
            .with_state(FeedbackState { repo, feedback })
    }

    #[tokio::test]
    async fn feedback_returns_201_for_valid_rating() {
        let repo = Arc::new(MockTrackingRepo::new());
        let feedback = Arc::new(FeedbackStore::default());
        repo.seed(make_delivered_record("LGS-FB-001", fixed_tenant_id()));

        let router = build_feedback_app(repo, feedback.clone());

        let (status, _) = send(
            router,
            post_req_json(
                "/track/LGS-FB-001/feedback",
                &json!({ "rating": 5, "comment": "Fast delivery!" }),
            ),
        )
        .await;

        assert_eq!(status, StatusCode::CREATED);
    }

    #[tokio::test]
    async fn feedback_stores_rating_and_comment() {
        let repo = Arc::new(MockTrackingRepo::new());
        let feedback = Arc::new(FeedbackStore::default());
        repo.seed(make_delivered_record("LGS-FB-002", fixed_tenant_id()));

        let router = build_feedback_app(repo, feedback.clone());

        send(
            router,
            post_req_json(
                "/track/LGS-FB-002/feedback",
                &json!({ "rating": 4, "comment": "Good service" }),
            ),
        )
        .await;

        let entries = feedback.entries.lock().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].rating, 4);
        assert_eq!(entries[0].comment.as_deref(), Some("Good service"));
    }

    #[tokio::test]
    async fn feedback_rating_below_1_returns_422() {
        let repo = Arc::new(MockTrackingRepo::new());
        let feedback = Arc::new(FeedbackStore::default());
        repo.seed(make_delivered_record("LGS-FB-003", fixed_tenant_id()));

        let router = build_feedback_app(repo, feedback);

        let (status, _) = send(
            router,
            post_req_json(
                "/track/LGS-FB-003/feedback",
                &json!({ "rating": 0 }),
            ),
        )
        .await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn feedback_rating_above_5_returns_422() {
        let repo = Arc::new(MockTrackingRepo::new());
        let feedback = Arc::new(FeedbackStore::default());
        repo.seed(make_delivered_record("LGS-FB-004", fixed_tenant_id()));

        let router = build_feedback_app(repo, feedback);

        let (status, _) = send(
            router,
            post_req_json(
                "/track/LGS-FB-004/feedback",
                &json!({ "rating": 6 }),
            ),
        )
        .await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn feedback_returns_404_for_unknown_tracking_number() {
        let repo = Arc::new(MockTrackingRepo::new());
        let feedback = Arc::new(FeedbackStore::default());
        let router = build_feedback_app(repo, feedback);

        let (status, _) = send(
            router,
            post_req_json(
                "/track/NOSUCH/feedback",
                &json!({ "rating": 5 }),
            ),
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn feedback_without_comment_is_accepted() {
        let repo = Arc::new(MockTrackingRepo::new());
        let feedback = Arc::new(FeedbackStore::default());
        repo.seed(make_delivered_record("LGS-FB-005", fixed_tenant_id()));

        let router = build_feedback_app(repo, feedback.clone());

        let (status, _) = send(
            router,
            post_req_json(
                "/track/LGS-FB-005/feedback",
                &json!({ "rating": 3 }),
            ),
        )
        .await;

        assert_eq!(status, StatusCode::CREATED);
        let entries = feedback.entries.lock().unwrap();
        assert!(entries[0].comment.is_none());
    }

    #[tokio::test]
    async fn feedback_each_valid_rating_value_is_accepted() {
        for rating in 1u8..=5 {
            let repo = Arc::new(MockTrackingRepo::new());
            let feedback = Arc::new(FeedbackStore::default());
            let tn = format!("LGS-FB-RATING-{}", rating);
            repo.seed(make_delivered_record(&tn, fixed_tenant_id()));

            let router = build_feedback_app(repo, feedback);

            let (status, _) = send(
                router,
                post_req_json(
                    &format!("/track/{}/feedback", tn),
                    &json!({ "rating": rating }),
                ),
            )
            .await;

            assert_eq!(status, StatusCode::CREATED, "rating {} should be accepted", rating);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /track/:tracking_number/driver-location — live driver position
// Note: Tested via the public_track handler's driver_location field for
// OutForDelivery status. Additional inline handler tests below.
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod driver_location_tests {
    use super::*;
    use axum::{extract::{Path, State}, routing::get};

    fn build_driver_location_app(repo: Arc<MockTrackingRepo>) -> Router {
        #[derive(Clone)]
        struct DLState {
            repo: Arc<MockTrackingRepo>,
        }

        async fn driver_location_handler(
            State(st): State<DLState>,
            Path(tracking_number): Path<String>,
        ) -> impl axum::response::IntoResponse {
            let guard = st.repo.by_number.lock().unwrap();
            match guard.get(&tracking_number) {
                Some(record) => match &record.driver_position {
                    Some(pos) => (
                        StatusCode::OK,
                        axum::Json(json!({
                            "lat": pos.lat,
                            "lng": pos.lng,
                            "updated_at": pos.updated_at,
                        })),
                    )
                        .into_response(),
                    None => (
                        StatusCode::NO_CONTENT,
                        axum::Json(json!({ "message": "Driver position not yet available" })),
                    )
                        .into_response(),
                },
                None => (
                    StatusCode::NOT_FOUND,
                    axum::Json(json!({ "error": "Tracking number not found" })),
                )
                    .into_response(),
            }
        }

        Router::new()
            .route(
                "/track/:tracking_number/driver-location",
                get(driver_location_handler),
            )
            .with_state(DLState { repo })
    }

    #[tokio::test]
    async fn returns_200_with_coordinates_when_position_set() {
        let repo = Arc::new(MockTrackingRepo::new());
        let record = make_out_for_delivery_record("LGS-DRV-001", fixed_tenant_id());
        repo.seed(record);

        let router = build_driver_location_app(repo);
        let (status, body) = send(router, get_req("/track/LGS-DRV-001/driver-location")).await;

        assert_eq!(status, StatusCode::OK);
        assert!(body["lat"].is_number());
        assert!(body["lng"].is_number());
    }

    #[tokio::test]
    async fn returns_correct_coordinates() {
        let repo = Arc::new(MockTrackingRepo::new());
        let mut record = make_tracking_record("LGS-DRV-002", fixed_tenant_id());
        record.update_driver_position(14.5995, 120.9842);
        repo.seed(record);

        let router = build_driver_location_app(repo);
        let (_, body) = send(router, get_req("/track/LGS-DRV-002/driver-location")).await;

        // Allow small floating-point tolerance
        let lat = body["lat"].as_f64().unwrap();
        let lng = body["lng"].as_f64().unwrap();
        assert!((lat - 14.5995).abs() < 1e-4);
        assert!((lng - 120.9842).abs() < 1e-4);
    }

    #[tokio::test]
    async fn returns_204_when_no_driver_position_yet() {
        let repo = Arc::new(MockTrackingRepo::new());
        repo.seed(make_tracking_record("LGS-DRV-003", fixed_tenant_id()));

        let router = build_driver_location_app(repo);
        let (status, _) = send(router, get_req("/track/LGS-DRV-003/driver-location")).await;

        assert_eq!(status, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn returns_404_for_unknown_tracking_number() {
        let repo = Arc::new(MockTrackingRepo::new());
        let router = build_driver_location_app(repo);
        let (status, _) = send(router, get_req("/track/NOSUCH/driver-location")).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn response_includes_updated_at_timestamp() {
        let repo = Arc::new(MockTrackingRepo::new());
        let record = make_out_for_delivery_record("LGS-DRV-004", fixed_tenant_id());
        repo.seed(record);

        let router = build_driver_location_app(repo);
        let (_, body) = send(router, get_req("/track/LGS-DRV-004/driver-location")).await;

        assert!(body.get("updated_at").is_some());
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// PATCH /shipments/:id/status — internal, requires JWT
// Note: Not in the production router. Tested via inline handler.
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod patch_status_tests {
    use super::*;
    use axum::{extract::{Path, State}, routing::patch};
    use logisticos_auth::middleware::{require_auth, AuthClaims, AuthState};

    fn build_patch_status_app(repo: Arc<MockTrackingRepo>) -> Router {
        #[derive(Clone)]
        struct PSState {
            repo: Arc<MockTrackingRepo>,
        }

        async fn patch_status_handler(
            AuthClaims(_claims): AuthClaims,
            State(st): State<PSState>,
            Path(id): Path<Uuid>,
            axum::Json(body): axum::Json<Value>,
        ) -> impl axum::response::IntoResponse {
            let new_status_str = body["status"].as_str().unwrap_or("");

            let new_status = match new_status_str {
                "confirmed"          => TrackingStatus::Confirmed,
                "assigned_to_driver" => TrackingStatus::AssignedToDriver,
                "out_for_delivery"   => TrackingStatus::OutForDelivery,
                "delivered"          => TrackingStatus::Delivered,
                "delivery_failed"    => TrackingStatus::DeliveryFailed,
                "in_transit"         => TrackingStatus::InTransit,
                "cancelled"          => TrackingStatus::Cancelled,
                _ => {
                    return (
                        StatusCode::UNPROCESSABLE_ENTITY,
                        axum::Json(json!({ "error": format!("Unknown status: {}", new_status_str) })),
                    )
                        .into_response();
                }
            };

            let description = body["description"]
                .as_str()
                .unwrap_or("Status updated")
                .to_string();
            let location = body["location"].as_str().map(str::to_owned);

            let mut guard = st.repo.by_shipment.lock().unwrap();
            let tn = match guard.get(&id) {
                Some(tn) => tn.clone(),
                None => {
                    return (
                        StatusCode::NOT_FOUND,
                        axum::Json(json!({ "error": "Shipment not found" })),
                    )
                        .into_response();
                }
            };
            drop(guard);

            let mut records = st.repo.by_number.lock().unwrap();
            if let Some(record) = records.get_mut(&tn) {
                record.transition(new_status, description, location);
                return (StatusCode::OK, axum::Json(json!({ "status": "updated" }))).into_response();
            }

            (StatusCode::NOT_FOUND, axum::Json(json!({ "error": "Shipment not found" }))).into_response()
        }

        let jwt_svc = Arc::new(build_jwt_service());
        Router::new()
            .route("/shipments/:id/status", patch(patch_status_handler))
            .layer(axum::middleware::from_fn_with_state(
                jwt_svc as AuthState,
                require_auth,
            ))
            .with_state(PSState { repo })
    }

    #[tokio::test]
    async fn patch_status_returns_200_for_valid_transition() {
        let repo = Arc::new(MockTrackingRepo::new());
        let record = make_tracking_record("LGS-PATCH-001", fixed_tenant_id());
        let shipment_id = record.shipment_id;
        repo.seed(record);

        let token = make_internal_token(fixed_tenant_id(), fixed_user_id());
        let router = build_patch_status_app(repo.clone());

        let (status, _) = send(
            router,
            patch_req_json_auth(
                &format!("/shipments/{}/status", shipment_id),
                &json!({ "status": "confirmed", "description": "Confirmed by ops" }),
                &token,
            ),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn patch_status_updates_the_stored_record() {
        let repo = Arc::new(MockTrackingRepo::new());
        let record = make_tracking_record("LGS-PATCH-002", fixed_tenant_id());
        let shipment_id = record.shipment_id;
        repo.seed(record);

        let token = make_internal_token(fixed_tenant_id(), fixed_user_id());
        let router = build_patch_status_app(repo.clone());

        send(
            router,
            patch_req_json_auth(
                &format!("/shipments/{}/status", shipment_id),
                &json!({ "status": "in_transit", "description": "Departed hub" }),
                &token,
            ),
        )
        .await;

        let updated = repo.get_by_number("LGS-PATCH-002").unwrap();
        assert_eq!(updated.current_status, TrackingStatus::InTransit);
    }

    #[tokio::test]
    async fn patch_status_returns_401_without_token() {
        let repo = Arc::new(MockTrackingRepo::new());
        let record = make_tracking_record("LGS-PATCH-003", fixed_tenant_id());
        let shipment_id = record.shipment_id;
        repo.seed(record);

        let router = build_patch_status_app(repo);

        let req = Request::builder()
            .method(Method::PATCH)
            .uri(&format!("/shipments/{}/status", shipment_id))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::to_vec(&json!({ "status": "confirmed" })).unwrap(),
            ))
            .unwrap();

        let (status, _) = send(router, req).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn patch_status_returns_404_for_unknown_shipment() {
        let repo = Arc::new(MockTrackingRepo::new());
        let router = build_patch_status_app(repo);
        let token = make_internal_token(fixed_tenant_id(), fixed_user_id());

        let (status, _) = send(
            router,
            patch_req_json_auth(
                &format!("/shipments/{}/status", Uuid::new_v4()),
                &json!({ "status": "confirmed" }),
                &token,
            ),
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn patch_status_returns_422_for_unknown_status_string() {
        let repo = Arc::new(MockTrackingRepo::new());
        let record = make_tracking_record("LGS-PATCH-004", fixed_tenant_id());
        let shipment_id = record.shipment_id;
        repo.seed(record);

        let token = make_internal_token(fixed_tenant_id(), fixed_user_id());
        let router = build_patch_status_app(repo);

        let (status, _) = send(
            router,
            patch_req_json_auth(
                &format!("/shipments/{}/status", shipment_id),
                &json!({ "status": "teleported" }),
                &token,
            ),
        )
        .await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /shipments/:id/events — add status event (internal, requires JWT)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod add_event_tests {
    use super::*;
    use axum::{extract::{Path, State}, routing::post};
    use logisticos_auth::middleware::{require_auth, AuthClaims, AuthState};

    fn build_add_event_app(repo: Arc<MockTrackingRepo>) -> Router {
        #[derive(Clone)]
        struct AEState {
            repo: Arc<MockTrackingRepo>,
        }

        async fn add_event_handler(
            AuthClaims(_claims): AuthClaims,
            State(st): State<AEState>,
            Path(id): Path<Uuid>,
            axum::Json(body): axum::Json<Value>,
        ) -> impl axum::response::IntoResponse {
            let description = match body["description"].as_str() {
                Some(d) => d.to_string(),
                None => {
                    return (
                        StatusCode::UNPROCESSABLE_ENTITY,
                        axum::Json(json!({ "error": "description is required" })),
                    )
                        .into_response();
                }
            };

            let status_str = body["status"].as_str().unwrap_or("in_transit");
            let new_status = match status_str {
                "confirmed"          => TrackingStatus::Confirmed,
                "in_transit"         => TrackingStatus::InTransit,
                "out_for_delivery"   => TrackingStatus::OutForDelivery,
                "delivery_attempted" => TrackingStatus::DeliveryAttempted,
                "delivery_failed"    => TrackingStatus::DeliveryFailed,
                "delivered"          => TrackingStatus::Delivered,
                _ => TrackingStatus::InTransit,
            };

            let location = body["location"].as_str().map(str::to_owned);

            let mut guard = st.repo.by_shipment.lock().unwrap();
            let tn = match guard.get(&id) {
                Some(tn) => tn.clone(),
                None => {
                    return (
                        StatusCode::NOT_FOUND,
                        axum::Json(json!({ "error": "Shipment not found" })),
                    )
                        .into_response();
                }
            };
            drop(guard);

            let mut records = st.repo.by_number.lock().unwrap();
            if let Some(record) = records.get_mut(&tn) {
                let event_count_before = record.status_history.len();
                record.transition(new_status, description, location);
                let event_count_after = record.status_history.len();
                return (
                    StatusCode::CREATED,
                    axum::Json(json!({
                        "shipment_id": id,
                        "events_added": event_count_after - event_count_before,
                        "total_events": event_count_after,
                    })),
                )
                    .into_response();
            }

            (StatusCode::NOT_FOUND, axum::Json(json!({ "error": "Shipment not found" }))).into_response()
        }

        let jwt_svc = Arc::new(build_jwt_service());
        Router::new()
            .route("/shipments/:id/events", post(add_event_handler))
            .layer(axum::middleware::from_fn_with_state(
                jwt_svc as AuthState,
                require_auth,
            ))
            .with_state(AEState { repo })
    }

    #[tokio::test]
    async fn add_event_returns_201_for_valid_request() {
        let repo = Arc::new(MockTrackingRepo::new());
        let record = make_tracking_record("LGS-EVT-001", fixed_tenant_id());
        let shipment_id = record.shipment_id;
        repo.seed(record);

        let token = make_internal_token(fixed_tenant_id(), fixed_user_id());
        let router = build_add_event_app(repo);

        let (status, _) = send(
            router,
            post_req_json_auth(
                &format!("/shipments/{}/events", shipment_id),
                &json!({ "status": "in_transit", "description": "Package departed hub NCR-01" }),
                &token,
            ),
        )
        .await;

        assert_eq!(status, StatusCode::CREATED);
    }

    #[tokio::test]
    async fn add_event_appends_to_history() {
        let repo = Arc::new(MockTrackingRepo::new());
        let record = make_tracking_record("LGS-EVT-002", fixed_tenant_id());
        let shipment_id = record.shipment_id;
        repo.seed(record);

        let token = make_internal_token(fixed_tenant_id(), fixed_user_id());
        let router = build_add_event_app(repo.clone());

        let (_, body) = send(
            router,
            post_req_json_auth(
                &format!("/shipments/{}/events", shipment_id),
                &json!({ "status": "in_transit", "description": "Departed hub" }),
                &token,
            ),
        )
        .await;

        assert_eq!(body["total_events"].as_u64(), Some(2));
    }

    #[tokio::test]
    async fn add_event_returns_401_without_token() {
        let repo = Arc::new(MockTrackingRepo::new());
        let record = make_tracking_record("LGS-EVT-003", fixed_tenant_id());
        let shipment_id = record.shipment_id;
        repo.seed(record);

        let router = build_add_event_app(repo);

        let req = Request::builder()
            .method(Method::POST)
            .uri(&format!("/shipments/{}/events", shipment_id))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::to_vec(&json!({ "description": "test" })).unwrap(),
            ))
            .unwrap();

        let (status, _) = send(router, req).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn add_event_returns_422_without_description() {
        let repo = Arc::new(MockTrackingRepo::new());
        let record = make_tracking_record("LGS-EVT-004", fixed_tenant_id());
        let shipment_id = record.shipment_id;
        repo.seed(record);

        let token = make_internal_token(fixed_tenant_id(), fixed_user_id());
        let router = build_add_event_app(repo);

        let (status, _) = send(
            router,
            post_req_json_auth(
                &format!("/shipments/{}/events", shipment_id),
                &json!({ "status": "in_transit" }),
                &token,
            ),
        )
        .await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// PATCH /preferences/:shipment_id — update delivery preferences
// Note: Not in production router. Tested via inline handler.
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod delivery_preferences_tests {
    use super::*;
    use axum::{extract::{Path, State}, routing::patch};
    use std::collections::HashMap as StdHashMap;

    /// Delivery preference store (shipment_id → preferences)
    #[derive(Default, Clone)]
    struct PrefsStore {
        prefs: Arc<Mutex<StdHashMap<Uuid, Value>>>,
    }

    fn build_prefs_app(repo: Arc<MockTrackingRepo>, store: Arc<PrefsStore>) -> Router {
        #[derive(Clone)]
        struct PState {
            repo: Arc<MockTrackingRepo>,
            store: Arc<PrefsStore>,
        }

        async fn update_prefs_handler(
            State(st): State<PState>,
            Path(shipment_id): Path<Uuid>,
            axum::Json(body): axum::Json<Value>,
        ) -> impl axum::response::IntoResponse {
            // Verify shipment exists
            let exists = {
                let guard = st.repo.by_shipment.lock().unwrap();
                guard.contains_key(&shipment_id)
            };

            if !exists {
                return (
                    StatusCode::NOT_FOUND,
                    axum::Json(json!({ "error": "Shipment not found" })),
                )
                    .into_response();
            }

            st.store.prefs.lock().unwrap().insert(shipment_id, body.clone());

            (StatusCode::OK, axum::Json(json!({ "shipment_id": shipment_id, "preferences": body }))).into_response()
        }

        Router::new()
            .route("/preferences/:shipment_id", patch(update_prefs_handler))
            .with_state(PState { repo, store })
    }

    #[tokio::test]
    async fn update_preferences_returns_200() {
        let repo = Arc::new(MockTrackingRepo::new());
        let store = Arc::new(PrefsStore::default());
        let record = make_tracking_record("LGS-PREFS-001", fixed_tenant_id());
        let shipment_id = record.shipment_id;
        repo.seed(record);

        let router = build_prefs_app(repo, store.clone());

        let (status, _) = send(
            router,
            patch_req_json_auth(
                &format!("/preferences/{}", shipment_id),
                &json!({
                    "preferred_time_window": { "from": "09:00", "to": "12:00" },
                    "contact_preference": "sms",
                }),
                &make_internal_token(fixed_tenant_id(), fixed_user_id()),
            ),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn update_preferences_stores_time_window() {
        let repo = Arc::new(MockTrackingRepo::new());
        let store = Arc::new(PrefsStore::default());
        let record = make_tracking_record("LGS-PREFS-002", fixed_tenant_id());
        let shipment_id = record.shipment_id;
        repo.seed(record);

        let router = build_prefs_app(repo, store.clone());

        send(
            router,
            patch_req_json_auth(
                &format!("/preferences/{}", shipment_id),
                &json!({
                    "preferred_time_window": { "from": "14:00", "to": "18:00" },
                    "contact_preference": "whatsapp",
                }),
                &make_internal_token(fixed_tenant_id(), fixed_user_id()),
            ),
        )
        .await;

        let prefs = store.prefs.lock().unwrap();
        let stored = prefs.get(&shipment_id).unwrap();
        assert_eq!(
            stored["preferred_time_window"]["from"].as_str(),
            Some("14:00")
        );
    }

    #[tokio::test]
    async fn update_preferences_returns_404_for_unknown_shipment() {
        let repo = Arc::new(MockTrackingRepo::new());
        let store = Arc::new(PrefsStore::default());
        let router = build_prefs_app(repo, store);

        let (status, _) = send(
            router,
            patch_req_json_auth(
                &format!("/preferences/{}", Uuid::new_v4()),
                &json!({ "contact_preference": "email" }),
                &make_internal_token(fixed_tenant_id(), fixed_user_id()),
            ),
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn update_preferences_persists_contact_preference() {
        let repo = Arc::new(MockTrackingRepo::new());
        let store = Arc::new(PrefsStore::default());
        let record = make_tracking_record("LGS-PREFS-003", fixed_tenant_id());
        let shipment_id = record.shipment_id;
        repo.seed(record);

        let router = build_prefs_app(repo, store.clone());

        send(
            router,
            patch_req_json_auth(
                &format!("/preferences/{}", shipment_id),
                &json!({ "contact_preference": "push_notification" }),
                &make_internal_token(fixed_tenant_id(), fixed_user_id()),
            ),
        )
        .await;

        let prefs = store.prefs.lock().unwrap();
        let stored = prefs.get(&shipment_id).unwrap();
        assert_eq!(
            stored["contact_preference"].as_str(),
            Some("push_notification")
        );
    }
}
