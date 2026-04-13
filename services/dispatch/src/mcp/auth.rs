use axum::http::HeaderMap;
use std::sync::Arc;
use logisticos_auth::jwt::JwtService;
use super::context::McpContext;

/// Extracts and validates a Bearer JWT from request headers.
/// Returns `McpContext` on success or an error string on failure.
pub fn extract_context(headers: &HeaderMap, jwt: &Arc<JwtService>) -> Result<McpContext, String> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| "Missing Authorization header".to_string())?;

    let data = jwt
        .validate_access_token(token)
        .map_err(|e| format!("Invalid token: {e}"))?;

    let claims = data.claims;

    // Best-effort trace_id: generate a new UUID (future: extract from active tracing span).
    let trace_id = uuid::Uuid::new_v4().to_string();

    Ok(McpContext {
        tenant_id: claims.tenant_id,
        actor_uid: claims.user_id,
        permissions: claims.permissions,
        trace_id,
    })
}
