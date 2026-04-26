use axum::{extract::{Path, State}, Json};
use std::sync::Arc;
use uuid::Uuid;
use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::require_permission;
use logisticos_errors::AppError;
use crate::{api::http::AppState, application::commands::CreateApiKeyCommand, infrastructure::db::NewAuditEntry};

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
    let key_name = cmd.name.clone();
    let result = state.api_key_service.create(&tenant_id, cmd).await?;
    let audit = Arc::clone(&state.audit_log);
    let entry = NewAuditEntry {
        tenant_id:   claims.tenant_id,
        actor_id:    claims.user_id,
        actor_email: claims.email.clone(),
        action:      "api_key.created".into(),
        resource:    key_name,
    };
    tokio::spawn(async move { let _ = audit.append(&entry).await; });
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
    let audit = Arc::clone(&state.audit_log);
    let entry = NewAuditEntry {
        tenant_id:   claims.tenant_id,
        actor_id:    claims.user_id,
        actor_email: claims.email.clone(),
        action:      "api_key.revoked".into(),
        resource:    id.to_string(),
    };
    tokio::spawn(async move { let _ = audit.append(&entry).await; });
    Ok(axum::http::StatusCode::NO_CONTENT)
}
