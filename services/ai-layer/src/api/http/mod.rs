/// HTTP API for the Agentic runtime.
///
/// Endpoints:
///   POST /v1/agents/run          — Trigger an on-demand agent with a natural language prompt
///   GET  /v1/agents/sessions     — List agent sessions for a tenant
///   GET  /v1/agents/sessions/:id — Get a specific session (full message history)
///   GET  /v1/agents/escalated    — List sessions awaiting human review
///   POST /v1/agents/sessions/:id/resolve — Human resolves an escalated session
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use uuid::Uuid;

use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::rbac::permissions;
use logisticos_errors::AppError;

use crate::domain::entities::AgentType;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/agents/run",                       post(run_agent))
        .route("/v1/agents/sessions",                  get(list_sessions))
        .route("/v1/agents/sessions/escalated",        get(list_escalated))
        .route("/v1/agents/sessions/:id",              get(get_session))
        .route("/v1/agents/sessions/:id/resolve",      post(resolve_escalation))
        // Internal endpoint — called by the Python LangGraph sidecar via MCPBridge.
        // Not exposed through the API gateway (protected by Istio network policy).
        .route("/internal/tools/execute",              post(execute_tool))
        .route("/internal/tools",                      get(list_tools))
}

// ---------------------------------------------------------------------------
// POST /v1/agents/run — trigger on-demand agent
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RunAgentRequest {
    /// Natural language task description.
    prompt: String,
    /// Optional context data to include in the trigger.
    context: Option<serde_json::Value>,
}

async fn run_agent(
    State(state): State<AppState>,
    claims: AuthClaims,
    Json(req): Json<RunAgentRequest>,
) -> impl IntoResponse {
    // Any authenticated user can trigger an on-demand agent.
    // Specific agents (Dispatch, Recovery) are triggered automatically.
    let trigger = req.context.unwrap_or(serde_json::json!({"tenant_id": claims.tenant_id.inner()}));

    let session = state
        .runner
        .run(
            claims.tenant_id.clone(),
            AgentType::OnDemand,
            trigger,
            req.prompt,
        )
        .await?;

    Ok::<_, AppError>((
        StatusCode::OK,
        Json(serde_json::json!({
            "session_id":  session.id,
            "status":      session.status,
            "outcome":     session.outcome,
            "escalated":   session.status == crate::domain::entities::SessionStatus::HumanEscalated,
            "actions_taken": session.actions.len(),
            "confidence":  session.confidence_score,
        })),
    ))
}

// ---------------------------------------------------------------------------
// GET /v1/agents/sessions
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ListQuery { limit: Option<i64>, offset: Option<i64> }

async fn list_sessions(
    State(state): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<ListQuery>,
) -> impl IntoResponse {
    let sessions = state
        .session_repo
        .list_by_tenant(
            claims.tenant_id.inner(),
            q.limit.unwrap_or(50).clamp(1, 200),
            q.offset.unwrap_or(0).max(0),
        )
        .await
        .map_err(AppError::internal)?;

    // Return summary (no full message history for list view).
    let summaries: Vec<_> = sessions.iter().map(|s| serde_json::json!({
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

    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"sessions": summaries, "count": summaries.len()}))))
}

// ---------------------------------------------------------------------------
// GET /v1/agents/sessions/escalated
// ---------------------------------------------------------------------------

async fn list_escalated(
    State(state): State<AppState>,
    claims: AuthClaims,
) -> impl IntoResponse {
    let sessions = state
        .session_repo
        .list_escalated(claims.tenant_id.inner())
        .await
        .map_err(AppError::internal)?;

    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"escalated": sessions, "count": sessions.len()}))))
}

// ---------------------------------------------------------------------------
// GET /v1/agents/sessions/:id
// ---------------------------------------------------------------------------

async fn get_session(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let session = state
        .session_repo
        .find_by_id(id)
        .await
        .map_err(AppError::internal)?
        .ok_or_else(|| AppError::NotFound("Agent session not found".into()))?;

    // Tenant isolation.
    if session.tenant_id != claims.tenant_id {
        return Err(AppError::Forbidden("Access denied".into()));
    }

    Ok::<_, AppError>((StatusCode::OK, Json(session)))
}

// ---------------------------------------------------------------------------
// POST /v1/agents/sessions/:id/resolve — human resolves escalation
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ResolveRequest {
    resolution_notes: String,
}

async fn resolve_escalation(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(body): Json<ResolveRequest>,
) -> impl IntoResponse {
    let mut session = state
        .session_repo
        .find_by_id(id)
        .await
        .map_err(AppError::internal)?
        .ok_or_else(|| AppError::NotFound("Agent session not found".into()))?;

    if session.tenant_id != claims.tenant_id {
        return Err(AppError::Forbidden("Access denied".into()));
    }

    if session.status != crate::domain::entities::SessionStatus::HumanEscalated {
        return Err(AppError::BusinessRule("Session is not awaiting human resolution".into()));
    }

    session.complete(
        format!("Resolved by human ({}): {}", claims.user_id, body.resolution_notes),
        1.0,
    );
    state
        .session_repo
        .save(&session)
        .await
        .map_err(AppError::internal)?;

    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"resolved": true, "session_id": id}))))
}

// ---------------------------------------------------------------------------
// POST /internal/tools/execute — Python sidecar bridge
// Called by MCPBridge in the Python LangGraph agent sidecar.
// Not exposed through the API gateway.
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize)]
struct ExecuteToolRequest {
    tool_name:   String,
    input:       serde_json::Value,
    tenant_id:   String,
    session_id:  String,
    tool_use_id: String,
}

async fn execute_tool(
    State(state): State<AppState>,
    Json(req): Json<ExecuteToolRequest>,
) -> impl IntoResponse {
    let result = state
        .tools
        .execute(&req.tool_name, req.input, req.tool_use_id.clone())
        .await;

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "tool_use_id": result.tool_use_id,
            "content":     result.content,
            "is_error":    result.is_error,
        })),
    )
}

// ---------------------------------------------------------------------------
// GET /internal/tools — list all registered tool definitions
// ---------------------------------------------------------------------------

async fn list_tools(State(state): State<AppState>) -> impl IntoResponse {
    let defs: Vec<_> = state.tools.definitions().iter().map(|d| serde_json::json!({
        "name":         d.name,
        "description":  d.description,
        "input_schema": d.input_schema,
    })).collect();

    (StatusCode::OK, Json(serde_json::json!({"tools": defs, "count": defs.len()})))
}
