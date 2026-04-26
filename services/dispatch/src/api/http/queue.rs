use axum::{extract::{Query, State}, Json};
use serde::Deserialize;
use std::sync::Arc;
use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::require_permission;
use logisticos_errors::AppError;
use crate::api::http::AppState;

#[derive(Debug, Deserialize)]
pub struct QueueQuery {
    /// `pending` (default), `dispatched`, or `all`. Backwards compatible:
    /// existing callers omitting this param continue to see the pending queue.
    #[serde(default)]
    pub status: Option<String>,
}

pub async fn list_queue(
    AuthClaims(claims): AuthClaims,
    Query(q): Query<QueueQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::DISPATCH_VIEW);

    // "all" → no status filter; anything else falls through to the literal value.
    // Default to "pending" for callers that don't pass ?status=, preserving
    // the original /v1/queue contract.
    let filter: Option<&str> = match q.status.as_deref() {
        None | Some("") | Some("pending") => Some("pending"),
        Some("all")                       => None,
        Some(other)                       => Some(other),
    };

    let items = state
        .queue_repo
        .list_by_status(claims.tenant_id, filter)
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(serde_json::json!({ "data": items, "count": items.len() })))
}

pub async fn list_drivers(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::DISPATCH_VIEW);
    let drivers = state
        .drivers_repo
        .list_by_tenant(claims.tenant_id)
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(serde_json::json!({ "data": drivers })))
}
