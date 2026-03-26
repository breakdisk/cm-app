use axum::{extract::{Path, State}, Json};
use std::sync::Arc;
use uuid::Uuid;
use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;
use crate::{api::http::AppState, application::commands::InviteUserCommand};

pub async fn list_users(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::USERS_MANAGE);
    let tenant_id = logisticos_types::TenantId::from_uuid(claims.tenant_id);
    let users = state.tenant_service.user_repo_ref().list_by_tenant(&tenant_id).await
        .map_err(AppError::Internal)?;
    Ok(Json(serde_json::json!({ "data": users })))
}

pub async fn invite_user(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<InviteUserCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::USERS_INVITE);
    let tenant_id = logisticos_types::TenantId::from_uuid(claims.tenant_id);
    let (user, temp_password) = state.tenant_service.invite_user(&tenant_id, cmd).await?;
    Ok(Json(serde_json::json!({ "data": { "user_id": user.id, "email": user.email, "temp_password": temp_password } })))
}

pub async fn get_user(
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::USERS_MANAGE);
    let user_id = logisticos_types::UserId::from_uuid(id);
    let user = state.tenant_service.user_repo_ref().find_by_id(&user_id).await
        .map_err(AppError::Internal)?
        .ok_or(AppError::NotFound { resource: "User", id: id.to_string() })?;
    Ok(Json(serde_json::json!({ "data": user })))
}
