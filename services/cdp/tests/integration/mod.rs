/// Integration tests for the CDP (Customer Data Platform) service.
///
/// All tests use an in-memory mock repository — no real database is needed.
/// Each test builds its own `Router` via `build_test_app()` so there is zero
/// shared mutable state between test cases.
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
    middleware,
    Router,
};
use chrono::Utc;
use tower::ServiceExt; // `oneshot`
use uuid::Uuid;

use async_trait::async_trait;
use logisticos_auth::{
    claims::Claims,
    jwt::JwtService,
    middleware::require_auth,
    rbac::permissions,
};
use logisticos_types::TenantId;

use logisticos_cdp::{
    api::http::router as cdp_router,
    application::services::ProfileService,
    domain::{
        entities::{BehavioralEvent, CustomerProfile, CustomerId, EventType},
        repositories::{CustomerProfileRepository, ProfileFilter},
    },
    AppState,
};

// ---------------------------------------------------------------------------
// In-memory mock repository
// ---------------------------------------------------------------------------

/// In-memory implementation of `CustomerProfileRepository`.
/// All profiles are stored in a `HashMap<Uuid, CustomerProfile>` keyed by the
/// profile's internal `CustomerId`.
#[derive(Default, Clone)]
struct MockProfileRepo {
    store: Arc<Mutex<HashMap<Uuid, CustomerProfile>>>,
}

#[async_trait]
impl CustomerProfileRepository for MockProfileRepo {
    async fn find_by_id(&self, id: &CustomerId) -> anyhow::Result<Option<CustomerProfile>> {
        let guard = self.store.lock().unwrap();
        Ok(guard.get(&id.inner()).cloned())
    }

    async fn find_by_external_id(
        &self,
        tenant_id: &TenantId,
        external_id: Uuid,
    ) -> anyhow::Result<Option<CustomerProfile>> {
        let guard = self.store.lock().unwrap();
        let profile = guard.values().find(|p| {
            p.tenant_id.inner() == tenant_id.inner() && p.external_customer_id == external_id
        });
        Ok(profile.cloned())
    }

    async fn find_by_email(
        &self,
        tenant_id: &TenantId,
        email: &str,
    ) -> anyhow::Result<Option<CustomerProfile>> {
        let guard = self.store.lock().unwrap();
        let profile = guard.values().find(|p| {
            p.tenant_id.inner() == tenant_id.inner()
                && p.email.as_deref() == Some(email)
        });
        Ok(profile.cloned())
    }

    async fn save(&self, profile: &CustomerProfile) -> anyhow::Result<()> {
        let mut guard = self.store.lock().unwrap();
        guard.insert(profile.id.inner(), profile.clone());
        Ok(())
    }

    async fn list(
        &self,
        tenant_id: &TenantId,
        filter: &ProfileFilter,
    ) -> anyhow::Result<Vec<CustomerProfile>> {
        let guard = self.store.lock().unwrap();
        let mut profiles: Vec<CustomerProfile> = guard
            .values()
            .filter(|p| p.tenant_id.inner() == tenant_id.inner())
            .filter(|p| {
                filter.name_contains.as_ref().map_or(true, |n| {
                    p.name.as_deref().unwrap_or("").to_lowercase().contains(&n.to_lowercase())
                })
            })
            .filter(|p| filter.email.as_ref().map_or(true, |e| p.email.as_deref() == Some(e)))
            .filter(|p| filter.phone.as_ref().map_or(true, |ph| p.phone.as_deref() == Some(ph)))
            .filter(|p| filter.min_clv.map_or(true, |min| p.clv_score >= min))
            .cloned()
            .collect();

        profiles.sort_by(|a, b| b.clv_score.partial_cmp(&a.clv_score).unwrap_or(std::cmp::Ordering::Equal));
        let start = filter.offset as usize;
        let end = (start + filter.limit as usize).min(profiles.len());
        Ok(profiles[start..end].to_vec())
    }

    async fn top_by_clv(
        &self,
        tenant_id: &TenantId,
        limit: i64,
    ) -> anyhow::Result<Vec<CustomerProfile>> {
        let guard = self.store.lock().unwrap();
        let mut profiles: Vec<CustomerProfile> = guard
            .values()
            .filter(|p| p.tenant_id.inner() == tenant_id.inner())
            .cloned()
            .collect();
        profiles.sort_by(|a, b| b.clv_score.partial_cmp(&a.clv_score).unwrap_or(std::cmp::Ordering::Equal));
        profiles.truncate(limit as usize);
        Ok(profiles)
    }

    async fn count(&self, tenant_id: &TenantId) -> anyhow::Result<i64> {
        let guard = self.store.lock().unwrap();
        let count = guard
            .values()
            .filter(|p| p.tenant_id.inner() == tenant_id.inner())
            .count();
        Ok(count as i64)
    }
}

// ---------------------------------------------------------------------------
// Test constants and helpers
// ---------------------------------------------------------------------------

const TEST_JWT_SECRET: &str = "cdp-test-secret-must-be-long-enough-for-hs256";
const TEST_TENANT_ID: Uuid = Uuid::from_u128(0x_dead_beef_0000_0000_0000_0000_0000_0002);

/// Mint a HS256 JWT for the given `user_id` with the given permissions.
fn mint_jwt_token(user_id: Uuid, permissions: Vec<String>) -> String {
    let jwt = JwtService::new(TEST_JWT_SECRET, 3600, 86400);
    let claims = Claims::new(
        user_id,
        TEST_TENANT_ID,
        "test-cdp-tenant".to_owned(),
        "business".to_owned(),
        "ops@logisticos.io".to_owned(),
        vec!["admin".to_owned()],
        permissions,
        3600,
    );
    jwt.issue_access_token(claims)
        .expect("test JWT must be mintable")
}

/// Convenience: a token with full customer read + manage permissions.
fn ops_token() -> String {
    mint_jwt_token(
        Uuid::new_v4(),
        vec![
            permissions::CUSTOMERS_VIEW.to_owned(),
            permissions::CUSTOMERS_MANAGE.to_owned(),
        ],
    )
}

/// Convenience: a read-only token.
fn readonly_token() -> String {
    mint_jwt_token(Uuid::new_v4(), vec![permissions::CUSTOMERS_VIEW.to_owned()])
}

/// Build a test `Router` from the provided mock repository.
/// The test app adds the `require_auth` middleware layer so that `AuthClaims`
/// extractors inside handlers receive the validated claims from request extensions.
fn build_test_app(repo: MockProfileRepo) -> Router {
    let jwt = Arc::new(JwtService::new(TEST_JWT_SECRET, 3600, 86400));

    let profile_svc = Arc::new(ProfileService::new(Arc::new(repo)));
    let state = AppState { profile_svc };

    cdp_router()
        .layer(middleware::from_fn_with_state(
            Arc::clone(&jwt),
            require_auth,
        ))
        .with_state(state)
}

/// Helper: send an HTTP request through the app.
async fn send(app: Router, req: Request<Body>) -> axum::response::Response {
    app.oneshot(req).await.expect("service call must not fail")
}

/// Helper: build a JSON-body authenticated request.
fn json_request(method: Method, uri: &str, body: serde_json::Value, token: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {}", token))
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

/// Helper: build an empty-body authenticated GET request.
fn get_request(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap()
}

/// Deserialise the response body as JSON.
async fn body_json(resp: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("response body must be readable");
    serde_json::from_slice(&bytes).expect("response body must be valid JSON")
}

/// Create a `CustomerProfile` fixture pre-populated in a repo and return
/// both the repo and the profile's `external_customer_id`.
fn repo_with_profile(
    name: Option<&str>,
    email: Option<&str>,
    phone: Option<&str>,
) -> (MockProfileRepo, Uuid) {
    let external_id = Uuid::new_v4();
    let mut profile = CustomerProfile::new(TenantId::from_uuid(TEST_TENANT_ID), external_id);
    profile.enrich_identity(
        name.map(str::to_owned),
        email.map(str::to_owned),
        phone.map(str::to_owned),
    );

    let repo = MockProfileRepo::default();
    repo.store.lock().unwrap().insert(profile.id.inner(), profile);
    (repo, external_id)
}

// ===========================================================================
// Tests: Profile upsert  (PUT /v1/customers/:external_id)
// ===========================================================================

mod profile_upsert {
    use super::*;

    #[tokio::test]
    async fn upsert_creates_new_profile() {
        let external_id = Uuid::new_v4();
        let token = ops_token();
        let app = build_test_app(MockProfileRepo::default());

        let resp = send(
            app,
            json_request(
                Method::PUT,
                &format!("/v1/customers/{external_id}"),
                serde_json::json!({
                    "name":  "Maria Santos",
                    "email": "maria@example.com",
                    "phone": "+639171234567"
                }),
                &token,
            ),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["external_customer_id"], external_id.to_string());
        assert_eq!(json["name"], "Maria Santos");
        assert_eq!(json["email"], "maria@example.com");
        assert_eq!(json["phone"], "+639171234567");
    }

    #[tokio::test]
    async fn upsert_updates_existing_profile_merges_fields() {
        let (repo, external_id) = repo_with_profile(Some("Juan"), Some("juan@example.com"), None);
        let token = ops_token();
        let app = build_test_app(repo.clone());

        // Update only the phone — name and email should be preserved.
        let resp = send(
            app,
            json_request(
                Method::PUT,
                &format!("/v1/customers/{external_id}"),
                serde_json::json!({ "phone": "+639987654321" }),
                &token,
            ),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["name"], "Juan");
        assert_eq!(json["email"], "juan@example.com");
        assert_eq!(json["phone"], "+639987654321");
    }

    #[tokio::test]
    async fn upsert_requires_manage_permission() {
        let external_id = Uuid::new_v4();
        let readonly_tok = readonly_token(); // only CUSTOMERS_VIEW, not CUSTOMERS_MANAGE
        let app = build_test_app(MockProfileRepo::default());

        let resp = send(
            app,
            json_request(
                Method::PUT,
                &format!("/v1/customers/{external_id}"),
                serde_json::json!({ "name": "Should Fail" }),
                &readonly_tok,
            ),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn upsert_without_token_returns_401() {
        let external_id = Uuid::new_v4();
        let app = build_test_app(MockProfileRepo::default());

        let resp = send(
            app,
            Request::builder()
                .method(Method::PUT)
                .uri(&format!("/v1/customers/{external_id}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b"{\"name\":\"x\"}".as_ref()))
                .unwrap(),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn upsert_sets_initial_counters_to_zero_for_new_profile() {
        let external_id = Uuid::new_v4();
        let token = ops_token();
        let app = build_test_app(MockProfileRepo::default());

        let resp = send(
            app,
            json_request(
                Method::PUT,
                &format!("/v1/customers/{external_id}"),
                serde_json::json!({ "name": "New Customer" }),
                &token,
            ),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["total_shipments"], 0);
        assert_eq!(json["successful_deliveries"], 0);
        assert_eq!(json["failed_deliveries"], 0);
        assert_eq!(json["total_cod_collected_cents"], 0);
        assert_eq!(json["clv_score"], 0.0);
    }
}

// ===========================================================================
// Tests: Get profile  (GET /v1/customers/:external_id)
// ===========================================================================

mod get_profile {
    use super::*;

    #[tokio::test]
    async fn returns_full_profile_by_external_id() {
        let (repo, external_id) =
            repo_with_profile(Some("Ana Reyes"), Some("ana@example.com"), Some("+639211111111"));
        let token = ops_token();
        let app = build_test_app(repo);

        let resp = send(app, get_request(&format!("/v1/customers/{external_id}"), &token)).await;

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["external_customer_id"], external_id.to_string());
        assert_eq!(json["name"], "Ana Reyes");
        assert_eq!(json["email"], "ana@example.com");
        assert_eq!(json["phone"], "+639211111111");
    }

    #[tokio::test]
    async fn returns_404_when_profile_not_found() {
        let token = ops_token();
        let app = build_test_app(MockProfileRepo::default());
        let fake_id = Uuid::new_v4();

        let resp = send(app, get_request(&format!("/v1/customers/{fake_id}"), &token)).await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn delivery_success_rate_computed_correctly() {
        // 8 successful / 10 total deliveries (8 successful + 2 failed) = 80 %
        let external_id = Uuid::new_v4();
        let mut profile = CustomerProfile::new(TenantId::from_uuid(TEST_TENANT_ID), external_id);

        for _ in 0..10 {
            let event = BehavioralEvent::new(
                EventType::ShipmentCreated,
                None,
                serde_json::Value::Null,
                Utc::now(),
            );
            profile.record_event(event);
        }
        for _ in 0..8 {
            let event = BehavioralEvent::new(
                EventType::DeliveryCompleted,
                None,
                serde_json::Value::Null,
                Utc::now(),
            );
            profile.record_event(event);
        }
        for _ in 0..2 {
            let event = BehavioralEvent::new(
                EventType::DeliveryFailed,
                None,
                serde_json::Value::Null,
                Utc::now(),
            );
            profile.record_event(event);
        }

        let repo = MockProfileRepo::default();
        repo.store.lock().unwrap().insert(profile.id.inner(), profile);
        let token = ops_token();
        let app = build_test_app(repo);

        let resp = send(app, get_request(&format!("/v1/customers/{external_id}"), &token)).await;

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        let rate = json["delivery_success_rate"].as_f64().expect("delivery_success_rate must be a number");
        assert!((rate - 80.0).abs() < 0.01, "Expected 80.0 but got {rate}");
    }

    #[tokio::test]
    async fn returns_401_without_authorization() {
        let app = build_test_app(MockProfileRepo::default());

        let resp = send(
            app,
            Request::builder()
                .method(Method::GET)
                .uri(&format!("/v1/customers/{}", Uuid::new_v4()))
                .body(Body::empty())
                .unwrap(),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}

// ===========================================================================
// Tests: List profiles with query filter  (GET /v1/customers?phone=...)
// ===========================================================================

mod list_profiles {
    use super::*;

    #[tokio::test]
    async fn finds_profile_by_phone() {
        let (repo, _external_id) =
            repo_with_profile(Some("Pedro Cruz"), None, Some("+639171234567"));
        let token = ops_token();
        let app = build_test_app(repo);

        let resp = send(
            app,
            get_request("/v1/customers?phone=%2B639171234567", &token),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        let profiles = json["profiles"].as_array().expect("profiles must be an array");
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0]["phone"], "+639171234567");
    }

    #[tokio::test]
    async fn returns_empty_list_when_phone_not_found() {
        let token = ops_token();
        let app = build_test_app(MockProfileRepo::default());

        let resp = send(
            app,
            get_request("/v1/customers?phone=%2B639000000000", &token),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        let profiles = json["profiles"].as_array().expect("profiles must be an array");
        assert!(profiles.is_empty());
    }

    #[tokio::test]
    async fn finds_profile_by_email() {
        let (repo, external_id) = repo_with_profile(None, Some("find-me@example.com"), None);
        let token = ops_token();
        let app = build_test_app(repo);

        let resp = send(
            app,
            get_request("/v1/customers?email=find-me%40example.com", &token),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        let profiles = json["profiles"].as_array().unwrap();
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0]["external_customer_id"], external_id.to_string());
    }

    #[tokio::test]
    async fn returns_count_matching_results() {
        let repo = MockProfileRepo::default();

        for i in 0..3 {
            let ext_id = Uuid::new_v4();
            let mut p = CustomerProfile::new(TenantId::from_uuid(TEST_TENANT_ID), ext_id);
            p.enrich_identity(Some(format!("Customer {i}")), None, None);
            repo.store.lock().unwrap().insert(p.id.inner(), p);
        }

        let token = ops_token();
        let app = build_test_app(repo);

        let resp = send(app, get_request("/v1/customers", &token)).await;

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["count"], 3);
    }
}

// ===========================================================================
// Tests: Behavioral event recording  (GET /v1/customers/:id/events)
// ===========================================================================

mod behavioral_events {
    use super::*;

    /// Record a sequence of events directly on a profile via the domain model,
    /// then verify the stored counters via the GET profile endpoint.
    #[tokio::test]
    async fn shipment_created_increments_total_shipments() {
        let external_id = Uuid::new_v4();
        let mut profile = CustomerProfile::new(TenantId::from_uuid(TEST_TENANT_ID), external_id);

        profile.record_event(BehavioralEvent::new(
            EventType::ShipmentCreated,
            Some(Uuid::new_v4()),
            serde_json::Value::Null,
            Utc::now(),
        ));

        let repo = MockProfileRepo::default();
        repo.store.lock().unwrap().insert(profile.id.inner(), profile);

        let token = ops_token();
        let app = build_test_app(repo);

        let resp = send(app, get_request(&format!("/v1/customers/{external_id}"), &token)).await;

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["total_shipments"], 1);
    }

    #[tokio::test]
    async fn delivery_completed_increments_successful_deliveries() {
        let external_id = Uuid::new_v4();
        let mut profile = CustomerProfile::new(TenantId::from_uuid(TEST_TENANT_ID), external_id);

        profile.record_event(BehavioralEvent::new(
            EventType::DeliveryCompleted,
            Some(Uuid::new_v4()),
            serde_json::Value::Null,
            Utc::now(),
        ));

        let repo = MockProfileRepo::default();
        repo.store.lock().unwrap().insert(profile.id.inner(), profile);

        let token = ops_token();
        let app = build_test_app(repo);

        let resp = send(app, get_request(&format!("/v1/customers/{external_id}"), &token)).await;

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["successful_deliveries"], 1);
    }

    #[tokio::test]
    async fn delivery_failed_increments_failed_deliveries() {
        let external_id = Uuid::new_v4();
        let mut profile = CustomerProfile::new(TenantId::from_uuid(TEST_TENANT_ID), external_id);

        profile.record_event(BehavioralEvent::new(
            EventType::DeliveryFailed,
            Some(Uuid::new_v4()),
            serde_json::Value::Null,
            Utc::now(),
        ));

        let repo = MockProfileRepo::default();
        repo.store.lock().unwrap().insert(profile.id.inner(), profile);

        let token = ops_token();
        let app = build_test_app(repo);

        let resp = send(app, get_request(&format!("/v1/customers/{external_id}"), &token)).await;

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["failed_deliveries"], 1);
    }

    #[tokio::test]
    async fn cod_paid_increments_total_cod_collected_cents() {
        let external_id = Uuid::new_v4();
        let mut profile = CustomerProfile::new(TenantId::from_uuid(TEST_TENANT_ID), external_id);

        profile.record_event(BehavioralEvent::new(
            EventType::CodPaid,
            Some(Uuid::new_v4()),
            serde_json::json!({ "amount_cents": 50000 }),
            Utc::now(),
        ));

        let repo = MockProfileRepo::default();
        repo.store.lock().unwrap().insert(profile.id.inner(), profile);

        let token = ops_token();
        let app = build_test_app(repo);

        let resp = send(app, get_request(&format!("/v1/customers/{external_id}"), &token)).await;

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["total_cod_collected_cents"], 50000);
    }

    #[tokio::test]
    async fn multiple_cod_events_accumulate_total() {
        let external_id = Uuid::new_v4();
        let mut profile = CustomerProfile::new(TenantId::from_uuid(TEST_TENANT_ID), external_id);

        for amount in [10000_i64, 20000, 30000] {
            profile.record_event(BehavioralEvent::new(
                EventType::CodPaid,
                Some(Uuid::new_v4()),
                serde_json::json!({ "amount_cents": amount }),
                Utc::now(),
            ));
        }

        let repo = MockProfileRepo::default();
        repo.store.lock().unwrap().insert(profile.id.inner(), profile);

        let token = ops_token();
        let app = build_test_app(repo);

        let resp = send(app, get_request(&format!("/v1/customers/{external_id}"), &token)).await;

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["total_cod_collected_cents"], 60000);
    }

    #[tokio::test]
    async fn get_events_endpoint_returns_event_list() {
        let external_id = Uuid::new_v4();
        let mut profile = CustomerProfile::new(TenantId::from_uuid(TEST_TENANT_ID), external_id);

        // Record 3 events.
        for event_type in [
            EventType::ShipmentCreated,
            EventType::DeliveryCompleted,
            EventType::NotificationRead,
        ] {
            profile.record_event(BehavioralEvent::new(
                event_type,
                None,
                serde_json::Value::Null,
                Utc::now(),
            ));
        }

        let repo = MockProfileRepo::default();
        repo.store.lock().unwrap().insert(profile.id.inner(), profile);

        let token = ops_token();
        let app = build_test_app(repo);

        let resp = send(
            app,
            get_request(&format!("/v1/customers/{external_id}/events"), &token),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["external_customer_id"], external_id.to_string());
        let events = json["events"].as_array().expect("events must be an array");
        assert_eq!(events.len(), 3);
        assert_eq!(json["count"], 3);
    }

    #[tokio::test]
    async fn get_events_returns_404_when_profile_not_found() {
        let token = ops_token();
        let app = build_test_app(MockProfileRepo::default());

        let resp = send(
            app,
            get_request(&format!("/v1/customers/{}/events", Uuid::new_v4()), &token),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}

// ===========================================================================
// Tests: CLV score and churn signals
// ===========================================================================

mod clv_and_churn {
    use super::*;

    #[tokio::test]
    async fn new_profile_has_zero_clv_score() {
        let external_id = Uuid::new_v4();
        let profile = CustomerProfile::new(TenantId::from_uuid(TEST_TENANT_ID), external_id);

        let repo = MockProfileRepo::default();
        repo.store.lock().unwrap().insert(profile.id.inner(), profile);

        let token = ops_token();
        let app = build_test_app(repo);

        let resp = send(app, get_request(&format!("/v1/customers/{external_id}"), &token)).await;

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["clv_score"], 0.0);
    }

    #[tokio::test]
    async fn clv_score_positive_after_five_successful_deliveries() {
        let external_id = Uuid::new_v4();
        let mut profile = CustomerProfile::new(TenantId::from_uuid(TEST_TENANT_ID), external_id);

        for _ in 0..5 {
            profile.record_event(BehavioralEvent::new(
                EventType::DeliveryCompleted,
                Some(Uuid::new_v4()),
                serde_json::Value::Null,
                Utc::now(),
            ));
        }

        let repo = MockProfileRepo::default();
        repo.store.lock().unwrap().insert(profile.id.inner(), profile);

        let token = ops_token();
        let app = build_test_app(repo);

        let resp = send(app, get_request(&format!("/v1/customers/{external_id}"), &token)).await;

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        let score = json["clv_score"].as_f64().expect("clv_score must be a number");
        assert!(score > 0.0, "Expected CLV score > 0 after 5 deliveries, got {score}");
    }

    #[tokio::test]
    async fn top_by_clv_returns_profiles_sorted_descending() {
        let repo = MockProfileRepo::default();

        // Create three profiles with increasing CLV.
        for n_deliveries in [1_u32, 5, 15] {
            let ext_id = Uuid::new_v4();
            let mut p = CustomerProfile::new(TenantId::from_uuid(TEST_TENANT_ID), ext_id);
            for _ in 0..n_deliveries {
                p.record_event(BehavioralEvent::new(
                    EventType::DeliveryCompleted,
                    Some(Uuid::new_v4()),
                    serde_json::Value::Null,
                    Utc::now(),
                ));
            }
            repo.store.lock().unwrap().insert(p.id.inner(), p);
        }

        let token = ops_token();
        let app = build_test_app(repo);

        let resp = send(app, get_request("/v1/customers/top-clv?limit=3", &token)).await;

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        let profiles = json["profiles"].as_array().expect("profiles must be an array");
        assert_eq!(profiles.len(), 3);

        // Verify descending order.
        let scores: Vec<f64> = profiles
            .iter()
            .map(|p| p["clv_score"].as_f64().unwrap())
            .collect();
        for i in 0..scores.len() - 1 {
            assert!(
                scores[i] >= scores[i + 1],
                "Scores not sorted descending: {scores:?}"
            );
        }
    }

    #[tokio::test]
    async fn new_profile_has_zero_engagement_score() {
        let external_id = Uuid::new_v4();
        let profile = CustomerProfile::new(TenantId::from_uuid(TEST_TENANT_ID), external_id);

        let repo = MockProfileRepo::default();
        repo.store.lock().unwrap().insert(profile.id.inner(), profile);

        let token = ops_token();
        let app = build_test_app(repo);

        let resp = send(app, get_request(&format!("/v1/customers/{external_id}"), &token)).await;

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["engagement_score"], 0.0);
    }
}

// ===========================================================================
// Tests: Preferred address logic
// ===========================================================================

mod preferred_address {
    use super::*;

    #[tokio::test]
    async fn returns_most_used_address() {
        let external_id = Uuid::new_v4();
        let mut profile = CustomerProfile::new(TenantId::from_uuid(TEST_TENANT_ID), external_id);

        let addr_a = "123 Rizal St, Makati City";
        let addr_b = "456 Bonifacio Ave, BGC";

        // addr_a used 3 times, addr_b used 1 time → addr_a is preferred.
        for _ in 0..3 {
            profile.record_event(BehavioralEvent::new(
                EventType::DeliveryCompleted,
                Some(Uuid::new_v4()),
                serde_json::json!({ "destination_address": addr_a }),
                Utc::now(),
            ));
        }
        profile.record_event(BehavioralEvent::new(
            EventType::DeliveryCompleted,
            Some(Uuid::new_v4()),
            serde_json::json!({ "destination_address": addr_b }),
            Utc::now(),
        ));

        let repo = MockProfileRepo::default();
        repo.store.lock().unwrap().insert(profile.id.inner(), profile);

        let token = ops_token();
        let app = build_test_app(repo);

        let resp = send(app, get_request(&format!("/v1/customers/{external_id}"), &token)).await;

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert_eq!(json["preferred_address"], addr_a);
    }

    #[tokio::test]
    async fn returns_null_preferred_address_when_no_history() {
        let external_id = Uuid::new_v4();
        let profile = CustomerProfile::new(TenantId::from_uuid(TEST_TENANT_ID), external_id);

        let repo = MockProfileRepo::default();
        repo.store.lock().unwrap().insert(profile.id.inner(), profile);

        let token = ops_token();
        let app = build_test_app(repo);

        let resp = send(app, get_request(&format!("/v1/customers/{external_id}"), &token)).await;

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        assert!(json["preferred_address"].is_null());
    }

    #[tokio::test]
    async fn address_history_tracks_use_counts_correctly() {
        let external_id = Uuid::new_v4();
        let mut profile = CustomerProfile::new(TenantId::from_uuid(TEST_TENANT_ID), external_id);

        let addr = "789 Mabini St, Manila";
        for _ in 0..5 {
            profile.record_event(BehavioralEvent::new(
                EventType::ShipmentCreated,
                Some(Uuid::new_v4()),
                serde_json::json!({ "destination_address": addr }),
                Utc::now(),
            ));
        }

        // Verify address_history use_count via the domain model directly.
        assert_eq!(profile.address_history.len(), 1);
        assert_eq!(profile.address_history[0].use_count, 5);
        assert_eq!(profile.address_history[0].address, addr);
        assert_eq!(profile.preferred_address(), Some(addr));
    }
}

// ===========================================================================
// Tests: Delivery success rate edge cases
// ===========================================================================

mod delivery_success_rate {
    use super::*;

    #[tokio::test]
    async fn zero_deliveries_gives_zero_rate() {
        let p = CustomerProfile::new(TenantId::from_uuid(TEST_TENANT_ID), Uuid::new_v4());
        assert_eq!(p.delivery_success_rate(), 0.0);
    }

    #[tokio::test]
    async fn all_successful_gives_100_percent() {
        let mut p = CustomerProfile::new(TenantId::from_uuid(TEST_TENANT_ID), Uuid::new_v4());
        for _ in 0..5 {
            p.record_event(BehavioralEvent::new(
                EventType::DeliveryCompleted,
                None,
                serde_json::Value::Null,
                Utc::now(),
            ));
        }
        assert!((p.delivery_success_rate() - 100.0).abs() < 0.01);
    }

    #[tokio::test]
    async fn all_failed_gives_zero_percent() {
        let mut p = CustomerProfile::new(TenantId::from_uuid(TEST_TENANT_ID), Uuid::new_v4());
        for _ in 0..3 {
            p.record_event(BehavioralEvent::new(
                EventType::DeliveryFailed,
                None,
                serde_json::Value::Null,
                Utc::now(),
            ));
        }
        assert!((p.delivery_success_rate() - 0.0).abs() < 0.01);
    }

    #[tokio::test]
    async fn mixed_deliveries_correct_rate_via_api() {
        let external_id = Uuid::new_v4();
        let mut profile = CustomerProfile::new(TenantId::from_uuid(TEST_TENANT_ID), external_id);

        // 4 successful + 1 failed = 80%.
        for _ in 0..4 {
            profile.record_event(BehavioralEvent::new(
                EventType::DeliveryCompleted,
                None,
                serde_json::Value::Null,
                Utc::now(),
            ));
        }
        profile.record_event(BehavioralEvent::new(
            EventType::DeliveryFailed,
            None,
            serde_json::Value::Null,
            Utc::now(),
        ));

        let repo = MockProfileRepo::default();
        repo.store.lock().unwrap().insert(profile.id.inner(), profile);

        let token = ops_token();
        let app = build_test_app(repo);

        let resp = send(
            app,
            get_request(&format!("/v1/customers/{external_id}"), &token),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        let rate = json["delivery_success_rate"].as_f64().unwrap();
        assert!((rate - 80.0).abs() < 0.01, "Expected 80.0 got {rate}");
    }
}

// ===========================================================================
// Tests: Event timeline window (90-event cap)
// ===========================================================================

mod event_timeline {
    use super::*;

    #[tokio::test]
    async fn recent_events_capped_at_90() {
        let external_id = Uuid::new_v4();
        let mut profile = CustomerProfile::new(TenantId::from_uuid(TEST_TENANT_ID), external_id);

        for _ in 0..95 {
            profile.record_event(BehavioralEvent::new(
                EventType::NotificationRead,
                None,
                serde_json::Value::Null,
                Utc::now(),
            ));
        }

        assert_eq!(
            profile.recent_events.len(),
            90,
            "recent_events must be capped at 90"
        );
    }

    #[tokio::test]
    async fn get_events_reflects_capped_list_via_api() {
        let external_id = Uuid::new_v4();
        let mut profile = CustomerProfile::new(TenantId::from_uuid(TEST_TENANT_ID), external_id);

        for _ in 0..100 {
            profile.record_event(BehavioralEvent::new(
                EventType::ShipmentCreated,
                Some(Uuid::new_v4()),
                serde_json::Value::Null,
                Utc::now(),
            ));
        }

        let repo = MockProfileRepo::default();
        repo.store.lock().unwrap().insert(profile.id.inner(), profile);

        let token = ops_token();
        let app = build_test_app(repo);

        let resp = send(
            app,
            get_request(&format!("/v1/customers/{external_id}/events"), &token),
        )
        .await;

        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp).await;
        let events = json["events"].as_array().unwrap();
        assert_eq!(events.len(), 90);
        assert_eq!(json["count"], 90);
    }
}
