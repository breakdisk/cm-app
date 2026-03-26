use axum::{extract::{Path, State}, Json};
use std::sync::Arc;
use uuid::Uuid;
use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;
use crate::{api::http::AppState, application::commands::CreateApiKeyCommand};

pub async fn list(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::API_KEYS_MANAGE);
    let tenant_id = logisticos_types::TenantId::from_uuid(claims.tenant_id);
    let keys = state.api_key_service.list(&tenant_id).await?;
    Ok(Json(serde_json::json!({ "data": keys })))
}

pub async fn create(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<CreateApiKeyCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::API_KEYS_MANAGE);
    let tenant_id = logisticos_types::TenantId::from_uuid(claims.tenant_id);
    let result = state.api_key_service.create(&tenant_id, cmd).await?;
    Ok(Json(serde_json::json!({ "data": result })))
}

pub async fn revoke(
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<axum::http::StatusCode, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::API_KEYS_MANAGE);
    let tenant_id = logisticos_types::TenantId::from_uuid(claims.tenant_id);
    let key_id = logisticos_types::ApiKeyId::from_uuid(id);
    state.api_key_service.revoke(&tenant_id, &key_id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
