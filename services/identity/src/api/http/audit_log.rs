use axum::extract::State;
use axum::Json;
use std::sync::Arc;
use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;
use crate::api::http::AppState;

pub async fn list(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let entries = state.audit_log
        .list_by_tenant(claims.tenant_id, 100)
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(serde_json::json!({ "data": entries })))
}
