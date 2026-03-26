// Integration tests for the Carrier Management HTTP API.
// InMemoryCarrierRepo + real Axum router + tower::ServiceExt::oneshot.

use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use async_trait::async_trait;
use axum::{body::Body, http::{header, Method, Request, StatusCode}, Router};
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;
use logisticos_auth::{claims::Claims, jwt::JwtService};
use logisticos_types::TenantId;
use logisticos_carrier::{
    AppState, application::services::CarrierService,
    domain::{
        entities::{Carrier, CarrierId, CarrierStatus, RateCard, SlaCommitment},
        repositories::CarrierRepository,
    },
};

// ─────────────────────────────────────────────────────────────────────────────
// In-memory repository
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Default, Clone)]
struct InMemoryCarrierRepo {
    store: Arc<Mutex<HashMap<Uuid, Carrier>>>,
}

#[async_trait]
impl CarrierRepository for InMemoryCarrierRepo {
    async fn find_by_id(&self, id: &CarrierId) -> anyhow::Result<Option<Carrier>> {
        Ok(self.store.lock().unwrap().get(&id.inner()).cloned())
    }
    async fn find_by_code(&self, tid: &TenantId, code: &str) -> anyhow::Result<Option<Carrier>> {
        let up = code.to_uppercase();
        Ok(self.store.lock().unwrap().values()
            .find(|c| &c.tenant_id == tid && c.code == up).cloned())
    }
    async fn list(&self, tid: &TenantId, limit: i64, _offset: i64) -> anyhow::Result<Vec<Carrier>> {
        let s = self.store.lock().unwrap();
        let mut r: Vec<Carrier> = s.values().filter(|c| &c.tenant_id == tid).cloned().collect();
        r.truncate(limit as usize);
        Ok(r)
    }
    async fn list_active(&self, tid: &TenantId) -> anyhow::Result<Vec<Carrier>> {
        Ok(self.store.lock().unwrap().values()
            .filter(|c| &c.tenant_id == tid && c.status == CarrierStatus::Active)
            .cloned().collect())
    }
    async fn save(&self, c: &Carrier) -> anyhow::Result<()> {
        self.store.lock().unwrap().insert(c.id.inner(), c.clone());
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn make_app(repo: InMemoryCarrierRepo) -> Router {
    let svc = Arc::new(CarrierService::new(Arc::new(repo)));
    logisticos_carrier::api::http::router().with_state(AppState { carrier_svc: svc })
}

fn make_jwt(tid: Uuid) -> String {
    let svc = JwtService::new("test-secret-key-for-logisticos-testing");
    let c = Claims::new(Uuid::new_v4(), tid,
        vec!["carriers:read".into(), "carriers:manage".into()]);
    svc.encode(&c).unwrap()
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

fn mk_carrier(tid: Uuid) -> Carrier {
    Carrier::new(
        TenantId::from_uuid(tid),
        "J&T Express".into(),
        "JNT".into(),
        "ops@jnt.ph".into(),
        SlaCommitment { on_time_target_pct: 92.0, max_delivery_days: 3, penalty_per_breach: 5000 },
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /v1/carriers
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_carriers_empty() {
    let tid = Uuid::new_v4();
    let req = Request::builder().method(Method::GET).uri("/v1/carriers")
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .body(Body::empty()).unwrap();
    let (s, b) = call(make_app(Default::default()), req).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["carriers"].as_array().unwrap().len(), 0);
    assert_eq!(b["count"], 0);
}

#[tokio::test]
async fn list_carriers_scoped_to_tenant() {
    let tid = Uuid::new_v4();
    let other = Uuid::new_v4();
    let repo: InMemoryCarrierRepo = Default::default();
    let mut c2 = mk_carrier(tid);
    c2.code = "LBC".into();
    repo.save(&mk_carrier(tid)).await.unwrap();
    repo.save(&c2).await.unwrap();
    repo.save(&mk_carrier(other)).await.unwrap();
    let req = Request::builder().method(Method::GET).uri("/v1/carriers")
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .body(Body::empty()).unwrap();
    let (s, b) = call(make_app(repo), req).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["carriers"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn list_carriers_requires_auth() {
    let req = Request::builder().method(Method::GET).uri("/v1/carriers")
        .body(Body::empty()).unwrap();
    let r = make_app(Default::default()).oneshot(req).await.unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /v1/carriers  (onboard)
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn onboard_carrier_succeeds() {
    let tid = Uuid::new_v4();
    let payload = serde_json::json!({
        "name": "LBC Express", "code": "lbc",
        "contact_email": "ops@lbc.ph",
        "sla_target": 90.0, "max_delivery_days": 3
    });
    let req = Request::builder().method(Method::POST).uri("/v1/carriers")
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .header(header::CONTENT_TYPE, "application/json")
        .body(jbody(&payload)).unwrap();
    let (s, b) = call(make_app(Default::default()), req).await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(b["name"], "LBC Express");
    // Code is uppercased by domain
    assert_eq!(b["code"], "LBC");
    assert_eq!(b["status"], "pending_verification");
}

#[tokio::test]
async fn onboard_carrier_code_uppercased() {
    let tid = Uuid::new_v4();
    let payload = serde_json::json!({
        "name": "Grab Express", "code": "grab",
        "contact_email": "logistics@grab.com",
        "sla_target": 88.0, "max_delivery_days": 2
    });
    let req = Request::builder().method(Method::POST).uri("/v1/carriers")
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .header(header::CONTENT_TYPE, "application/json")
        .body(jbody(&payload)).unwrap();
    let (s, b) = call(make_app(Default::default()), req).await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(b["code"], "GRAB");
}

#[tokio::test]
async fn onboard_carrier_duplicate_code_rejected() {
    let tid = Uuid::new_v4();
    let repo: InMemoryCarrierRepo = Default::default();
    repo.save(&mk_carrier(tid)).await.unwrap(); // code = JNT
    let payload = serde_json::json!({
        "name": "Another JNT", "code": "JNT",
        "contact_email": "other@jnt.ph",
        "sla_target": 85.0, "max_delivery_days": 3
    });
    let req = Request::builder().method(Method::POST).uri("/v1/carriers")
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .header(header::CONTENT_TYPE, "application/json")
        .body(jbody(&payload)).unwrap();
    let (s, _) = call(make_app(repo), req).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn onboard_carrier_same_code_different_tenants_ok() {
    let t1 = Uuid::new_v4(); let t2 = Uuid::new_v4();
    let repo: InMemoryCarrierRepo = Default::default();
    repo.save(&mk_carrier(t1)).await.unwrap(); // code JNT for t1
    let payload = serde_json::json!({
        "name": "J&T Express", "code": "JNT",
        "contact_email": "ops@jnt.ph",
        "sla_target": 90.0, "max_delivery_days": 3
    });
    let req = Request::builder().method(Method::POST).uri("/v1/carriers")
        .header(header::AUTHORIZATION, bearer(&make_jwt(t2)))
        .header(header::CONTENT_TYPE, "application/json")
        .body(jbody(&payload)).unwrap();
    let (s, _) = call(make_app(repo), req).await;
    assert_eq!(s, StatusCode::CREATED);
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /v1/carriers/:id
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_carrier_by_id() {
    let tid = Uuid::new_v4();
    let c = mk_carrier(tid); let cid = c.id.inner();
    let repo: InMemoryCarrierRepo = Default::default();
    repo.save(&c).await.unwrap();
    let req = Request::builder().method(Method::GET)
        .uri(format!("/v1/carriers/{}", cid))
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .body(Body::empty()).unwrap();
    let (s, b) = call(make_app(repo), req).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["id"], cid.to_string());
    assert_eq!(b["code"], "JNT");
}

#[tokio::test]
async fn get_carrier_not_found() {
    let tid = Uuid::new_v4();
    let req = Request::builder().method(Method::GET)
        .uri(format!("/v1/carriers/{}", Uuid::new_v4()))
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .body(Body::empty()).unwrap();
    let (s, _) = call(make_app(Default::default()), req).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /v1/carriers/:id/activate
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn activate_carrier_from_pending() {
    let tid = Uuid::new_v4();
    let c = mk_carrier(tid); let cid = c.id.inner();
    assert_eq!(c.status, CarrierStatus::PendingVerification);
    let repo: InMemoryCarrierRepo = Default::default();
    repo.save(&c).await.unwrap();
    let req = Request::builder().method(Method::POST)
        .uri(format!("/v1/carriers/{}/activate", cid))
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .body(Body::empty()).unwrap();
    let (s, b) = call(make_app(repo), req).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["status"], "active");
}

#[tokio::test]
async fn activate_carrier_from_suspended() {
    let tid = Uuid::new_v4();
    let mut c = mk_carrier(tid);
    c.suspend("Test");
    let cid = c.id.inner();
    let repo: InMemoryCarrierRepo = Default::default();
    repo.save(&c).await.unwrap();
    let req = Request::builder().method(Method::POST)
        .uri(format!("/v1/carriers/{}/activate", cid))
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .body(Body::empty()).unwrap();
    let (s, b) = call(make_app(repo), req).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["status"], "active");
}

// ─────────────────────────────────────────────────────────────────────────────
// POST /v1/carriers/:id/suspend
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn suspend_carrier() {
    let tid = Uuid::new_v4();
    let mut c = mk_carrier(tid);
    c.activate().unwrap();
    let cid = c.id.inner();
    let repo: InMemoryCarrierRepo = Default::default();
    repo.save(&c).await.unwrap();
    let payload = serde_json::json!({ "reason": "SLA breach — 3 consecutive misses" });
    let req = Request::builder().method(Method::POST)
        .uri(format!("/v1/carriers/{}/suspend", cid))
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .header(header::CONTENT_TYPE, "application/json")
        .body(jbody(&payload)).unwrap();
    let (s, b) = call(make_app(repo), req).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["status"], "suspended");
}

// ─────────────────────────────────────────────────────────────────────────────
// GET /v1/carriers/rate-shop
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn rate_shop_returns_empty_when_no_active_carriers() {
    let tid = Uuid::new_v4();
    let req = Request::builder().method(Method::GET)
        .uri("/v1/carriers/rate-shop?service_type=standard&weight_kg=2.5")
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .body(Body::empty()).unwrap();
    let (s, b) = call(make_app(Default::default()), req).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["quotes"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn rate_shop_only_active_carriers() {
    let tid = Uuid::new_v4();
    let repo: InMemoryCarrierRepo = Default::default();

    // Active carrier with rate card
    let mut active = mk_carrier(tid);
    active.activate().unwrap();
    active.rate_cards.push(RateCard {
        service_type: "standard".into(),
        base_rate_cents: 10000,
        per_kg_cents: 500,
        max_weight_kg: 30.0,
        coverage_zones: vec!["NCR".into()],
    });

    // Pending carrier — should not appear in quotes
    let pending = mk_carrier(tid);

    repo.save(&active).await.unwrap();
    repo.save(&pending).await.unwrap();

    let req = Request::builder().method(Method::GET)
        .uri("/v1/carriers/rate-shop?service_type=standard&weight_kg=2.0")
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .body(Body::empty()).unwrap();
    let (s, b) = call(make_app(repo), req).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["quotes"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn rate_shop_sorted_by_total_cost() {
    let tid = Uuid::new_v4();
    let repo: InMemoryCarrierRepo = Default::default();

    let mut c1 = mk_carrier(tid);
    let mut c2 = Carrier::new(
        TenantId::from_uuid(tid), "LBC".into(), "LBC".into(),
        "ops@lbc.ph".into(), SlaCommitment::default(),
    );
    c1.activate().unwrap();
    c2.activate().unwrap();

    c1.rate_cards.push(RateCard {
        service_type: "standard".into(), base_rate_cents: 20000,
        per_kg_cents: 1000, max_weight_kg: 30.0,
        coverage_zones: vec!["NCR".into()],
    });
    c2.rate_cards.push(RateCard {
        service_type: "standard".into(), base_rate_cents: 10000,
        per_kg_cents: 500, max_weight_kg: 30.0,
        coverage_zones: vec!["NCR".into()],
    });

    repo.save(&c1).await.unwrap();
    repo.save(&c2).await.unwrap();

    let req = Request::builder().method(Method::GET)
        .uri("/v1/carriers/rate-shop?service_type=standard&weight_kg=1.0")
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .body(Body::empty()).unwrap();
    let (s, b) = call(make_app(repo), req).await;
    assert_eq!(s, StatusCode::OK);
    let quotes = b["quotes"].as_array().unwrap();
    assert_eq!(quotes.len(), 2);
    // Cheapest (LBC — 10000+500) should be first
    assert_eq!(quotes[0]["carrier_code"], "LBC");
}

#[tokio::test]
async fn rate_shop_requires_auth() {
    let req = Request::builder().method(Method::GET)
        .uri("/v1/carriers/rate-shop?service_type=standard&weight_kg=1.0")
        .body(Body::empty()).unwrap();
    let r = make_app(Default::default()).oneshot(req).await.unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn rate_shop_no_matching_rate_cards() {
    let tid = Uuid::new_v4();
    let repo: InMemoryCarrierRepo = Default::default();
    let mut c = mk_carrier(tid);
    c.activate().unwrap();
    c.rate_cards.push(RateCard {
        service_type: "same_day".into(), base_rate_cents: 30000,
        per_kg_cents: 2000, max_weight_kg: 10.0,
        coverage_zones: vec!["NCR".into()],
    });
    repo.save(&c).await.unwrap();

    // Request "standard" but carrier only has "same_day"
    let req = Request::builder().method(Method::GET)
        .uri("/v1/carriers/rate-shop?service_type=standard&weight_kg=1.0")
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .body(Body::empty()).unwrap();
    let (s, b) = call(make_app(repo), req).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["quotes"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn rate_shop_excludes_over_max_weight() {
    let tid = Uuid::new_v4();
    let repo: InMemoryCarrierRepo = Default::default();
    let mut c = mk_carrier(tid);
    c.activate().unwrap();
    c.rate_cards.push(RateCard {
        service_type: "standard".into(), base_rate_cents: 10000,
        per_kg_cents: 500, max_weight_kg: 5.0,  // max 5kg
        coverage_zones: vec!["NCR".into()],
    });
    repo.save(&c).await.unwrap();

    // Request 10kg — exceeds max_weight_kg=5.0
    let req = Request::builder().method(Method::GET)
        .uri("/v1/carriers/rate-shop?service_type=standard&weight_kg=10.0")
        .header(header::AUTHORIZATION, bearer(&make_jwt(tid)))
        .body(Body::empty()).unwrap();
    let (s, b) = call(make_app(repo), req).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["quotes"].as_array().unwrap().len(), 0);
}
