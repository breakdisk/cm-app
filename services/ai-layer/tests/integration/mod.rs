// Integration tests for the ai-layer HTTP API.
//
// Strategy: no real Claude API, database, or Kafka is needed.
// `MockSessionRepository` stores sessions in memory. `MockAgentRunner`
// returns a canned successful session without contacting Claude.
// The test router re-uses the real URL paths and enforces the real JWT
// middleware so auth, routing, and serialisation are all exercised.

use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
    Router,
};
use serde_json::{json, Value};
use tokio::sync::Mutex;
use tower::ServiceExt; // for `.oneshot()`
use uuid::Uuid;

use logisticos_auth::{claims::Claims, jwt::JwtService};
use logisticos_errors::{AppError, AppResult};
use logisticos_types::TenantId;

use logisticos_ai_layer::domain::entities::{
    AgentAction, AgentMessage, AgentSession, AgentType, MessageRole, SessionStatus,
};
use logisticos_ai_layer::infrastructure::db::SessionRepository;

// ─────────────────────────────────────────────────────────────────────────────
// In-memory mock repository
// ─────────────────────────────────────────────────────────────────────────────

/// Thread-safe in-memory store for `AgentSession` objects.
#[derive(Default)]
struct MockSessionRepository {
    store: Mutex<Vec<AgentSession>>,
}

impl MockSessionRepository {
    fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Seed the store with pre-built sessions for read-path tests.
    async fn with_sessions(sessions: Vec<AgentSession>) -> Arc<Self> {
        let repo = Arc::new(Self::default());
        let mut s = repo.store.lock().await;
        s.extend(sessions);
        drop(s);
        repo
    }
}

#[async_trait]
impl SessionRepository for MockSessionRepository {
    async fn save(&self, session: &AgentSession) -> anyhow::Result<()> {
        let mut store = self.store.lock().await;
        // Upsert by id
        if let Some(pos) = store.iter().position(|s| s.id == session.id) {
            store[pos] = session.clone();
        } else {
            store.push(session.clone());
        }
        Ok(())
    }

    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<AgentSession>> {
        let store = self.store.lock().await;
        Ok(store.iter().find(|s| s.id == id).cloned())
    }

    async fn list_by_tenant(
        &self,
        tenant_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<AgentSession>> {
        let store = self.store.lock().await;
        let results: Vec<AgentSession> = store
            .iter()
            .filter(|s| s.tenant_id.inner() == tenant_id)
            .cloned()
            .skip(offset as usize)
            .take(limit as usize)
            .collect();
        Ok(results)
    }

    async fn list_escalated(&self, tenant_id: Uuid) -> anyhow::Result<Vec<AgentSession>> {
        let store = self.store.lock().await;
        Ok(store
            .iter()
            .filter(|s| {
                s.tenant_id.inner() == tenant_id
                    && s.status == SessionStatus::HumanEscalated
            })
            .cloned()
            .collect())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Mock AgentRunner
// ─────────────────────────────────────────────────────────────────────────────

/// Returns a pre-built completed session without calling Claude.
struct MockAgentRunner {
    repo: Arc<dyn SessionRepository>,
}

impl MockAgentRunner {
    fn new(repo: Arc<dyn SessionRepository>) -> Arc<Self> {
        Arc::new(Self { repo })
    }

    async fn run(
        &self,
        tenant_id: TenantId,
        agent_type: AgentType,
        trigger: Value,
        _prompt: String,
    ) -> AppResult<AgentSession> {
        let mut session = AgentSession::new(tenant_id, agent_type, trigger);
        // Add a sample action to verify actions_taken is populated
        let action = AgentAction::new(
            session.id,
            "get_available_drivers".into(),
            json!({"zone": "Quezon City"}),
        );
        session.actions.push(action);
        session.complete("Driver assigned successfully.".into(), 0.95);
        self.repo.save(&session).await.map_err(|e| AppError::Internal(e.into()))?;
        Ok(session)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Test app wiring
// ─────────────────────────────────────────────────────────────────────────────

const TEST_JWT_SECRET: &str = "test-ai-layer-secret-32-bytes!!!";

/// Mints a valid JWT for the given tenant and permissions.
fn mint_jwt(tenant_id: Uuid, permissions: Vec<&str>) -> String {
    let jwt = JwtService::new(TEST_JWT_SECRET, 3600, 86400);
    let claims = Claims::new(
        Uuid::new_v4(),
        tenant_id,
        "test-tenant".into(),
        "business".into(),
        "test@example.com".into(),
        vec!["admin".into()],
        permissions.into_iter().map(String::from).collect(),
        3600,
    );
    jwt.issue_access_token(claims).expect("JWT minting failed in test")
}

/// Builds the test Axum router, replicating the real endpoint paths from
/// `services/ai-layer/src/api/http/mod.rs`.
fn build_test_router(
    runner: Arc<MockAgentRunner>,
    session_repo: Arc<dyn SessionRepository>,
) -> Router {
    use axum::{
        extract::{Path, Query, State},
        http::StatusCode,
        response::{IntoResponse, Json},
        routing::{get, post},
    };
    use logisticos_auth::middleware::{require_auth, AuthClaims, AuthState};
    use serde::Deserialize;

    #[derive(Clone)]
    struct TestState {
        runner:       Arc<MockAgentRunner>,
        session_repo: Arc<dyn SessionRepository>,
    }

    // POST /v1/agents/run
    #[derive(Debug, Deserialize)]
    struct RunAgentRequest {
        prompt:  String,
        context: Option<Value>,
    }

    async fn run_agent(
        State(state): State<TestState>,
        claims: AuthClaims,
        Json(req): Json<RunAgentRequest>,
    ) -> impl IntoResponse {
        let trigger = req.context.unwrap_or(json!({"tenant_id": claims.tenant_id.to_string()}));
        let session = state
            .runner
            .run(TenantId::from_uuid(claims.tenant_id), AgentType::OnDemand, trigger, req.prompt)
            .await?;

        Ok::<_, AppError>((
            StatusCode::OK,
            Json(json!({
                "session_id":    session.id,
                "status":        session.status,
                "outcome":       session.outcome,
                "escalated":     session.status == SessionStatus::HumanEscalated,
                "actions_taken": session.actions.len(),
                "confidence":    session.confidence_score,
            })),
        ))
    }

    // GET /v1/agents/sessions
    #[derive(Debug, Deserialize)]
    struct ListQuery { limit: Option<i64>, offset: Option<i64> }

    async fn list_sessions(
        State(state): State<TestState>,
        claims: AuthClaims,
        Query(q): Query<ListQuery>,
    ) -> impl IntoResponse {
        let sessions = state
            .session_repo
            .list_by_tenant(
                claims.tenant_id,
                q.limit.unwrap_or(50).clamp(1, 200),
                q.offset.unwrap_or(0).max(0),
            )
            .await
            .map_err(|e| AppError::Internal(e.into()))?;

        let summaries: Vec<_> = sessions.iter().map(|s| json!({
            "id":               s.id,
            "agent_type":       s.agent_type,
            "status":           s.status,
            "outcome":          s.outcome,
            "escalation_reason": s.escalation_reason,
            "confidence_score": s.confidence_score,
            "actions_taken":    s.actions.len(),
            "started_at":       s.started_at,
            "completed_at":     s.completed_at,
        })).collect();

        Ok::<_, AppError>((
            StatusCode::OK,
            Json(json!({"sessions": summaries, "count": summaries.len()})),
        ))
    }

    // GET /v1/agents/sessions/escalated
    async fn list_escalated(
        State(state): State<TestState>,
        claims: AuthClaims,
    ) -> impl IntoResponse {
        let sessions = state
            .session_repo
            .list_escalated(claims.tenant_id)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;

        Ok::<_, AppError>((
            StatusCode::OK,
            Json(json!({"escalated": sessions, "count": sessions.len()})),
        ))
    }

    // GET /v1/agents/sessions/:id
    async fn get_session(
        State(state): State<TestState>,
        claims: AuthClaims,
        Path(id): Path<Uuid>,
    ) -> impl IntoResponse {
        let session = state
            .session_repo
            .find_by_id(id)
            .await
            .map_err(|e| AppError::Internal(e.into()))?
            .ok_or_else(|| AppError::NotFound { resource: "agent_session", id: id.to_string() })?;

        if session.tenant_id.inner() != claims.tenant_id {
            return Err(AppError::Forbidden { resource: "agent_session".into() });
        }

        Ok::<_, AppError>((StatusCode::OK, Json(session)))
    }

    // POST /v1/agents/sessions/:id/resolve
    #[derive(Debug, Deserialize)]
    struct ResolveRequest { resolution_notes: String }

    async fn resolve_escalation(
        State(state): State<TestState>,
        claims: AuthClaims,
        Path(id): Path<Uuid>,
        Json(body): Json<ResolveRequest>,
    ) -> impl IntoResponse {
        let mut session = state
            .session_repo
            .find_by_id(id)
            .await
            .map_err(|e| AppError::Internal(e.into()))?
            .ok_or_else(|| AppError::NotFound { resource: "agent_session", id: id.to_string() })?;

        if session.tenant_id.inner() != claims.tenant_id {
            return Err(AppError::Forbidden { resource: "agent_session".into() });
        }

        if session.status != SessionStatus::HumanEscalated {
            return Err(AppError::BusinessRule(
                "Session is not awaiting human resolution".into(),
            ));
        }

        session.complete(
            format!("Resolved by human: {}", body.resolution_notes),
            1.0,
        );
        state
            .session_repo
            .save(&session)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;

        Ok::<_, AppError>((
            StatusCode::OK,
            Json(json!({"resolved": true, "session_id": id})),
        ))
    }

    let jwt_svc = Arc::new(JwtService::new(TEST_JWT_SECRET, 3600, 86400));
    let auth_layer = axum::middleware::from_fn_with_state(
        Arc::clone(&jwt_svc) as AuthState,
        require_auth,
    );

    let state = TestState { runner, session_repo };

    Router::new()
        .route("/v1/agents/run",                      post(run_agent))
        .route("/v1/agents/sessions",                 get(list_sessions))
        .route("/v1/agents/sessions/escalated",       get(list_escalated))
        .route("/v1/agents/sessions/:id",             get(get_session))
        .route("/v1/agents/sessions/:id/resolve",     post(resolve_escalation))
        .layer(auth_layer)
        .with_state(state)
}

// ─────────────────────────────────────────────────────────────────────────────
// HTTP request helper
// ─────────────────────────────────────────────────────────────────────────────

async fn send_get(app: Router, uri: &str, token: &str) -> (StatusCode, Value) {
    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(uri)
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

async fn send_post(app: Router, uri: &str, token: &str, body: Value) -> (StatusCode, Value) {
    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(uri)
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

async fn send_post_no_auth(app: Router, uri: &str, body: Value) -> StatusCode {
    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(uri)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    resp.status()
}

// ─────────────────────────────────────────────────────────────────────────────
// Fixtures
// ─────────────────────────────────────────────────────────────────────────────

fn completed_session(tenant_id: TenantId) -> AgentSession {
    let mut s = AgentSession::new(tenant_id, AgentType::Dispatch, json!({}));
    s.complete("Route optimised.".into(), 0.92);
    s
}

fn escalated_session(tenant_id: TenantId) -> AgentSession {
    let mut s = AgentSession::new(tenant_id, AgentType::Recovery, json!({}));
    s.escalate("Shipment failed 3 times consecutively".into());
    s
}

fn failed_session(tenant_id: TenantId) -> AgentSession {
    let mut s = AgentSession::new(tenant_id, AgentType::Anomaly, json!({}));
    s.fail("External service timeout".into());
    s
}

// ─────────────────────────────────────────────────────────────────────────────
// Test suite — POST /v1/agents/run
// ─────────────────────────────────────────────────────────────────────────────

mod run_agent_endpoint {
    use super::*;

    #[tokio::test]
    async fn returns_200_with_session_fields_on_success() {
        let tenant_id = Uuid::new_v4();
        let repo = MockSessionRepository::new();
        let runner = MockAgentRunner::new(Arc::clone(&repo) as Arc<dyn SessionRepository>);
        let app = build_test_router(runner, repo as Arc<dyn SessionRepository>);
        let token = mint_jwt(tenant_id, vec!["*"]);

        let (status, body) = send_post(
            app,
            "/v1/agents/run",
            &token,
            json!({"prompt": "Find available drivers in Quezon City"}),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert!(body["session_id"].is_string(), "session_id must be present");
        assert_eq!(body["status"], "completed");
        assert_eq!(body["outcome"], "Driver assigned successfully.");
        assert_eq!(body["escalated"], false);
        assert_eq!(body["actions_taken"], 1);
        assert!(body["confidence"].as_f64().unwrap() > 0.0);
    }

    #[tokio::test]
    async fn returns_200_session_is_persisted_in_repository() {
        let tenant_id = Uuid::new_v4();
        let repo = MockSessionRepository::new();
        let repo_clone = Arc::clone(&repo);
        let runner = MockAgentRunner::new(Arc::clone(&repo) as Arc<dyn SessionRepository>);
        let app = build_test_router(runner, repo as Arc<dyn SessionRepository>);
        let token = mint_jwt(tenant_id, vec!["*"]);

        let (status, body) = send_post(
            app,
            "/v1/agents/run",
            &token,
            json!({"prompt": "Assign driver for shipment S123"}),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let session_id: Uuid = body["session_id"].as_str().unwrap().parse().unwrap();
        let stored = repo_clone.find_by_id(session_id).await.unwrap();
        assert!(stored.is_some(), "Session should be persisted after run");
    }

    #[tokio::test]
    async fn returns_200_with_context_forwarded_as_trigger() {
        let tenant_id = Uuid::new_v4();
        let repo = MockSessionRepository::new();
        let runner = MockAgentRunner::new(Arc::clone(&repo) as Arc<dyn SessionRepository>);
        let app = build_test_router(runner, repo as Arc<dyn SessionRepository>);
        let token = mint_jwt(tenant_id, vec!["*"]);

        let (status, _body) = send_post(
            app,
            "/v1/agents/run",
            &token,
            json!({
                "prompt": "Reconcile COD for today",
                "context": {"shipment_id": "SHP-001", "zone": "BGC"}
            }),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn returns_401_without_auth_token() {
        let repo = MockSessionRepository::new();
        let runner = MockAgentRunner::new(Arc::clone(&repo) as Arc<dyn SessionRepository>);
        let app = build_test_router(runner, repo as Arc<dyn SessionRepository>);

        let status = send_post_no_auth(
            app,
            "/v1/agents/run",
            json!({"prompt": "hello"}),
        )
        .await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn returns_400_when_body_is_not_json() {
        let tenant_id = Uuid::new_v4();
        let repo = MockSessionRepository::new();
        let runner = MockAgentRunner::new(Arc::clone(&repo) as Arc<dyn SessionRepository>);
        let app = build_test_router(runner, repo as Arc<dyn SessionRepository>);
        let token = mint_jwt(tenant_id, vec!["*"]);

        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/v1/agents/run")
                    .header(header::AUTHORIZATION, format!("Bearer {}", token))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(b"this is not json".to_vec()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn returns_400_when_prompt_field_is_missing() {
        let tenant_id = Uuid::new_v4();
        let repo = MockSessionRepository::new();
        let runner = MockAgentRunner::new(Arc::clone(&repo) as Arc<dyn SessionRepository>);
        let app = build_test_router(runner, repo as Arc<dyn SessionRepository>);
        let token = mint_jwt(tenant_id, vec!["*"]);

        // Missing required `prompt` field
        let (status, _) = send_post(
            app,
            "/v1/agents/run",
            &token,
            json!({"context": {"zone": "Makati"}}),
        )
        .await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Test suite — GET /v1/agents/sessions
// ─────────────────────────────────────────────────────────────────────────────

mod list_sessions_endpoint {
    use super::*;

    #[tokio::test]
    async fn returns_200_with_list_for_tenant() {
        let tenant_id = Uuid::new_v4();
        let tid = TenantId::from_uuid(tenant_id);

        let repo = MockSessionRepository::with_sessions(vec![
            completed_session(tid.clone()),
            completed_session(tid.clone()),
        ])
        .await;

        let runner_repo = Arc::clone(&repo);
        let runner = MockAgentRunner::new(runner_repo as Arc<dyn SessionRepository>);
        let app = build_test_router(runner, repo as Arc<dyn SessionRepository>);
        let token = mint_jwt(tenant_id, vec!["*"]);

        let (status, body) = send_get(app, "/v1/agents/sessions", &token).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["count"], 2);
        let sessions = body["sessions"].as_array().unwrap();
        assert_eq!(sessions.len(), 2);
    }

    #[tokio::test]
    async fn returns_empty_list_when_no_sessions() {
        let tenant_id = Uuid::new_v4();
        let repo = MockSessionRepository::new();
        let runner = MockAgentRunner::new(Arc::clone(&repo) as Arc<dyn SessionRepository>);
        let app = build_test_router(runner, repo as Arc<dyn SessionRepository>);
        let token = mint_jwt(tenant_id, vec!["*"]);

        let (status, body) = send_get(app, "/v1/agents/sessions", &token).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["count"], 0);
        let sessions = body["sessions"].as_array().unwrap();
        assert!(sessions.is_empty());
    }

    #[tokio::test]
    async fn returns_401_without_token() {
        let repo = MockSessionRepository::new();
        let runner = MockAgentRunner::new(Arc::clone(&repo) as Arc<dyn SessionRepository>);
        let app = build_test_router(runner, repo as Arc<dyn SessionRepository>);

        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/v1/agents/sessions")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn sessions_from_other_tenants_not_included() {
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        let tid_a = TenantId::from_uuid(tenant_a);
        let tid_b = TenantId::from_uuid(tenant_b);

        let repo = MockSessionRepository::with_sessions(vec![
            completed_session(tid_a.clone()),
            completed_session(tid_b.clone()), // different tenant
        ])
        .await;

        let runner_repo = Arc::clone(&repo);
        let runner = MockAgentRunner::new(runner_repo as Arc<dyn SessionRepository>);
        let app = build_test_router(runner, repo as Arc<dyn SessionRepository>);
        let token = mint_jwt(tenant_a, vec!["*"]);

        let (status, body) = send_get(app, "/v1/agents/sessions", &token).await;

        assert_eq!(status, StatusCode::OK);
        // Tenant A only sees their own session, not Tenant B's.
        assert_eq!(body["count"], 1, "Tenant isolation: only own sessions must be returned");
    }

    #[tokio::test]
    async fn session_summary_includes_expected_fields() {
        let tenant_id = Uuid::new_v4();
        let tid = TenantId::from_uuid(tenant_id);

        let repo = MockSessionRepository::with_sessions(vec![completed_session(tid)]).await;
        let runner_repo = Arc::clone(&repo);
        let runner = MockAgentRunner::new(runner_repo as Arc<dyn SessionRepository>);
        let app = build_test_router(runner, repo as Arc<dyn SessionRepository>);
        let token = mint_jwt(tenant_id, vec!["*"]);

        let (status, body) = send_get(app, "/v1/agents/sessions", &token).await;

        assert_eq!(status, StatusCode::OK);
        let session = &body["sessions"][0];
        assert!(session["id"].is_string());
        assert!(session["agent_type"].is_string());
        assert!(session["status"].is_string());
        assert!(session["started_at"].is_string());
        assert!(session["completed_at"].is_string());
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Test suite — GET /v1/agents/sessions/:id
// ─────────────────────────────────────────────────────────────────────────────

mod get_session_endpoint {
    use super::*;

    #[tokio::test]
    async fn returns_200_with_session_details() {
        let tenant_id = Uuid::new_v4();
        let tid = TenantId::from_uuid(tenant_id);
        let session = completed_session(tid);
        let session_id = session.id;

        let repo = MockSessionRepository::with_sessions(vec![session]).await;
        let runner_repo = Arc::clone(&repo);
        let runner = MockAgentRunner::new(runner_repo as Arc<dyn SessionRepository>);
        let app = build_test_router(runner, repo as Arc<dyn SessionRepository>);
        let token = mint_jwt(tenant_id, vec!["*"]);

        let (status, body) =
            send_get(app, &format!("/v1/agents/sessions/{}", session_id), &token).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["id"].as_str().unwrap(), session_id.to_string());
        assert_eq!(body["status"], "completed");
    }

    #[tokio::test]
    async fn returns_404_when_session_not_found() {
        let tenant_id = Uuid::new_v4();
        let repo = MockSessionRepository::new();
        let runner = MockAgentRunner::new(Arc::clone(&repo) as Arc<dyn SessionRepository>);
        let app = build_test_router(runner, repo as Arc<dyn SessionRepository>);
        let token = mint_jwt(tenant_id, vec!["*"]);

        let nonexistent_id = Uuid::new_v4();
        let (status, body) =
            send_get(app, &format!("/v1/agents/sessions/{}", nonexistent_id), &token).await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn returns_403_when_session_belongs_to_different_tenant() {
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        let tid_b = TenantId::from_uuid(tenant_b);

        // Session owned by tenant B
        let session = completed_session(tid_b);
        let session_id = session.id;

        let repo = MockSessionRepository::with_sessions(vec![session]).await;
        let runner_repo = Arc::clone(&repo);
        let runner = MockAgentRunner::new(runner_repo as Arc<dyn SessionRepository>);
        let app = build_test_router(runner, repo as Arc<dyn SessionRepository>);
        // But tenant A's token
        let token = mint_jwt(tenant_a, vec!["*"]);

        let (status, body) =
            send_get(app, &format!("/v1/agents/sessions/{}", session_id), &token).await;

        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["error"]["code"], "FORBIDDEN");
    }

    #[tokio::test]
    async fn returns_401_without_token() {
        let tenant_id = Uuid::new_v4();
        let tid = TenantId::from_uuid(tenant_id);
        let session = completed_session(tid);
        let session_id = session.id;

        let repo = MockSessionRepository::with_sessions(vec![session]).await;
        let runner = MockAgentRunner::new(Arc::clone(&repo) as Arc<dyn SessionRepository>);
        let app = build_test_router(runner, repo as Arc<dyn SessionRepository>);

        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("/v1/agents/sessions/{}", session_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn session_detail_includes_messages_and_actions() {
        let tenant_id = Uuid::new_v4();
        let tid = TenantId::from_uuid(tenant_id);
        let mut session = AgentSession::new(tid, AgentType::Dispatch, json!({}));
        session.messages.push(AgentMessage {
            role: MessageRole::User,
            content: Value::String("Assign driver".into()),
        });
        let action = AgentAction::new(session.id, "assign_driver".into(), json!({}));
        session.actions.push(action);
        session.complete("Done".into(), 0.9);
        let session_id = session.id;

        let repo = MockSessionRepository::with_sessions(vec![session]).await;
        let runner_repo = Arc::clone(&repo);
        let runner = MockAgentRunner::new(runner_repo as Arc<dyn SessionRepository>);
        let app = build_test_router(runner, repo as Arc<dyn SessionRepository>);
        let token = mint_jwt(tenant_id, vec!["*"]);

        let (status, body) =
            send_get(app, &format!("/v1/agents/sessions/{}", session_id), &token).await;

        assert_eq!(status, StatusCode::OK);
        assert!(body["messages"].is_array(), "messages field must be present");
        assert!(body["actions"].is_array(), "actions field must be present");
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
        assert_eq!(body["actions"].as_array().unwrap().len(), 1);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Test suite — GET /v1/agents/sessions/escalated
// ─────────────────────────────────────────────────────────────────────────────

mod list_escalated_endpoint {
    use super::*;

    #[tokio::test]
    async fn returns_200_with_escalated_sessions_for_tenant() {
        let tenant_id = Uuid::new_v4();
        let tid = TenantId::from_uuid(tenant_id);

        let repo = MockSessionRepository::with_sessions(vec![
            escalated_session(tid.clone()),
            escalated_session(tid.clone()),
            completed_session(tid.clone()), // not escalated — should not appear
        ])
        .await;

        let runner_repo = Arc::clone(&repo);
        let runner = MockAgentRunner::new(runner_repo as Arc<dyn SessionRepository>);
        let app = build_test_router(runner, repo as Arc<dyn SessionRepository>);
        let token = mint_jwt(tenant_id, vec!["*"]);

        let (status, body) =
            send_get(app, "/v1/agents/sessions/escalated", &token).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["count"], 2, "Should only return escalated sessions");
        let escalated = body["escalated"].as_array().unwrap();
        assert_eq!(escalated.len(), 2);
    }

    #[tokio::test]
    async fn returns_empty_when_no_escalated_sessions() {
        let tenant_id = Uuid::new_v4();
        let tid = TenantId::from_uuid(tenant_id);

        let repo = MockSessionRepository::with_sessions(vec![
            completed_session(tid.clone()),
            failed_session(tid.clone()),
        ])
        .await;

        let runner_repo = Arc::clone(&repo);
        let runner = MockAgentRunner::new(runner_repo as Arc<dyn SessionRepository>);
        let app = build_test_router(runner, repo as Arc<dyn SessionRepository>);
        let token = mint_jwt(tenant_id, vec!["*"]);

        let (status, body) =
            send_get(app, "/v1/agents/sessions/escalated", &token).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["count"], 0);
        let escalated = body["escalated"].as_array().unwrap();
        assert!(escalated.is_empty());
    }

    #[tokio::test]
    async fn escalated_sessions_from_other_tenants_not_included() {
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        let tid_a = TenantId::from_uuid(tenant_a);
        let tid_b = TenantId::from_uuid(tenant_b);

        let repo = MockSessionRepository::with_sessions(vec![
            escalated_session(tid_a.clone()),
            escalated_session(tid_b.clone()), // other tenant
        ])
        .await;

        let runner_repo = Arc::clone(&repo);
        let runner = MockAgentRunner::new(runner_repo as Arc<dyn SessionRepository>);
        let app = build_test_router(runner, repo as Arc<dyn SessionRepository>);
        let token = mint_jwt(tenant_a, vec!["*"]);

        let (status, body) =
            send_get(app, "/v1/agents/sessions/escalated", &token).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body["count"], 1,
            "Only escalated sessions for the requesting tenant should be returned"
        );
    }

    #[tokio::test]
    async fn returns_401_without_token() {
        let repo = MockSessionRepository::new();
        let runner = MockAgentRunner::new(Arc::clone(&repo) as Arc<dyn SessionRepository>);
        let app = build_test_router(runner, repo as Arc<dyn SessionRepository>);

        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/v1/agents/sessions/escalated")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Test suite — POST /v1/agents/sessions/:id/resolve
// ─────────────────────────────────────────────────────────────────────────────

mod resolve_escalation_endpoint {
    use super::*;

    #[tokio::test]
    async fn returns_200_when_resolving_escalated_session() {
        let tenant_id = Uuid::new_v4();
        let tid = TenantId::from_uuid(tenant_id);
        let session = escalated_session(tid);
        let session_id = session.id;

        let repo = MockSessionRepository::with_sessions(vec![session]).await;
        let runner_repo = Arc::clone(&repo);
        let runner = MockAgentRunner::new(runner_repo as Arc<dyn SessionRepository>);
        let app = build_test_router(runner, repo as Arc<dyn SessionRepository>);
        let token = mint_jwt(tenant_id, vec!["*"]);

        let (status, body) = send_post(
            app,
            &format!("/v1/agents/sessions/{}/resolve", session_id),
            &token,
            json!({"resolution_notes": "Manually assigned driver DR-007"}),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["resolved"], true);
        assert_eq!(body["session_id"].as_str().unwrap(), session_id.to_string());
    }

    #[tokio::test]
    async fn session_status_becomes_completed_after_resolve() {
        let tenant_id = Uuid::new_v4();
        let tid = TenantId::from_uuid(tenant_id);
        let session = escalated_session(tid);
        let session_id = session.id;

        let repo = MockSessionRepository::with_sessions(vec![session]).await;
        let repo_clone = Arc::clone(&repo);
        let runner_repo = Arc::clone(&repo);
        let runner = MockAgentRunner::new(runner_repo as Arc<dyn SessionRepository>);
        let app = build_test_router(runner, repo as Arc<dyn SessionRepository>);
        let token = mint_jwt(tenant_id, vec!["*"]);

        send_post(
            app,
            &format!("/v1/agents/sessions/{}/resolve", session_id),
            &token,
            json!({"resolution_notes": "Handled manually"}),
        )
        .await;

        // The repository should now have the session in Completed status
        let stored = repo_clone.find_by_id(session_id).await.unwrap().unwrap();
        assert_eq!(stored.status, SessionStatus::Completed);
        assert!(stored.outcome.is_some(), "outcome should be set after resolve");
    }

    #[tokio::test]
    async fn returns_422_when_session_is_not_escalated() {
        let tenant_id = Uuid::new_v4();
        let tid = TenantId::from_uuid(tenant_id);
        let session = completed_session(tid); // already Completed, not escalated
        let session_id = session.id;

        let repo = MockSessionRepository::with_sessions(vec![session]).await;
        let runner_repo = Arc::clone(&repo);
        let runner = MockAgentRunner::new(runner_repo as Arc<dyn SessionRepository>);
        let app = build_test_router(runner, repo as Arc<dyn SessionRepository>);
        let token = mint_jwt(tenant_id, vec!["*"]);

        let (status, body) = send_post(
            app,
            &format!("/v1/agents/sessions/{}/resolve", session_id),
            &token,
            json!({"resolution_notes": "Trying to resolve non-escalated session"}),
        )
        .await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["error"]["code"], "BUSINESS_RULE_VIOLATION");
    }

    #[tokio::test]
    async fn returns_404_when_session_not_found() {
        let tenant_id = Uuid::new_v4();
        let repo = MockSessionRepository::new();
        let runner = MockAgentRunner::new(Arc::clone(&repo) as Arc<dyn SessionRepository>);
        let app = build_test_router(runner, repo as Arc<dyn SessionRepository>);
        let token = mint_jwt(tenant_id, vec!["*"]);

        let nonexistent_id = Uuid::new_v4();
        let (status, _) = send_post(
            app,
            &format!("/v1/agents/sessions/{}/resolve", nonexistent_id),
            &token,
            json!({"resolution_notes": "no-op"}),
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn returns_403_when_session_belongs_to_different_tenant() {
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        let tid_b = TenantId::from_uuid(tenant_b);

        let session = escalated_session(tid_b);
        let session_id = session.id;

        let repo = MockSessionRepository::with_sessions(vec![session]).await;
        let runner_repo = Arc::clone(&repo);
        let runner = MockAgentRunner::new(runner_repo as Arc<dyn SessionRepository>);
        let app = build_test_router(runner, repo as Arc<dyn SessionRepository>);
        let token = mint_jwt(tenant_a, vec!["*"]); // Tenant A token

        let (status, body) = send_post(
            app,
            &format!("/v1/agents/sessions/{}/resolve", session_id),
            &token,
            json!({"resolution_notes": "cross-tenant attempt"}),
        )
        .await;

        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["error"]["code"], "FORBIDDEN");
    }

    #[tokio::test]
    async fn returns_401_without_token() {
        let tenant_id = Uuid::new_v4();
        let tid = TenantId::from_uuid(tenant_id);
        let session = escalated_session(tid);
        let session_id = session.id;

        let repo = MockSessionRepository::with_sessions(vec![session]).await;
        let runner = MockAgentRunner::new(Arc::clone(&repo) as Arc<dyn SessionRepository>);
        let app = build_test_router(runner, repo as Arc<dyn SessionRepository>);

        let status = send_post_no_auth(
            app,
            &format!("/v1/agents/sessions/{}/resolve", session_id),
            json!({"resolution_notes": "no auth"}),
        )
        .await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}
