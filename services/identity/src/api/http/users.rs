use axum::{extract::{Path, State}, Json};
use std::sync::Arc;
use uuid::Uuid;
use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::require_permission;
use logisticos_errors::AppError;
use crate::{api::http::AppState, application::commands::{InviteUserCommand, UpdateUserSelfCommand}};

pub async fn get_me(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = logisticos_types::UserId::from_uuid(claims.user_id);
    let user = state.tenant_service.user_repo_ref().find_by_id(&user_id).await
        .map_err(AppError::Internal)?
        .ok_or(AppError::NotFound { resource: "User", id: claims.user_id.to_string() })?;
    Ok(Json(serde_json::json!({ "data": user })))
}

pub async fn update_me(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<UpdateUserSelfCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = logisticos_types::UserId::from_uuid(claims.user_id);
    let user = state.tenant_service.update_user_self(&user_id, cmd).await?;
    Ok(Json(serde_json::json!({ "data": user })))
}

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
    // DEV-ONLY: temp_password is returned in response for local smoke testing.
    // In production, the password is delivered out-of-band via email (engagement service).
    // TODO: Gate this behind a dev-mode config flag before going to production.
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

/// `PATCH /v1/users/:id/roles`
///
/// Grants or revokes a single named role on a user within the calling tenant.
/// Supports role `"hub_scanner"` (SHIPMENT_READ + SHIPMENT_UPDATE permissions).
///
/// Body: `{ "role": "hub_scanner", "action": "assign" | "revoke" }`
/// Requires: `USERS_MANAGE` permission.
pub async fn patch_user_roles(
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<PatchRolesRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::USERS_MANAGE);

    let user_id = logisticos_types::UserId::from_uuid(id);
    let repo    = state.tenant_service.user_repo_ref();

    let mut user = repo.find_by_id(&user_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::NotFound { resource: "User", id: id.to_string() })?;

    // Tenant isolation
    if user.tenant_id.inner() != claims.tenant_id {
        return Err(AppError::NotFound { resource: "User", id: id.to_string() });
    }

    match body.action.as_str() {
        "assign" => user.assign_role(&body.role),
        "revoke" => user.revoke_role(&body.role),
        other    => return Err(AppError::Validation(
            format!("Unknown action '{}'; expected 'assign' or 'revoke'", other)
        )),
    }

    repo.save(&user)
        .await
        .map_err(AppError::Internal)?;

    Ok(Json(serde_json::json!({ "data": user })))
}

#[derive(serde::Deserialize)]
pub struct PatchRolesRequest {
    pub role:   String,
    /// "assign" or "revoke"
    pub action: String,
}

/// POST /v1/users/:id/invite-link
///
/// Generates a signed deep-link invite URL for a pre-registered driver.
/// The admin copies this URL and shares it with the driver (WhatsApp, SMS, etc.).
/// The driver taps the link; the app pre-fills their phone and tenant and
/// sends them through normal OTP login, landing in the correct tenant.
///
/// Requires: USERS_INVITE permission.
/// Returns:  `{ "data": { "invite_url": "https://...", "expires_at": "..." } }`
pub async fn generate_invite_link(
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::USERS_INVITE);
    let tenant_id = logisticos_types::TenantId::from_uuid(claims.tenant_id);
    let result = state.tenant_service.generate_invite_link(&tenant_id, id).await?;
    Ok(Json(serde_json::json!({ "data": result })))
}
