//! Integration tests for the marketing service HTTP API.
//!
//! These tests construct an in-process Axum application backed by an
//! in-memory mock repository and a no-op event publisher, then drive it
//! with `axum_test::TestServer` (or equivalent `tower::ServiceExt` calls).
//!
//! No database or Kafka broker is required.

use std::sync::Arc;
use std::collections::HashMap;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::{Duration, Utc};
use serde_json::{json, Value};
use tokio::sync::Mutex;
use tower::ServiceExt; // for `.oneshot()`
use uuid::Uuid;

use logisticos_marketing::{
    api::http,
    application::services::{CampaignService, EventPublisher},
    domain::{
        entities::{Campaign, CampaignId, CampaignStatus, Channel, MessageTemplate, TargetingRule},
        repositories::CampaignRepository,
    },
    AppState,
};
use logisticos_types::TenantId;

// ---------------------------------------------------------------------------
// Mock repository
// ---------------------------------------------------------------------------

#[derive(Default)]
struct MockCampaignRepo {
    store: Mutex<HashMap<Uuid, Campaign>>,
}

#[async_trait]
impl CampaignRepository for MockCampaignRepo {
    async fn find_by_id(&self, id: &CampaignId) -> anyhow::Result<Option<Campaign>> {
        let store = self.store.lock().await;
        Ok(store.get(&id.inner()).cloned())
    }

    async fn list(&self, tenant_id: &TenantId, limit: i64, offset: i64) -> anyhow::Result<Vec<Campaign>> {
        let store = self.store.lock().await;
        let mut results: Vec<Campaign> = store
            .values()
            .filter(|c| &c.tenant_id == tenant_id)
            .cloned()
            .collect();
        // Stable ordering by created_at for deterministic pagination tests.
        results.sort_by_key(|c| c.created_at);
        let start = (offset as usize).min(results.len());
        let end = (start + limit as usize).min(results.len());
        Ok(results[start..end].to_vec())
    }

    async fn list_by_status(&self, tenant_id: &TenantId, status: &CampaignStatus) -> anyhow::Result<Vec<Campaign>> {
        let store = self.store.lock().await;
        Ok(store
            .values()
            .filter(|c| &c.tenant_id == tenant_id && &c.status == status)
            .cloned()
            .collect())
    }

    async fn save(&self, campaign: &Campaign) -> anyhow::Result<()> {
        let mut store = self.store.lock().await;
        store.insert(campaign.id.inner(), campaign.clone());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Mock event publisher
// ---------------------------------------------------------------------------

#[derive(Default)]
struct MockPublisher {
    published: Mutex<Vec<(String, String)>>, // (topic, key)
}

#[async_trait]
impl EventPublisher for MockPublisher {
    async fn publish(&self, topic: &str, key: &str, _payload: &[u8]) -> anyhow::Result<()> {
        self.published.lock().await.push((topic.to_string(), key.to_string()));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Test app factory
// ---------------------------------------------------------------------------

struct TestApp {
    router: axum::Router,
    tenant_id: TenantId,
    /// Shared publisher so individual tests can inspect published events.
    publisher: Arc<MockPublisher>,
}

impl TestApp {
    fn new() -> Self {
        let repo      = Arc::new(MockCampaignRepo::default());
        let publisher = Arc::new(MockPublisher::default());
        let svc       = Arc::new(CampaignService::new(repo, publisher.clone()));
        let state     = AppState { campaign_svc: svc };
        let router    = http::router().with_state(state);
        Self {
            router,
            tenant_id: TenantId::new(),
            publisher,
        }
    }

    /// Issue a one-shot HTTP request to the in-process router.
    async fn request(&self, req: Request<Body>) -> axum::response::Response {
        self.router.clone().oneshot(req).await.unwrap()
    }

    /// Build a JSON POST request with a fake JWT claim header that the
    /// middleware accepts in test mode.
    fn post_json(&self, uri: &str, body: Value) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(uri)
            .header("Content-Type", "application/json")
            .header("X-Test-Tenant-Id", self.tenant_id.inner().to_string())
            .header("X-Test-User-Id", Uuid::new_v4().to_string())
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap()
    }

    fn get(&self, uri: &str) -> Request<Body> {
        Request::builder()
            .method("GET")
            .uri(uri)
            .header("X-Test-Tenant-Id", self.tenant_id.inner().to_string())
            .header("X-Test-User-Id", Uuid::new_v4().to_string())
            .body(Body::empty())
            .unwrap()
    }
}

/// Read the full response body and parse it as JSON.
async fn body_json(resp: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

/// Minimal campaign payload for POST /v1/campaigns.
fn campaign_payload(name: &str) -> Value {
    json!({
        "name": name,
        "description": null,
        "channel": "whatsapp",
        "template": {
            "template_id": "pickup_confirmation_v1",
            "subject": null,
            "variables": { "promo_code": "REBOOK10" }
        },
        "targeting": {
            "min_clv_score": null,
            "last_active_days": null,
            "customer_ids": [],
            "estimated_reach": 500
        }
    })
}

// ---------------------------------------------------------------------------
// POST /v1/campaigns
// ---------------------------------------------------------------------------

mod create_campaign {
    use super::*;

    #[tokio::test]
    async fn returns_201_with_campaign_id() {
        let app = TestApp::new();
        let req = app.post_json("/v1/campaigns", campaign_payload("Summer Promo"));
        let resp = app.request(req).await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        let body = body_json(resp).await;
        assert!(body["id"].is_string(), "response must contain a campaign id");
    }

    #[tokio::test]
    async fn new_campaign_has_draft_status() {
        let app = TestApp::new();
        let req = app.post_json("/v1/campaigns", campaign_payload("Draft Test"));
        let resp = app.request(req).await;
        let body = body_json(resp).await;
        assert_eq!(body["status"], "draft");
    }

    #[tokio::test]
    async fn created_campaign_stores_name() {
        let app = TestApp::new();
        let req = app.post_json("/v1/campaigns", campaign_payload("Loyalty Push"));
        let resp = app.request(req).await;
        let body = body_json(resp).await;
        assert_eq!(body["name"], "Loyalty Push");
    }

    #[tokio::test]
    async fn created_campaign_stores_channel() {
        let app = TestApp::new();
        let req = app.post_json("/v1/campaigns", campaign_payload("Channel Test"));
        let resp = app.request(req).await;
        let body = body_json(resp).await;
        assert_eq!(body["channel"], "whatsapp");
    }
}

// ---------------------------------------------------------------------------
// GET /v1/campaigns/:id
// ---------------------------------------------------------------------------

mod get_campaign {
    use super::*;

    #[tokio::test]
    async fn returns_200_with_campaign_data_when_found() {
        let app = TestApp::new();

        // Create first.
        let create_req = app.post_json("/v1/campaigns", campaign_payload("Existing"));
        let create_resp = app.request(create_req).await;
        assert_eq!(create_resp.status(), StatusCode::CREATED);
        let created = body_json(create_resp).await;
        let id = created["id"].as_str().unwrap();

        // Fetch.
        let get_req = app.get(&format!("/v1/campaigns/{}", id));
        let get_resp = app.request(get_req).await;
        assert_eq!(get_resp.status(), StatusCode::OK);
        let fetched = body_json(get_resp).await;
        assert_eq!(fetched["id"], json!(id));
        assert_eq!(fetched["name"], "Existing");
    }

    #[tokio::test]
    async fn returns_404_for_unknown_campaign_id() {
        let app = TestApp::new();
        let random_id = Uuid::new_v4();
        let req = app.get(&format!("/v1/campaigns/{}", random_id));
        let resp = app.request(req).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}

// ---------------------------------------------------------------------------
// POST /v1/campaigns/:id/schedule
// ---------------------------------------------------------------------------

mod schedule_campaign {
    use super::*;

    async fn create_and_get_id(app: &TestApp) -> String {
        let req = app.post_json("/v1/campaigns", campaign_payload("Sched Test"));
        let resp = app.request(req).await;
        body_json(resp).await["id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    #[tokio::test]
    async fn scheduling_draft_campaign_returns_200_and_scheduled_status() {
        let app = TestApp::new();
        let id = create_and_get_id(&app).await;

        let payload = json!({
            "scheduled_at": (Utc::now() + Duration::hours(3)).to_rfc3339()
        });
        let req = app.post_json(&format!("/v1/campaigns/{}/schedule", id), payload);
        let resp = app.request(req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(body["status"], "scheduled");
    }

    #[tokio::test]
    async fn scheduling_already_scheduled_campaign_returns_422() {
        let app = TestApp::new();
        let id = create_and_get_id(&app).await;

        // Schedule once.
        let payload = json!({ "scheduled_at": (Utc::now() + Duration::hours(1)).to_rfc3339() });
        let req1 = app.post_json(&format!("/v1/campaigns/{}/schedule", id), payload.clone());
        app.request(req1).await;

        // Try to schedule again — already Scheduled, not Draft.
        let req2 = app.post_json(&format!("/v1/campaigns/{}/schedule", id), payload);
        let resp = app.request(req2).await;
        // BusinessRule error maps to 422 UNPROCESSABLE_ENTITY.
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn scheduling_sending_campaign_returns_conflict() {
        let app = TestApp::new();
        let id = create_and_get_id(&app).await;

        // Activate first (Draft → Sending).
        let activate_req = app.post_json(&format!("/v1/campaigns/{}/activate", id), json!({}));
        let r = app.request(activate_req).await;
        assert_eq!(r.status(), StatusCode::OK);

        // Now try to schedule — must fail.
        let payload = json!({ "scheduled_at": (Utc::now() + Duration::hours(1)).to_rfc3339() });
        let req = app.post_json(&format!("/v1/campaigns/{}/schedule", id), payload);
        let resp = app.request(req).await;
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}

// ---------------------------------------------------------------------------
// GET /v1/campaigns?status=draft  (list filtering)
// ---------------------------------------------------------------------------

mod list_campaigns {
    use super::*;

    #[tokio::test]
    async fn list_returns_all_campaigns_for_tenant() {
        let app = TestApp::new();

        // Create two campaigns.
        app.request(app.post_json("/v1/campaigns", campaign_payload("Camp A"))).await;
        app.request(app.post_json("/v1/campaigns", campaign_payload("Camp B"))).await;

        let req = app.get("/v1/campaigns");
        let resp = app.request(req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        let count = body["count"].as_u64().unwrap();
        assert!(count >= 2, "should list at least 2 campaigns");
    }

    #[tokio::test]
    async fn empty_tenant_returns_empty_list() {
        let app = TestApp::new();
        let req = app.get("/v1/campaigns");
        let resp = app.request(req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(body["count"], 0);
    }

    #[tokio::test]
    async fn pagination_limit_is_respected() {
        let app = TestApp::new();
        for i in 0..5 {
            app.request(
                app.post_json("/v1/campaigns", campaign_payload(&format!("Camp {}", i)))
            ).await;
        }

        let req = app.get("/v1/campaigns?limit=2&offset=0");
        let resp = app.request(req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        let campaigns = body["campaigns"].as_array().unwrap();
        assert_eq!(campaigns.len(), 2);
    }
}

// ---------------------------------------------------------------------------
// POST /v1/campaigns/:id/cancel
// ---------------------------------------------------------------------------

mod cancel_campaign {
    use super::*;

    async fn create_id(app: &TestApp) -> String {
        let req = app.post_json("/v1/campaigns", campaign_payload("Cancel Test"));
        body_json(app.request(req).await).await["id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    #[tokio::test]
    async fn cancelling_draft_returns_200_and_cancelled_status() {
        let app = TestApp::new();
        let id = create_id(&app).await;

        let req = app.post_json(&format!("/v1/campaigns/{}/cancel", id), json!({}));
        let resp = app.request(req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(body["status"], "cancelled");
    }

    #[tokio::test]
    async fn cancelling_completed_campaign_returns_conflict() {
        let app = TestApp::new();
        let id = create_id(&app).await;

        // Activate then complete.
        app.request(
            app.post_json(&format!("/v1/campaigns/{}/activate", id), json!({}))
        ).await;

        // Mark it completed by directly calling the service — or simulate via a
        // completed fixture by activating and then forcing complete through
        // the cancel path. Instead we test via the domain: after activating the
        // campaign is Sending; cancelling a Sending campaign should fail.
        let req = app.post_json(&format!("/v1/campaigns/{}/cancel", id), json!({}));
        let resp = app.request(req).await;
        // Sending → cancel is a BusinessRule violation → 422
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}

// ---------------------------------------------------------------------------
// Activate → Kafka event is published
// ---------------------------------------------------------------------------

mod activate_publishes_event {
    use super::*;
    use logisticos_events::topics;

    #[tokio::test]
    async fn activating_campaign_publishes_campaign_triggered_event() {
        let app = TestApp::new();

        // Create.
        let req = app.post_json("/v1/campaigns", campaign_payload("Event Test"));
        let resp = app.request(req).await;
        let id = body_json(resp).await["id"]
            .as_str()
            .unwrap()
            .to_string();

        // Activate.
        let req = app.post_json(&format!("/v1/campaigns/{}/activate", id), json!({}));
        let resp = app.request(req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        // Inspect the mock publisher.
        let published = app.publisher.published.lock().await;
        assert!(
            published.iter().any(|(topic, _key)| topic == topics::CAMPAIGN_TRIGGERED),
            "CAMPAIGN_TRIGGERED event must be published on activation"
        );
    }
}
