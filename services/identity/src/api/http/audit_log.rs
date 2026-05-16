use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;
use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;
use crate::api::http::AppState;

#[derive(Debug, Deserialize)]
pub struct AuditLogQuery {
    /// Maximum number of entries to return. Clamped to 200. Defaults to 50.
    pub limit: Option<i64>,
    /// Number of entries to skip (for pagination). Defaults to 0.
    pub offset: Option<i64>,
}

pub async fn list(
    AuthClaims(claims): AuthClaims,
    Query(params): Query<AuditLogQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let limit  = params.limit.unwrap_or(50).min(200).max(1);
    let offset = params.offset.unwrap_or(0).max(0);
    let entries = state.audit_log
        .list_by_tenant(claims.tenant_id, limit, offset)
        .await
        .map_err(AppError::Internal)?;
    let has_more = entries.len() as i64 == limit;
    Ok(Json(serde_json::json!({
        "data":     entries,
        "limit":    limit,
        "offset":   offset,
        "has_more": has_more,
    })))
}
