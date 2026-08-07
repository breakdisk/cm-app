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
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use uuid::Uuid;

use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;

use crate::domain::entities::AgentType;
use crate::infrastructure::tools::ToolContext;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/agents/chat",                      post(chat))
        .route("/v1/agents/chat/:id",                  get(get_chat))
        .route("/v1/agents/run",                       post(run_agent))
        .route("/v1/agents/aggregate",                 get(aggregate_stats))
        .route("/v1/agents/sessions",                  get(list_sessions))
        .route("/v1/agents/sessions/escalated",        get(list_escalated))
        .route("/v1/agents/sessions/:id",              get(get_session))
        .route("/v1/agents/sessions/:id/resolve",      post(resolve_escalation))
        // Internal endpoint — called by the Python LangGraph sidecar via MCPBridge.
        // Not exposed through the API gateway (protected by Istio network policy).
        .route("/internal/tools/execute",              post(execute_tool))
        .route("/internal/tools",                      get(list_tools))
}

/// GET /v1/agents/aggregate — KPI counters + 24-hour invocation breakdown
/// for the AI Agents dashboard. One round-trip; reads from ai.agent_sessions
/// directly (no Kafka, no aggregator service needed for now).
async fn aggregate_stats(
    State(state): State<AppState>,
    claims: AuthClaims,
) -> impl IntoResponse {
    if !claims.can_use_ai() {
        return Err(AppError::Forbidden { resource: "ai_features".into() });
    }
    let stats = state.session_repo
        .aggregate(claims.tenant_id)
        .await
        .map_err(AppError::internal)?;
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({ "data": stats }))))
}

// ---------------------------------------------------------------------------
// POST /v1/agents/chat — multi-turn customer support conversation
//
// Backs the AI Chat tab in the customer mobile app. Differs from
// /v1/agents/run in three ways that matter:
//   1. It is a *conversation* — pass the returned session_id back on the next
//      turn and the agent keeps its full context.
//   2. It runs as AgentType::CustomerSupport, which carries a hard tool
//      allowlist (get_shipment / reschedule_delivery / escalate_to_human).
//      Dispatch, billing, driver and analytics tools are unreachable here.
//   3. Tools execute with the caller's own bearer token, so order-intake's
//      RBAC and tenant isolation apply to the agent exactly as they would to
//      the app making the same call directly.
// ---------------------------------------------------------------------------

/// Shipment the customer can legitimately ask about, supplied by the app from
/// the list it already fetched under this same user's session.
///
/// This is a convenience index, not an authorisation input: tool calls still go
/// out under the caller's token, so putting a foreign id here buys nothing —
/// order-intake rejects it the same way it would reject the app asking directly.
#[derive(Debug, Deserialize)]
struct ChatShipmentContext {
    id:      Option<String>,
    awb:     Option<String>,
    status:  Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatRequest {
    /// Omit to start a new conversation; pass the previous reply's session_id
    /// to continue one.
    session_id: Option<Uuid>,
    message:    String,
    #[serde(default)]
    shipments:  Vec<ChatShipmentContext>,
}

const MAX_CHAT_MESSAGE_CHARS: usize = 2_000;
const MAX_CHAT_SHIPMENTS:     usize = 20;

async fn chat(
    State(state): State<AppState>,
    claims: AuthClaims,
    headers: HeaderMap,
    Json(req): Json<ChatRequest>,
) -> impl IntoResponse {
    if !claims.can_use_ai() {
        return Err(AppError::Forbidden { resource: "ai_features".into() });
    }

    let message = req.message.trim().to_owned();
    if message.is_empty() {
        return Err(AppError::Validation("message must not be empty".into()));
    }
    if message.chars().count() > MAX_CHAT_MESSAGE_CHARS {
        return Err(AppError::Validation(format!(
            "message must be {} characters or fewer",
            MAX_CHAT_MESSAGE_CHARS
        )));
    }

    // Propagate the caller's token so tools act with the caller's authority.
    let ctx = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(ToolContext::with_bearer)
        .unwrap_or_default();

    let turn = build_turn(&message, &req.shipments);

    let session = match req.session_id {
        // ── Continue an existing conversation ──────────────────────────
        Some(id) => {
            let existing = state
                .session_repo
                .find_by_id(id)
                .await
                .map_err(AppError::internal)?
                .ok_or_else(|| AppError::NotFound { resource: "agent_session", id: id.to_string() })?;

            // Tenant isolation, then per-user isolation. Tenant alone is not
            // enough here: every customer of a tenant shares its tenant_id, so
            // without the user check one customer could resume another's chat
            // by guessing a session id.
            if existing.tenant_id.inner() != claims.tenant_id {
                return Err(AppError::Forbidden { resource: "agent_session".into() });
            }
            if !AgentType::CustomerSupport.matches_role(&existing.role) {
                return Err(AppError::Forbidden { resource: "agent_session".into() });
            }
            let owner = existing.trigger.get("user_id").and_then(|v| v.as_str());
            if owner != Some(claims.user_id.to_string().as_str()) {
                return Err(AppError::Forbidden { resource: "agent_session".into() });
            }

            // Once a conversation has been handed to a human it stays handed
            // over. Resuming the agent here would flip the session out of
            // `HumanEscalated` and silently drop it from the ops review queue
            // (`GET /v1/agents/sessions/escalated`). Instead, record what the
            // customer added so the operator picking it up sees it, and tell
            // the customer a person has the case.
            if existing.status == crate::domain::entities::SessionStatus::HumanEscalated {
                return append_to_escalated(&state, existing, &turn).await;
            }

            state.runner.resume(existing, turn, ctx).await?
        }

        // ── Start a new conversation ───────────────────────────────────
        None => {
            let trigger = serde_json::json!({
                "source":  "customer_app_chat",
                "user_id": claims.user_id.to_string(),
                "email":   claims.email,
            });
            state
                .runner
                .run_with_context(
                    logisticos_types::TenantId::from_uuid(claims.tenant_id),
                    AgentType::CustomerSupport.into(),
                    trigger,
                    turn,
                    ctx,
                )
                .await?
        }
    };

    let escalated = session.status == crate::domain::entities::SessionStatus::HumanEscalated;
    let reply = session.outcome.clone().unwrap_or_else(|| {
        if escalated {
            "I've passed this to a member of our support team — they'll follow up with you shortly.".to_owned()
        } else {
            "Sorry, I wasn't able to answer that. Try rephrasing, or ask for a human agent.".to_owned()
        }
    });

    Ok::<_, AppError>((
        StatusCode::OK,
        Json(serde_json::json!({
            "session_id": session.id,
            "reply":      reply,
            "escalated":  escalated,
            "status":     session.status,
        })),
    ))
}

/// GET /v1/agents/chat/:id — current state of one customer conversation.
///
/// The app keeps its own message list, so this deliberately returns only what
/// the app cannot know: whether the conversation is still with a human, and the
/// latest assistant text. That is how an operator's resolution — written into
/// the session by `resolve_escalation` long after the app went to sleep —
/// reaches the customer's chat thread on next open.
async fn get_chat(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let session = state
        .session_repo
        .find_by_id(id)
        .await
        .map_err(AppError::internal)?
        .ok_or_else(|| AppError::NotFound { resource: "agent_session", id: id.to_string() })?;

    // Same three guards as resuming a chat: tenant, agent type, and the
    // originating user — customers of one tenant must not read each other's
    // conversations.
    if session.tenant_id.inner() != claims.tenant_id
        || !AgentType::CustomerSupport.matches_role(&session.role)
        || session.trigger.get("user_id").and_then(|v| v.as_str())
            != Some(claims.user_id.to_string().as_str())
    {
        return Err(AppError::Forbidden { resource: "agent_session".into() });
    }

    let latest_reply = session
        .messages
        .iter()
        .rev()
        .find(|m| matches!(m.role, crate::domain::entities::MessageRole::Assistant))
        .and_then(|m| m.content.as_str())
        .map(str::to_owned);

    Ok::<_, AppError>((
        StatusCode::OK,
        Json(serde_json::json!({
            "session_id":   session.id,
            "status":       session.status,
            "escalated":    session.status == crate::domain::entities::SessionStatus::HumanEscalated,
            "resolved_by_human": session.status == crate::domain::entities::SessionStatus::Completed
                && session.escalation_reason.is_some(),
            "latest_reply": latest_reply,
        })),
    ))
}

/// Attach a follow-up customer message to a session that is already awaiting a
/// human, without restarting the agent or clearing the escalation.
async fn append_to_escalated(
    state: &AppState,
    mut session: crate::domain::entities::AgentSession,
    turn: &str,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    session.messages.push(crate::domain::entities::AgentMessage {
        role:    crate::domain::entities::MessageRole::User,
        content: serde_json::Value::String(turn.to_owned()),
    });
    state.session_repo.save(&session).await.map_err(AppError::internal)?;

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "session_id": session.id,
            "reply":      "Thanks — I've added that to your case. One of our support team has it and will get back to you.",
            "escalated":  true,
            "status":     session.status,
        })),
    ))
}

/// Render one user turn: the customer's own words plus the shipment index the
/// app holds, so the agent can map "my Cebu parcel" to a shipment id without a
/// tenant-wide lookup.
fn build_turn(message: &str, shipments: &[ChatShipmentContext]) -> String {
    if shipments.is_empty() {
        return format!("Customer says: {}", message);
    }

    let mut ctx = String::from("The customer's own shipments (the only ones you may discuss):\n");
    for s in shipments.iter().take(MAX_CHAT_SHIPMENTS) {
        ctx.push_str(&format!(
            "- awb={} id={} status={}\n",
            s.awb.as_deref().unwrap_or("unknown"),
            s.id.as_deref().unwrap_or("unknown"),
            s.status.as_deref().unwrap_or("unknown"),
        ));
    }
    format!("{}\nCustomer says: {}", ctx, message)
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
    if !claims.can_use_ai() {
        return Err(AppError::Forbidden { resource: "ai_features".into() });
    }
    let trigger = req.context.unwrap_or(serde_json::json!({"tenant_id": claims.tenant_id.to_string()}));

    let session = state
        .runner
        .run(
            logisticos_types::TenantId::from_uuid(claims.tenant_id),
            AgentType::OnDemand.into(),
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
    if !claims.can_use_ai() {
        return Err(AppError::Forbidden { resource: "ai_features".into() });
    }
    let sessions = state
        .session_repo
        .list_by_tenant(
            claims.tenant_id,
            q.limit.unwrap_or(50).clamp(1, 200),
            q.offset.unwrap_or(0).max(0),
        )
        .await
        .map_err(AppError::internal)?;

    // Return summary (no full message history for list view).
    let summaries: Vec<_> = sessions.iter().map(|s| serde_json::json!({
        "id":               s.id,
        "agent_type":       s.role.key(),
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
    if !claims.can_use_ai() {
        return Err(AppError::Forbidden { resource: "ai_features".into() });
    }
    let sessions = state
        .session_repo
        .list_escalated(claims.tenant_id)
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
    if !claims.can_use_ai() {
        return Err(AppError::Forbidden { resource: "ai_features".into() });
    }
    let session = state
        .session_repo
        .find_by_id(id)
        .await
        .map_err(AppError::internal)?
        .ok_or_else(|| AppError::NotFound { resource: "agent_session", id: id.to_string() })?;

    // Tenant isolation.
    if session.tenant_id.inner() != claims.tenant_id {
        return Err(AppError::Forbidden { resource: "agent_session".into() });
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
    if !claims.can_use_ai() {
        return Err(AppError::Forbidden { resource: "ai_features".into() });
    }
    let mut session = state
        .session_repo
        .find_by_id(id)
        .await
        .map_err(AppError::internal)?
        .ok_or_else(|| AppError::NotFound { resource: "agent_session", id: id.to_string() })?;

    if session.tenant_id.inner() != claims.tenant_id {
        return Err(AppError::Forbidden { resource: "agent_session".into() });
    }

    if session.status != crate::domain::entities::SessionStatus::HumanEscalated {
        return Err(AppError::BusinessRule("Session is not awaiting human resolution".into()));
    }

    // The operator's note is the reply the customer has been waiting for, so it
    // goes on the conversation as an assistant turn — that is what
    // `GET /v1/agents/chat/:id` hands back to the app.
    let is_customer_chat = AgentType::CustomerSupport.matches_role(&session.role);
    if is_customer_chat {
        session.messages.push(crate::domain::entities::AgentMessage {
            role:    crate::domain::entities::MessageRole::Assistant,
            content: serde_json::Value::String(body.resolution_notes.clone()),
        });
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

    // Tell the customer their case was answered. Best-effort: a failed publish
    // must not fail the operator's resolve — the note is already persisted and
    // the app will still show it the next time the chat is opened.
    if is_customer_chat {
        publish_escalation_resolved(&state, &session, &body.resolution_notes).await;
    }

    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"resolved": true, "session_id": id}))))
}

/// Emit `AGENT_ESCALATION_RESOLVED` so engagement can push the resolution to the
/// customer's device. `customer_id` is the chat session's originating user — the
/// same id engagement's push channel uses to look up device tokens in identity.
async fn publish_escalation_resolved(
    state: &AppState,
    session: &crate::domain::entities::AgentSession,
    resolution_notes: &str,
) {
    let Some(kafka) = state.kafka.as_ref() else {
        tracing::warn!(session_id = %session.id, "No Kafka producer — skipping escalation-resolved notification");
        return;
    };
    let Some(customer_id) = session.trigger.get("user_id").and_then(|v| v.as_str()) else {
        tracing::warn!(session_id = %session.id, "Escalated chat has no user_id in trigger — cannot notify");
        return;
    };

    let payload = serde_json::json!({
        "event_type": logisticos_events::topics::AGENT_ESCALATION_RESOLVED,
        "tenant_id":  session.tenant_id.inner().to_string(),
        "data": {
            "customer_id":      customer_id,
            "customer_email":   session.trigger.get("email").and_then(|v| v.as_str()).unwrap_or(""),
            "session_id":       session.id.to_string(),
            "resolution_notes": resolution_notes,
        }
    });

    if let Err(e) = kafka
        .publish_json(logisticos_events::topics::AGENT_ESCALATION_RESOLVED, &payload)
        .await
    {
        tracing::error!(session_id = %session.id, err = %e, "Failed to publish AGENT_ESCALATION_RESOLVED");
    }
}

// ---------------------------------------------------------------------------
// POST /internal/tools/execute — Python sidecar bridge
// Called by MCPBridge in the Python LangGraph agent sidecar.
// Not exposed through the API gateway.
// ---------------------------------------------------------------------------

/// The Python sidecar's bridge payload. `tenant_id` and `session_id` are
/// declared but unused here on purpose: tenancy travels inside `input`,
/// which is what each tool scopes on (and what the remote-MCP path
/// overwrites server-side). Reading them here would imply a second,
/// competing source of tenant truth.
#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
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
    // Sidecar bridge — the Python agent runs unattended, so there is no caller
    // token to propagate. Tools needing one will be rejected downstream.
    let result = state
        .tools
        .execute(&req.tool_name, req.input, req.tool_use_id.clone(), ToolContext::default())
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
