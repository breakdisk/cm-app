use axum::{extract::{Path, State}, Json};
use std::sync::Arc;
use uuid::Uuid;
use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::require_permission;
use logisticos_errors::AppError;
use crate::{
    api::http::AppState,
    application::commands::{CreateTenantCommand, FinalizeTenantCommand, UpdateTenantCommand},
};

pub async fn create_tenant(
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<CreateTenantCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    let tenant = state.tenant_service.create_tenant(cmd).await?;
    Ok(Json(serde_json::json!({ "data": { "tenant_id": tenant.id, "slug": tenant.slug } })))
}

/// Finalize the caller's own tenant (promote `draft` → `active`).
///
/// Reached via `POST /v1/tenants/me/finalize` — the draft-tenant JWT minted
/// at Firebase exchange time grants `tenants:update-self`, so this is the
/// only tenant-mutating route the onboarding user can reach. After success,
/// the client should call `/api/auth/refresh` to receive a JWT with the
/// full role-based permission set.
pub async fn finalize_self(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<FinalizeTenantCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::TENANT_UPDATE_SELF);
    let tenant_id = logisticos_types::TenantId::from_uuid(claims.tenant_id);
    let tenant = state.tenant_service.finalize_self(&tenant_id, cmd).await?;
    Ok(Json(serde_json::json!({
        "data": {
            "tenant_id": tenant.id,
            "slug":      tenant.slug,
            "name":      tenant.name,
            "status":    tenant.status.as_str(),
        }
    })))
}

/// GET /v1/tenants/me — returns the caller's own tenant. Read-only, no
/// permission gate beyond a valid JWT (every authenticated user can see
/// the tenant they belong to). Used by admin Settings → General to render
/// the editable profile form.
pub async fn get_self(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let tenant_id = logisticos_types::TenantId::from_uuid(claims.tenant_id);
    let tenant = state.tenant_service.tenant_repo_ref()
        .find_by_id(&tenant_id).await
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::NotFound {
            resource: "Tenant",
            id: tenant_id.inner().to_string(),
        })?;
    Ok(Json(serde_json::json!({ "data": tenant })))
}

/// PUT /v1/tenants/:id — partial profile update (name, owner_email). The
/// caller must hold TENANT_MANAGE *and* the path id must match their own
/// tenant_id (cross-tenant edits are NotFound rather than Forbidden so we
/// don't leak existence to other tenants).
pub async fn update_tenant(
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<UpdateTenantCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::TENANT_MANAGE);
    if claims.tenant_id != id {
        return Err(AppError::NotFound { resource: "Tenant", id: id.to_string() });
    }
    let tenant_id = logisticos_types::TenantId::from_uuid(id);
    let tenant = state.tenant_service.update_tenant(&tenant_id, cmd).await?;
    Ok(Json(serde_json::json!({ "data": tenant })))
}
