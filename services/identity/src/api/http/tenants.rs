use axum::{extract::State, Json};
use std::sync::Arc;
use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::require_permission;
use logisticos_errors::AppError;
use crate::{
    api::http::AppState,
    application::commands::{CreateTenantCommand, FinalizeTenantCommand},
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
