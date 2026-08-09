// Integration tests for the Fleet Management HTTP API.
// InMemoryVehicleRepo + real Axum router + tower::ServiceExt::oneshot.

use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use async_trait::async_trait;
use axum::{body::Body, http::{header, Method, Request, StatusCode}, Router};
use chrono::NaiveDate;
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;
use logisticos_auth::{claims::Claims, jwt::JwtService};
use logisticos_types::TenantId;
use logisticos_fleet::{
    AppState, application::services::FleetService,
    domain::{
        entities::{Vehicle, VehicleId, VehicleType},
        repositories::VehicleRepository,
    },
};

// ─────────────────────────────────────────────────────────────────────────────
// In-memory repository
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Default, Clone)]
struct InMemoryVehicleRepo {
    store: Arc<Mutex<HashMap<Uuid, Vehicle>>>,
}

#[async_trait]
impl VehicleRepository for InMemoryVehicleRepo {
    async fn find_by_id(&self, id: &VehicleId) -> anyhow::Result<Option<Vehicle>> {
        Ok(self.store.lock().unwrap().get(&id.inner()).cloned())
    }
    async fn find_by_driver(&self, _t: &TenantId, did: Uuid) -> anyhow::Result<Option<Vehicle>> {
        Ok(self.store.lock().unwrap().values()
            .find(|v| v.assigned_driver_id == Some(did)).cloned())
    }
    async fn list(&self, tid: &TenantId, limit: i64, _offset: i64) -> anyhow::Result<Vec<Vehicle>> {
        let s = self.store.lock().unwrap();
        let mut r: Vec<Vehicle> = s.values().filter(|v| &v.tenant_id == tid).cloned().collect();
        r.truncate(limit as usize);
        Ok(r)
    }
    async fn list_maintenance_due(&self, tid: &TenantId, days: i64) -> anyhow::Result<Vec<Vehicle>> {
        Ok(self.store.lock().unwrap().values()
            .filter(|v| &v.tenant_id == tid && v.is_maintenance_due_within(days))
            .cloned().collect())
    }
    async fn save(&self, v: &Vehicle) -> anyhow::Result<()> {
        self.store.lock().unwrap().insert(v.id.inner(), v.clone());
        Ok(())
    }
    async fn count(&self, tid: &TenantId) -> anyhow::Result<i64> {
        Ok(self.store.lock().unwrap().values()
            .filter(|v| &v.tenant_id == tid).count() as i64)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Mirrors `bootstrap::run`, including the auth layer.
///
/// The suite previously built the router without it, which is why the tests
/// could not have caught that production had the same omission — and, once they
/// finally compiled, why every request came back 500 AUTH_NOT_CONFIGURED rather
/// than the 401 the unauthenticated cases expect.
fn make_app(repo: InMemoryVehicleRepo) -> Router {
    let svc = Arc::new(FleetService::new(Arc::new(repo)));
    let jwt: logisticos_auth::middleware::AuthState =
        Arc::new(JwtService::new(TEST_SECRET, 3600, 86_400));
    logisticos_fleet::api::http::router()
        .layer(axum::middleware::from_fn_with_state(
            jwt,
            logisticos_auth::middleware::require_auth,
        ))
        .with_state(AppState { fleet_svc: svc })
}

/// One secret, shared by the router and every token the tests mint. They must
/// agree or validation fails for reasons unrelated to what is being tested.
const TEST_SECRET: &str = "test-secret-key-for-logisticos-testing";

fn make_jwt(tid: Uuid) -> String {
    let svc = JwtService::new(TEST_SECRET, 3600, 86_400);
    let c = Claims::new(
        Uuid::new_v4(),
        tid,
        "demo".into(),
        "enterprise".into(),
        "fleet-test@example.com".into(),
        vec!["fleet_manager".into()],
        vec!["fleet:read".into(), "fleet:manage".into()],
        3600,
    );
    svc.issue_access_token(c).unwrap()
}

fn bearer(t: &str) -> String { format!("Bearer {}", t) }
fn jbody(v: &Value) -> Body { Body::from(serde_json::to_vec(v).unwrap()) }

async fn call(app: Router, req: Request<Body>) -> (StatusCode, Value) {
    let r = app.oneshot(req).await.unwrap();
    let s = r.status();
    let b = axum::body::to_bytes(r.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&b).unwrap_or(Value::Null);
    (s, v)
}

fn mk(tid: Uuid) -> Vehicle {
    Vehicle::new(TenantId::from_uuid(tid), "ABC 1234".into(), VehicleType::Motorcycle,
        "Honda".into(), "Beat".into(), 2022, "Red".into())
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /v1/vehicles
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_vehicles_empty() {
    let tid = Uuid::new_v4();
    let req = Request::builder().method(Method::GET).uri("/v1/fleet/vehicles")
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .body(Body::empty()).unwrap();
    let (s, b) = call(make_app(Default::default()), req).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["data"].as_array().unwrap().len(), 0);
    assert_eq!(b["total"], 0);
}

#[tokio::test]
async fn list_vehicles_scoped_to_tenant() {
    let tid = Uuid::new_v4();
    let other = Uuid::new_v4();
    let repo: InMemoryVehicleRepo = Default::default();
    repo.save(&mk(tid)).await.unwrap();
    repo.save(&mk(tid)).await.unwrap();
    repo.save(&mk(other)).await.unwrap();
    let req = Request::builder().method(Method::GET).uri("/v1/fleet/vehicles")
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .body(Body::empty()).unwrap();
    let (s, b) = call(make_app(repo), req).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["data"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn list_vehicles_requires_auth() {
    let req = Request::builder().method(Method::GET).uri("/v1/fleet/vehicles")
        .body(Body::empty()).unwrap();
    let r = make_app(Default::default()).oneshot(req).await.unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /v1/vehicles
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_vehicle_returns_201() {
    let tid = Uuid::new_v4();
    let payload = serde_json::json!({
        "plate_number": "XYZ 9999", "vehicle_type": "van",
        "make": "Toyota", "model": "HiAce", "year": 2023, "color": "Silver"
    });
    let req = Request::builder().method(Method::POST).uri("/v1/fleet/vehicles")
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .header(header::CONTENT_TYPE, "application/json")
        .body(jbody(&payload)).unwrap();
    let (s, b) = call(make_app(Default::default()), req).await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(b["data"]["plate"], "XYZ 9999");
    // "idle", not "active". The wire status is a projection, not the domain
    // enum: `Active` with a driver reads "active", `Active` without one reads
    // "idle", and a vehicle this new has nobody assigned. See
    // `VehicleApiResponse::from`.
    assert_eq!(b["data"]["status"], "idle");
    // Asymmetric on purpose: `VehicleType` serialises PascalCase and keeps
    // snake_case `alias`es so rows written by the old serde still read. So the
    // request above may say "van" and the response says "Van".
    assert_eq!(b["data"]["type"], "Van");
}

#[tokio::test]
async fn create_vehicle_persisted_in_repo() {
    let tid = Uuid::new_v4();
    let repo: InMemoryVehicleRepo = Default::default();
    let payload = serde_json::json!({
        "plate_number": "NEW 001", "vehicle_type": "motorcycle",
        "make": "Honda", "model": "Click", "year": 2024, "color": "Black"
    });
    let req = Request::builder().method(Method::POST).uri("/v1/fleet/vehicles")
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .header(header::CONTENT_TYPE, "application/json")
        .body(jbody(&payload)).unwrap();
    let (s, _) = call(make_app(repo.clone()), req).await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(repo.store.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn create_vehicle_requires_manage_permission() {
    // Only fleet:read — no manage
    let tid = Uuid::new_v4();
    let svc = JwtService::new(TEST_SECRET, 3600, 86_400);
    let claims = Claims::new(
        Uuid::new_v4(),
        tid,
        "demo".into(),
        "enterprise".into(),
        "fleet-test@example.com".into(),
        vec!["fleet_viewer".into()],
        // Read only — no fleet:manage. That absence is the subject of this test.
        vec!["fleet:read".into()],
        3600,
    );
    let token = svc.issue_access_token(claims).unwrap();
    let payload = serde_json::json!({
        "plate_number": "NOPE 01", "vehicle_type": "car",
        "make": "Toyota", "model": "Vios", "year": 2022, "color": "Red"
    });
    let req = Request::builder().method(Method::POST).uri("/v1/fleet/vehicles")
        .header(header::AUTHORIZATION, format!("Bearer {}", token))
        .header(header::CONTENT_TYPE, "application/json")
        .body(jbody(&payload)).unwrap();
    let (s, _) = call(make_app(Default::default()), req).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /v1/vehicles/:id
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_vehicle_by_id() {
    let tid = Uuid::new_v4();
    let v = mk(tid); let vid = v.id.inner();
    let repo: InMemoryVehicleRepo = Default::default();
    repo.save(&v).await.unwrap();
    let req = Request::builder().method(Method::GET)
        .uri(format!("/v1/fleet/vehicles/{}", vid))
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .body(Body::empty()).unwrap();
    let (s, b) = call(make_app(repo), req).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["data"]["id"], vid.to_string());
    assert_eq!(b["data"]["plate"], "ABC 1234");
}

#[tokio::test]
async fn get_vehicle_not_found() {
    let tid = Uuid::new_v4();
    let req = Request::builder().method(Method::GET)
        .uri(format!("/v1/fleet/vehicles/{}", Uuid::new_v4()))
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .body(Body::empty()).unwrap();
    let (s, _) = call(make_app(Default::default()), req).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn get_vehicle_cross_tenant_forbidden() {
    let owner = Uuid::new_v4(); let other = Uuid::new_v4();
    let v = mk(owner); let vid = v.id.inner();
    let repo: InMemoryVehicleRepo = Default::default();
    repo.save(&v).await.unwrap();
    let req = Request::builder().method(Method::GET)
        .uri(format!("/v1/fleet/vehicles/{}", vid))
        .header(header::AUTHORIZATION, bearer(&make_jwt(other)))
        .body(Body::empty()).unwrap();
    let (s, _) = call(make_app(repo), req).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

// ─────────────────────────────────────────────────────────────────────────────
// DELETE /v1/vehicles/:id  (decommission)
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn decommission_vehicle_sets_status() {
    let tid = Uuid::new_v4();
    let v = mk(tid); let vid = v.id.inner();
    let repo: InMemoryVehicleRepo = Default::default();
    repo.save(&v).await.unwrap();
    let req = Request::builder().method(Method::DELETE)
        .uri(format!("/v1/fleet/vehicles/{}", vid))
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .body(Body::empty()).unwrap();
    let (s, b) = call(make_app(repo), req).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["data"]["status"], "offline");  // Decommissioned projects to "offline" on the wire.
}

#[tokio::test]
async fn decommission_vehicle_not_found() {
    let tid = Uuid::new_v4();
    let req = Request::builder().method(Method::DELETE)
        .uri(format!("/v1/fleet/vehicles/{}", Uuid::new_v4()))
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .body(Body::empty()).unwrap();
    let (s, _) = call(make_app(Default::default()), req).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /v1/vehicles/:id/assign-driver
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn assign_driver_success() {
    let tid = Uuid::new_v4();
    let v = mk(tid); let vid = v.id.inner();
    let did = Uuid::new_v4();
    let repo: InMemoryVehicleRepo = Default::default();
    repo.save(&v).await.unwrap();
    let payload = serde_json::json!({ "driver_id": did });
    let req = Request::builder().method(Method::POST)
        .uri(format!("/v1/fleet/vehicles/{}/assign-driver", vid))
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .header(header::CONTENT_TYPE, "application/json")
        .body(jbody(&payload)).unwrap();
    let (s, b) = call(make_app(repo), req).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["data"]["driver_id"], did.to_string());
}

#[tokio::test]
async fn assign_driver_fails_when_decommissioned() {
    let tid = Uuid::new_v4();
    let mut v = mk(tid); v.decommission();
    let vid = v.id.inner();
    let repo: InMemoryVehicleRepo = Default::default();
    repo.save(&v).await.unwrap();
    let payload = serde_json::json!({ "driver_id": Uuid::new_v4() });
    let req = Request::builder().method(Method::POST)
        .uri(format!("/v1/fleet/vehicles/{}/assign-driver", vid))
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .header(header::CONTENT_TYPE, "application/json")
        .body(jbody(&payload)).unwrap();
    let (s, _) = call(make_app(repo), req).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /v1/vehicles/:id/unassign-driver
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn unassign_driver_clears_assignment() {
    let tid = Uuid::new_v4();
    let mut v = mk(tid); v.assign_driver(Uuid::new_v4()).unwrap();
    let vid = v.id.inner();
    let repo: InMemoryVehicleRepo = Default::default();
    repo.save(&v).await.unwrap();
    let req = Request::builder().method(Method::POST)
        .uri(format!("/v1/fleet/vehicles/{}/unassign-driver", vid))
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .body(Body::empty()).unwrap();
    let (s, b) = call(make_app(repo), req).await;
    assert_eq!(s, StatusCode::OK);
    assert!(b["data"]["driver_id"].is_null());
}

#[tokio::test]
async fn unassign_driver_on_vehicle_with_no_driver() {
    let tid = Uuid::new_v4();
    let v = mk(tid); let vid = v.id.inner();
    let repo: InMemoryVehicleRepo = Default::default();
    repo.save(&v).await.unwrap();
    let req = Request::builder().method(Method::POST)
        .uri(format!("/v1/fleet/vehicles/{}/unassign-driver", vid))
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .body(Body::empty()).unwrap();
    let (s, b) = call(make_app(repo), req).await;
    assert_eq!(s, StatusCode::OK);
    assert!(b["data"]["driver_id"].is_null());
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /v1/vehicles/:id/maintenance
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn schedule_maintenance_endpoint() {
    let tid = Uuid::new_v4();
    let v = mk(tid); let vid = v.id.inner();
    let repo: InMemoryVehicleRepo = Default::default();
    repo.save(&v).await.unwrap();
    let payload = serde_json::json!({
        "description": "30k km service",
        "scheduled_date": "2026-04-15"
    });
    let req = Request::builder().method(Method::POST)
        .uri(format!("/v1/fleet/vehicles/{}/maintenance", vid))
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .header(header::CONTENT_TYPE, "application/json")
        .body(jbody(&payload)).unwrap();
    let (s, b) = call(make_app(repo.clone()), req).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["data"]["status"], "maintenance");
    // The due date is no longer on the wire — `VehicleApiResponse` exposes
    // `next_service_km`, not a date. Asserted against the stored vehicle
    // instead, which is the fact that matters: scheduling recorded the date.
    let stored = repo.find_by_id(&VehicleId::from_uuid(vid)).await.unwrap().unwrap();
    assert_eq!(
        stored.next_maintenance_due.map(|d| d.to_string()).as_deref(),
        Some("2026-04-15"),
    );
}

#[tokio::test]
async fn schedule_maintenance_records_stored() {
    let tid = Uuid::new_v4();
    let v = mk(tid); let vid = v.id.inner();
    let repo: InMemoryVehicleRepo = Default::default();
    repo.save(&v).await.unwrap();
    let payload = serde_json::json!({
        "description": "Oil change", "scheduled_date": "2026-05-01"
    });
    let req = Request::builder().method(Method::POST)
        .uri(format!("/v1/fleet/vehicles/{}/maintenance", vid))
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .header(header::CONTENT_TYPE, "application/json")
        .body(jbody(&payload)).unwrap();
    let (s, b) = call(make_app(repo.clone()), req).await;
    assert_eq!(s, StatusCode::OK);
    // `maintenance_history` is not part of the wire projection. Asserted on the
    // stored vehicle, which is where the record actually has to land.
    let stored = repo.find_by_id(&VehicleId::from_uuid(vid)).await.unwrap().unwrap();
    assert_eq!(stored.maintenance_history.len(), 1);
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /v1/vehicles/:id/maintenance/complete
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn complete_maintenance_endpoint() {
    let tid = Uuid::new_v4();
    let mut v = mk(tid);
    v.schedule_maintenance("Service".into(), NaiveDate::from_ymd_opt(2026, 4, 1).unwrap());
    let vid = v.id.inner();
    let repo: InMemoryVehicleRepo = Default::default();
    repo.save(&v).await.unwrap();
    let payload = serde_json::json!({
        "odometer_km": 35000, "cost_cents": 450000, "notes": "Changed oil and filters"
    });
    let req = Request::builder().method(Method::POST)
        .uri(format!("/v1/fleet/vehicles/{}/maintenance/complete", vid))
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .header(header::CONTENT_TYPE, "application/json")
        .body(jbody(&payload)).unwrap();
    let (s, b) = call(make_app(repo.clone()), req).await;
    assert_eq!(s, StatusCode::OK);
    // "idle": completing maintenance returns the vehicle to `Active`, and an
    // unassigned Active vehicle projects as "idle".
    assert_eq!(b["data"]["status"], "idle");
    // Odometer and due-date are not on the wire; both are checked where they
    // are actually written.
    let stored = repo.find_by_id(&VehicleId::from_uuid(vid)).await.unwrap().unwrap();
    assert_eq!(stored.odometer_km, 35_000);
    assert!(stored.next_maintenance_due.is_none());
}

#[tokio::test]
async fn complete_maintenance_fails_when_no_pending() {
    let tid = Uuid::new_v4();
    let v = mk(tid); let vid = v.id.inner();
    let repo: InMemoryVehicleRepo = Default::default();
    repo.save(&v).await.unwrap();
    let payload = serde_json::json!({
        "odometer_km": 10000, "cost_cents": 100000
    });
    let req = Request::builder().method(Method::POST)
        .uri(format!("/v1/fleet/vehicles/{}/maintenance/complete", vid))
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .header(header::CONTENT_TYPE, "application/json")
        .body(jbody(&payload)).unwrap();
    let (s, _) = call(make_app(repo), req).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /v1/vehicles/maintenance-alerts
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn maintenance_alerts_empty_when_no_vehicles() {
    let tid = Uuid::new_v4();
    let req = Request::builder().method(Method::GET)
        .uri("/v1/fleet/vehicles/maintenance-alerts?within_days=7")
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .body(Body::empty()).unwrap();
    let (s, b) = call(make_app(Default::default()), req).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["count"], 0);
    assert_eq!(b["data"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn maintenance_alerts_returns_vehicles_due_soon() {
    let tid = Uuid::new_v4();
    let mut v1 = mk(tid); let mut v2 = mk(tid);
    let soon = (chrono::Local::now() + chrono::Duration::days(3)).date_naive();
    let far  = (chrono::Local::now() + chrono::Duration::days(30)).date_naive();
    v1.schedule_maintenance("Soon".into(), soon);
    v2.schedule_maintenance("Far".into(), far);
    let repo: InMemoryVehicleRepo = Default::default();
    repo.save(&v1).await.unwrap();
    repo.save(&v2).await.unwrap();
    let req = Request::builder().method(Method::GET)
        .uri("/v1/fleet/vehicles/maintenance-alerts?within_days=7")
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .body(Body::empty()).unwrap();
    let (s, b) = call(make_app(repo), req).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["count"], 1);
    assert_eq!(b["data"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn maintenance_alerts_default_window_is_seven_days() {
    let tid = Uuid::new_v4();
    let mut v = mk(tid);
    let soon = (chrono::Local::now() + chrono::Duration::days(5)).date_naive();
    v.schedule_maintenance("Soon".into(), soon);
    let repo: InMemoryVehicleRepo = Default::default();
    repo.save(&v).await.unwrap();
    // No within_days param — defaults to 7
    let req = Request::builder().method(Method::GET)
        .uri("/v1/fleet/vehicles/maintenance-alerts")
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .body(Body::empty()).unwrap();
    let (s, b) = call(make_app(repo), req).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["count"], 1);
}
