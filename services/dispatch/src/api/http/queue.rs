use axum::{extract::State, Json};
use std::sync::Arc;
use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::require_permission;
use logisticos_errors::AppError;
use crate::api::http::AppState;

pub async fn list_queue(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::DISPATCH_VIEW);
    let items = state
        .queue_repo
        .list_pending(claims.tenant_id)
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
