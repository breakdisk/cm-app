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
    response::IntoResponse,
    Router,
};
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

use async_trait::async_trait;
use chrono::Utc;

use logisticos_auth::{claims::Claims, jwt::JwtService};
use logisticos_types::TenantId;

use logisticos_delivery_experience::{
    application::services::{EventPublisher, TrackingService},
    domain::{
        entities::{TrackingRecord, TrackingStatus},
        repositories::{NewFeedback, TrackingRepository},
    },
    infrastructure::external::PodClient,
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

// ─────────────────────────────────────────────────────────────────────────────
// MockTrackingRepository — in-memory, keyed by tracking_number
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Default, Clone)]
struct MockTrackingRepo {
    /// Primary index: tracking_number → record
    by_number: Arc<Mutex<HashMap<String, TrackingRecord>>>,
    /// Secondary index: shipment_id → tracking_number (for O(1) lookup)
    by_shipment: Arc<Mutex<HashMap<Uuid, String>>>,
    /// (tracking_number, reason) per `reschedule` call.
    reschedules: Arc<Mutex<Vec<(String, String)>>>,
    feedback: Arc<Mutex<Vec<FeedbackRow>>>,
    confirmed: Arc<Mutex<Vec<String>>>,
    receipt_requests: Arc<Mutex<Vec<(String, String)>>>,
}

#[derive(Clone, Debug)]
struct FeedbackRow {
    tenant_id: Uuid,
    shipment_id: Uuid,
    driver_id: Option<Uuid>,
    tracking_number: String,
    rating: i16,
    tags: Vec<String>,
    comments: Option<String>,
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
        owner_id: Option<Uuid>,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<TrackingRecord>> {
        let guard = self.by_number.lock().unwrap();
        let records: Vec<TrackingRecord> = guard
            .values()
            .filter(|r| r.tenant_id.inner() == tenant_id.inner())
            // Same predicate as the SQL: `None` is the whole tenant, `Some` is
            // only records that owner booked.
            .filter(|r| owner_id.is_none_or(|o| r.owner_id == Some(o)))
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

    async fn reschedule(
        &self,
        tracking_number: &str,
        preferred_date: chrono::NaiveDate,
        preferred_time_slot: Option<&str>,
        reason: &str,
    ) -> anyhow::Result<()> {
        // Mirrors PgTrackingRepository::reschedule: bump the counter and move
        // next_attempt_at to 13:00 UTC for "afternoon", 09:00 UTC otherwise.
        let hour = if preferred_time_slot == Some("afternoon") { 13 } else { 9 };
        if let Some(rec) = self.by_number.lock().unwrap().get_mut(tracking_number) {
            rec.reschedule_count += 1;
            rec.next_attempt_at = preferred_date.and_hms_opt(hour, 0, 0).map(|dt| {
                chrono::DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc)
            });
        }
        self.reschedules
            .lock()
            .unwrap()
            .push((tracking_number.to_owned(), reason.to_owned()));
        Ok(())
    }

    async fn save_feedback(&self, f: &NewFeedback) -> anyhow::Result<bool> {
        let mut rows = self.feedback.lock().unwrap();
        // Mirrors the partial unique index on shipment_id.
        if rows.iter().any(|r| r.shipment_id == f.shipment_id) {
            return Ok(false);
        }
        rows.push(FeedbackRow {
            tenant_id: f.tenant_id,
            shipment_id: f.shipment_id,
            driver_id: f.driver_id,
            tracking_number: f.tracking_number.clone(),
            rating: f.rating,
            tags: f.tags.clone(),
            comments: f.comments.clone(),
        });
        Ok(true)
    }

    async fn confirm_customer_receipt(&self, tracking_number: &str) -> anyhow::Result<()> {
        self.confirmed.lock().unwrap().push(tracking_number.to_owned());
        Ok(())
    }

    async fn record_receipt_email_request(
        &self,
        tracking_number: &str,
        email: &str,
    ) -> anyhow::Result<()> {
        self.receipt_requests
            .lock()
            .unwrap()
            .push((tracking_number.to_owned(), email.to_owned()));
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

/// Records every publish; optionally fails them all.
#[derive(Default)]
struct RecordingPublisher {
    published: Mutex<Vec<(String, String, String)>>,
    fail: bool,
}

impl EventPublisher for RecordingPublisher {
    fn publish<'a>(
        &'a self,
        topic: &'a str,
        key: &'a str,
        payload: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'a>> {
        Box::pin(async move {
            if self.fail {
                return Err(anyhow::anyhow!("broker down"));
            }
            self.published
                .lock()
                .unwrap()
                .push((topic.to_owned(), key.to_owned(), payload.to_owned()));
            Ok(())
        })
    }
}

struct TestApp {
    router: Router,
    repo: Arc<MockTrackingRepo>,
    tenant_id: Uuid,
    user_id: Uuid,
}

impl TestApp {
    fn build() -> Self {
        Self::build_inner(None)
    }

    fn build_with_publisher(publisher: Arc<RecordingPublisher>) -> Self {
        Self::build_inner(Some(publisher))
    }

    fn build_inner(publisher: Option<Arc<RecordingPublisher>>) -> Self {
        let repo = Arc::new(MockTrackingRepo::new());
        let mut svc = TrackingService::new(repo.clone() as Arc<dyn TrackingRepository>);
        if let Some(p) = publisher {
            svc = svc.with_publisher(p as Arc<dyn EventPublisher>);
        }
        let tracking_svc = Arc::new(svc);
        // Nothing listens on port 1; `get_evidence` is best-effort and degrades
        // to "no evidence" on a refused connection, which is what these tests
        // want (the pod service is out of scope).
        let pod_client = Arc::new(
            PodClient::new("http://127.0.0.1:1".into()).expect("PodClient builds"),
        );
        let state = AppState { tracking_svc, pod_client };

        // Composed exactly as `bootstrap::run` does it: the public half bare,
        // `require_auth` layered over the authenticated half only.
        let jwt_svc = Arc::new(build_jwt_service());
        let auth_state = jwt_svc.clone();

        let router = logisticos_delivery_experience::api::http::public_router()
            .merge(
                logisticos_delivery_experience::api::http::authenticated_router().layer(
                    axum::middleware::from_fn_with_state(
                        auth_state,
                        logisticos_auth::middleware::require_auth,
                    ),
                ),
            )
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

        assert_eq!(body["data"]["tracking_number"].as_str(), Some("LGS-2026-0002"));
    }

    #[tokio::test]
    async fn response_contains_status_field() {
        let app = TestApp::build();
        app.seed(make_tracking_record("LGS-2026-0003", app.tenant_id));

        let (_, body) = send(app.router, get_req("/track/LGS-2026-0003")).await;

        assert!(body["data"].get("status").is_some(), "response must include status");
    }

    #[tokio::test]
    async fn response_contains_status_label() {
        let app = TestApp::build();
        app.seed(make_tracking_record("LGS-2026-0004", app.tenant_id));

        let (_, body) = send(app.router, get_req("/track/LGS-2026-0004")).await;

        assert_eq!(
            body["data"]["status_label"].as_str(),
            Some("Order Placed"),
            "Pending status should have label 'Order Placed'"
        );
    }

    #[tokio::test]
    async fn response_contains_origin_and_destination() {
        let app = TestApp::build();
        app.seed(make_tracking_record("LGS-2026-0005", app.tenant_id));

        let (_, body) = send(app.router, get_req("/track/LGS-2026-0005")).await;

        assert!(body["data"].get("origin").is_some());
        assert!(body["data"].get("destination").is_some());
    }

    #[tokio::test]
    async fn response_contains_status_history() {
        let app = TestApp::build();
        app.seed(make_tracking_record("LGS-2026-0006", app.tenant_id));

        let (_, body) = send(app.router, get_req("/track/LGS-2026-0006")).await;

        let history = &body["data"]["history"];
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
        assert_eq!(body["data"]["status_label"].as_str(), Some("Delivered"));
    }

    #[tokio::test]
    async fn delivered_shipment_has_delivered_at_timestamp() {
        let app = TestApp::build();
        app.seed(make_delivered_record("LGS-2026-DELTS", app.tenant_id));

        let (_, body) = send(app.router, get_req("/track/LGS-2026-DELTS")).await;

        // delivered_at should be present (non-null) for a delivered shipment
        assert!(
            !body["data"]["delivered_at"].is_null(),
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

        assert_eq!(body["data"]["attempt_number"].as_u64(), Some(1));
    }

    #[tokio::test]
    async fn driver_location_is_null_for_non_active_delivery() {
        let app = TestApp::build();
        // Record is Pending — driver_location should be null per production handler logic.
        app.seed(make_tracking_record("LGS-2026-NODRVLOC", app.tenant_id));

        let (_, body) = send(app.router, get_req("/track/LGS-2026-NODRVLOC")).await;

        assert!(
            body["data"]["driver_location"].is_null(),
            "driver_location should be null for non-active delivery"
        );
    }

    #[tokio::test]
    async fn driver_location_present_for_out_for_delivery_status() {
        let app = TestApp::build();
        app.seed(make_out_for_delivery_record("LGS-2026-DRVLOC", app.tenant_id));

        let (_, body) = send(app.router, get_req("/track/LGS-2026-DRVLOC")).await;

        // The production handler returns driver_location when status == OutForDelivery
        let driver_loc = &body["data"]["driver_location"];
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
        let history = body["data"]["history"].as_array().unwrap();
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

        let history = body["data"]["history"].as_array().unwrap();
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

        let history = body["data"]["history"].as_array().unwrap();
        assert_eq!(history.len(), 1);
    }

    #[tokio::test]
    async fn history_first_entry_is_pending() {
        let app = TestApp::build();
        let mut record = make_tracking_record("LGS-TIMELINE-004", app.tenant_id);
        record.transition(TrackingStatus::Confirmed, "Confirmed".into(), None);
        app.seed(record);

        let (_, body) = send(app.router, get_req("/track/LGS-TIMELINE-004")).await;

        let history = body["data"]["history"].as_array().unwrap();
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

        let history = body["data"]["history"].as_array().unwrap();
        let last_status = &history.last().unwrap()["status"];
        let current_status = &body["data"]["status"];
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
    async fn returns_404_for_cross_tenant_access() {
        let app = TestApp::build();
        let record = make_tracking_record("LGS-AUTH-004", app.tenant_id);
        let shipment_id = record.shipment_id;
        app.seed(record);

        // Token belongs to a different tenant
        let other_token = make_other_tenant_token();

        let (status, body) = send(
            app.router,
            get_req_auth(&format!("/v1/tracking/{}", shipment_id), &other_token),
        )
        .await;

        // A record the caller may not read is a 404, not a 403, so ids cannot
        // be probed across tenants (TrackingService::get_for). Nothing of the
        // record may leak in the body.
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(body.get("tracking_number").is_none());
        assert!(body.get("driver").is_none());
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
//
// Rewritten against the production `public_router`. The old tests drove a
// test-local handler that took `delay_hours` and called `mark_failed`; the
// production route takes `preferred_date` / `preferred_time_slot` / `reason`,
// validates the window and delegates to `TrackingRepository::reschedule`.
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod reschedule_tests {
    use super::*;

    fn in_days(n: i64) -> String {
        (Utc::now().date_naive() + chrono::Duration::days(n)).to_string()
    }

    #[tokio::test]
    async fn reschedule_returns_200_for_known_shipment() {
        let app = TestApp::build();
        app.seed(make_tracking_record("LGS-RESCHED-001", fixed_tenant_id()));

        let (status, body) = send(
            app.router.clone(),
            post_req_json(
                "/track/LGS-RESCHED-001/reschedule",
                &json!({ "preferred_date": in_days(2) }),
            ),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"]["rescheduled"], json!(true));
        assert_eq!(body["data"]["tracking_number"], json!("LGS-RESCHED-001"));
    }

    #[tokio::test]
    async fn reschedule_sets_next_attempt_at_from_date_and_slot() {
        let app = TestApp::build();
        app.seed(make_tracking_record("LGS-RESCHED-002", fixed_tenant_id()));
        let date = Utc::now().date_naive() + chrono::Duration::days(3);

        let (status, _) = send(
            app.router.clone(),
            post_req_json(
                "/track/LGS-RESCHED-002/reschedule",
                &json!({ "preferred_date": date.to_string(), "preferred_time_slot": "afternoon" }),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let stored = app.repo.get_by_number("LGS-RESCHED-002").unwrap();
        let expected = chrono::DateTime::<Utc>::from_naive_utc_and_offset(
            date.and_hms_opt(13, 0, 0).unwrap(),
            Utc,
        );
        assert_eq!(stored.next_attempt_at, Some(expected));
    }

    #[tokio::test]
    async fn reschedule_increments_reschedule_count_and_records_default_reason() {
        let app = TestApp::build();
        app.seed(make_tracking_record("LGS-RESCHED-003", fixed_tenant_id()));

        for _ in 0..2 {
            let (status, _) = send(
                app.router.clone(),
                post_req_json(
                    "/track/LGS-RESCHED-003/reschedule",
                    &json!({ "preferred_date": in_days(1) }),
                ),
            )
            .await;
            assert_eq!(status, StatusCode::OK);
        }

        assert_eq!(app.repo.get_by_number("LGS-RESCHED-003").unwrap().reschedule_count, 2);
        let calls = app.repo.reschedules.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].1, "Customer reschedule request");
    }

    #[tokio::test]
    async fn reschedule_returns_404_for_unknown_tracking_number() {
        let app = TestApp::build();

        let (status, _) = send(
            app.router.clone(),
            post_req_json(
                "/track/LGS-NOSUCH-999/reschedule",
                &json!({ "preferred_date": in_days(1) }),
            ),
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(app.repo.reschedules.lock().unwrap().is_empty());
    }

    // New coverage of the validation window the production route enforces
    // (the old test-local handler had none of it).
    #[tokio::test]
    async fn reschedule_rejects_past_and_too_far_dates_without_writing() {
        let app = TestApp::build();
        app.seed(make_tracking_record("LGS-RESCHED-004", fixed_tenant_id()));

        for days in [-1, 91] {
            let (status, body) = send(
                app.router.clone(),
                post_req_json(
                    "/track/LGS-RESCHED-004/reschedule",
                    &json!({ "preferred_date": in_days(days) }),
                ),
            )
            .await;
            // Invalid input is a client error, not a 500.
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "days={days}");
            assert!(body["error"].as_str().unwrap_or_default().contains("Preferred date"), "days={days}");
        }
        assert!(app.repo.reschedules.lock().unwrap().is_empty());
        assert_eq!(app.repo.get_by_number("LGS-RESCHED-004").unwrap().reschedule_count, 0);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /track/:tracking_number/feedback — customer submits delivery feedback
//
// Rewritten against the production `public_router`: 200 with
// `{"data":{"submitted":true}}`, body field `comments`, rating validated 1..=5
// by `TrackingService::submit_feedback`.
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod feedback_tests {
    use super::*;

    #[tokio::test]
    async fn feedback_returns_200_for_valid_rating() {
        let app = TestApp::build();
        app.seed(make_delivered_record("LGS-FB-001", fixed_tenant_id()));

        let (status, body) = send(
            app.router.clone(),
            post_req_json(
                "/track/LGS-FB-001/feedback",
                &json!({ "rating": 5, "comments": "Fast delivery!" }),
            ),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"]["submitted"], json!(true));
    }

    #[tokio::test]
    async fn feedback_stores_rating_tags_and_comments() {
        let app = TestApp::build();
        app.seed(make_delivered_record("LGS-FB-002", fixed_tenant_id()));

        send(
            app.router.clone(),
            post_req_json(
                "/track/LGS-FB-002/feedback",
                &json!({ "rating": 4, "comments": "Good service", "tags": ["on_time"] }),
            ),
        )
        .await;

        let rows = app.repo.feedback.lock().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].tracking_number, "LGS-FB-002");
        assert_eq!(rows[0].rating, 4);
        assert_eq!(rows[0].comments.as_deref(), Some("Good service"));
        assert_eq!(rows[0].tags, vec!["on_time".to_string()]);
    }

    #[tokio::test]
    async fn feedback_rating_below_1_returns_422_and_stores_nothing() {
        let app = TestApp::build();
        app.seed(make_delivered_record("LGS-FB-003", fixed_tenant_id()));

        let (status, body) = send(
            app.router.clone(),
            post_req_json("/track/LGS-FB-003/feedback", &json!({ "rating": 0 })),
        )
        .await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["error"], json!("Rating must be between 1 and 5"));
        assert!(app.repo.feedback.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn feedback_rating_above_5_returns_422_and_stores_nothing() {
        let app = TestApp::build();
        app.seed(make_delivered_record("LGS-FB-004", fixed_tenant_id()));

        let (status, body) = send(
            app.router.clone(),
            post_req_json("/track/LGS-FB-004/feedback", &json!({ "rating": 6 })),
        )
        .await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["error"], json!("Rating must be between 1 and 5"));
        assert!(app.repo.feedback.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn feedback_without_comments_is_accepted() {
        let app = TestApp::build();
        app.seed(make_delivered_record("LGS-FB-005", fixed_tenant_id()));

        let (status, _) = send(
            app.router.clone(),
            post_req_json("/track/LGS-FB-005/feedback", &json!({ "rating": 3 })),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let rows = app.repo.feedback.lock().unwrap();
        assert!(rows[0].comments.is_none());
        assert!(rows[0].tags.is_empty());
    }

    #[tokio::test]
    async fn feedback_unknown_tracking_number_is_404_and_writes_nothing() {
        let app = TestApp::build();

        let (status, _) = send(
            app.router.clone(),
            post_req_json("/track/LGS-NOSUCH-FB/feedback", &json!({ "rating": 5 })),
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(app.repo.feedback.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn feedback_row_carries_tenant_shipment_and_driver_from_the_record() {
        let app = TestApp::build();
        let mut record = make_out_for_delivery_record("LGS-FB-ATTR", fixed_tenant_id());
        let driver_id = record.driver_id;
        record.mark_delivered(Uuid::new_v4(), "Maria".into(), Utc::now());
        let shipment_id = record.shipment_id;
        app.seed(record);

        let (status, _) = send(
            app.router.clone(),
            post_req_json("/v1/tracking/LGS-FB-ATTR/feedback", &json!({ "rating": 5 })),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let rows = app.repo.feedback.lock().unwrap();
        assert_eq!(rows[0].tenant_id, fixed_tenant_id());
        assert_eq!(rows[0].shipment_id, shipment_id);
        assert!(driver_id.is_some());
        assert_eq!(rows[0].driver_id, driver_id);
    }

    #[tokio::test]
    async fn feedback_before_delivery_is_refused_and_writes_nothing() {
        let app = TestApp::build();
        app.seed(make_tracking_record("LGS-FB-EARLY", fixed_tenant_id()));

        let (status, _) = send(
            app.router.clone(),
            post_req_json("/track/LGS-FB-EARLY/feedback", &json!({ "rating": 5 })),
        )
        .await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(app.repo.feedback.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn second_feedback_for_the_same_shipment_is_409_and_keeps_the_first() {
        let app = TestApp::build();
        app.seed(make_delivered_record("LGS-FB-DUP", fixed_tenant_id()));

        let (first, _) = send(
            app.router.clone(),
            post_req_json("/track/LGS-FB-DUP/feedback", &json!({ "rating": 5 })),
        )
        .await;
        let (second, _) = send(
            app.router.clone(),
            post_req_json("/track/LGS-FB-DUP/feedback", &json!({ "rating": 1 })),
        )
        .await;

        assert_eq!(first, StatusCode::OK);
        assert_eq!(second, StatusCode::CONFLICT);
        let rows = app.repo.feedback.lock().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].rating, 5);
    }

    #[tokio::test]
    async fn feedback_oversized_comment_or_tags_are_422_and_store_nothing() {
        let app = TestApp::build();
        app.seed(make_delivered_record("LGS-FB-BIG", fixed_tenant_id()));

        for body in [
            json!({ "rating": 4, "comments": "x".repeat(1001) }),
            json!({ "rating": 4, "tags": vec!["t"; 11] }),
            json!({ "rating": 4, "tags": ["y".repeat(51)] }),
        ] {
            let (status, _) = send(
                app.router.clone(),
                post_req_json("/track/LGS-FB-BIG/feedback", &body),
            )
            .await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        }
        assert!(app.repo.feedback.lock().unwrap().is_empty());
    }

    fn delivered_with_driver(tn: &str) -> (TrackingRecord, Uuid, Uuid) {
        let mut record = make_out_for_delivery_record(tn, fixed_tenant_id());
        let driver_id = record.driver_id.expect("driver assigned");
        record.mark_delivered(Uuid::new_v4(), "Maria".into(), Utc::now());
        let shipment_id = record.shipment_id;
        (record, driver_id, shipment_id)
    }

    #[tokio::test]
    async fn feedback_publishes_event_with_tenant_shipment_driver_and_rating() {
        let publisher = Arc::new(RecordingPublisher::default());
        let app = TestApp::build_with_publisher(publisher.clone());
        let (record, driver_id, shipment_id) = delivered_with_driver("LGS-FB-EVT");
        app.seed(record);

        let (status, _) = send(
            app.router.clone(),
            post_req_json("/track/LGS-FB-EVT/feedback", &json!({ "rating": 4 })),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let sent = publisher.published.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, "logisticos.delivery.feedback.submitted");
        let evt: Value = serde_json::from_str(&sent[0].2).unwrap();
        assert_eq!(evt["tenant_id"], json!(fixed_tenant_id()));
        assert_eq!(evt["data"]["shipment_id"], json!(shipment_id));
        assert_eq!(evt["data"]["driver_id"], json!(driver_id));
        assert_eq!(evt["data"]["rating"], json!(4));
        assert!(evt["id"].is_string());
    }

    #[tokio::test]
    async fn feedback_without_a_driver_publishes_nothing() {
        let publisher = Arc::new(RecordingPublisher::default());
        let app = TestApp::build_with_publisher(publisher.clone());
        // make_delivered_record never assigned a driver.
        app.seed(make_delivered_record("LGS-FB-NODRV", fixed_tenant_id()));

        let (status, _) = send(
            app.router.clone(),
            post_req_json("/track/LGS-FB-NODRV/feedback", &json!({ "rating": 5 })),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert!(publisher.published.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn duplicate_feedback_does_not_publish_a_second_event() {
        let publisher = Arc::new(RecordingPublisher::default());
        let app = TestApp::build_with_publisher(publisher.clone());
        let (record, _, _) = delivered_with_driver("LGS-FB-EVT2");
        app.seed(record);

        for rating in [5, 1] {
            send(
                app.router.clone(),
                post_req_json("/track/LGS-FB-EVT2/feedback", &json!({ "rating": rating })),
            )
            .await;
        }

        let sent = publisher.published.lock().unwrap();
        assert_eq!(sent.len(), 1);
        let evt: Value = serde_json::from_str(&sent[0].2).unwrap();
        assert_eq!(evt["data"]["rating"], json!(5));
    }

    #[tokio::test]
    async fn feedback_succeeds_when_the_publish_fails() {
        let publisher = Arc::new(RecordingPublisher { fail: true, ..Default::default() });
        let app = TestApp::build_with_publisher(publisher);
        let (record, _, _) = delivered_with_driver("LGS-FB-EVT3");
        app.seed(record);

        let (status, body) = send(
            app.router.clone(),
            post_req_json("/track/LGS-FB-EVT3/feedback", &json!({ "rating": 3 })),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"]["submitted"], json!(true));
        assert_eq!(app.repo.feedback.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn feedback_each_valid_rating_value_is_accepted() {
        for rating in 1i16..=5 {
            let app = TestApp::build();
            let tn = format!("LGS-FB-RATING-{}", rating);
            app.seed(make_delivered_record(&tn, fixed_tenant_id()));

            let (status, _) = send(
                app.router.clone(),
                post_req_json(&format!("/track/{}/feedback", tn), &json!({ "rating": rating })),
            )
            .await;

            assert_eq!(status, StatusCode::OK, "rating {} should be accepted", rating);
            assert_eq!(app.repo.feedback.lock().unwrap()[0].rating, rating);
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

            let guard = st.repo.by_shipment.lock().unwrap();
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
            .uri(format!("/shipments/{}/status", shipment_id))
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

            let guard = st.repo.by_shipment.lock().unwrap();
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
            .uri(format!("/shipments/{}/events", shipment_id))
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
