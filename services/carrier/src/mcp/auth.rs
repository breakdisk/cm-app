use axum::http::HeaderMap;
use std::sync::Arc;
use logisticos_auth::jwt::JwtService;
use super::context::McpContext;

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

    Ok(McpContext {
        tenant_id:   claims.tenant_id,
        actor_uid:   claims.user_id,
        permissions: claims.permissions,
        trace_id:    uuid::Uuid::new_v4().to_string(),
    })
}
