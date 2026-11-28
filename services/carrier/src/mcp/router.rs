//! HTTP surface of the carrier MCP server: `GET /mcp/tools`, `POST /mcp/tools/call`.
//!
//! Mounted behind `require_auth` in `bootstrap.rs`; each handler additionally
//! re-validates the bearer token into an [`McpContext`] so the tenant, actor
//! and permissions it acts on come from the token and nowhere else.

use std::sync::Arc;
use std::time::Instant;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde_json::{json, Value};

use crate::mcp::{access, audit, auth, tools};
use crate::AppState;

pub fn mcp_router() -> Router<AppState> {
    Router::new()
        .route("/mcp/tools",      get(tools_list))
        .route("/mcp/tools/call", post(tools_call))
}

/// Lists exactly the tools this caller may run, so a client is never offered
/// a tool it would be refused.
async fn tools_list(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    let ctx = match auth::extract_context(&headers, &state.jwt) {
        Ok(c)  => c,
        Err(e) => return (StatusCode::UNAUTHORIZED, Json(json!({ "error": e }))).into_response(),
    };
    Json(json!({ "tools": tools::list_for(&ctx) })).into_response()
}

async fn tools_call(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let state = Arc::new(state);
    let ctx = match auth::extract_context(&headers, &state.jwt) {
        Ok(c)  => c,
        Err(e) => return (StatusCode::UNAUTHORIZED, Json(json!({ "error": e }))).into_response(),
    };

    let tool_name = match body["params"]["name"].as_str() {
        Some(n) => n.to_string(),
        None    => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "params.name is required" }))).into_response(),
    };

    let args  = body["params"]["arguments"].clone();
    let start = Instant::now();

    match tools::dispatch(&tool_name, &args, &ctx, &state).await {
        Ok(result) => {
            audit::audit_tool_call(&ctx, &tool_name, true, start);
            Json(json!({ "result": result })).into_response()
        }
        Err(tools::DispatchError::Denied(d)) => {
            audit::audit_tool_denied(&ctx, &tool_name, denial_reason(&d));
            (StatusCode::FORBIDDEN, Json(json!({ "error": d.message() }))).into_response()
        }
        Err(tools::DispatchError::Failed(e)) => {
            audit::audit_tool_call(&ctx, &tool_name, false, start);
            (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": e }))).into_response()
        }
    }
}

fn denial_reason(d: &access::Denied) -> &'static str {
    match d {
        access::Denied::NotExposed { .. }       => "not_exposed",
        access::Denied::MissingPermission { .. } => "missing_permission",
        access::Denied::PartnerScoped { .. }    => "partner_scoped",
    }
}
