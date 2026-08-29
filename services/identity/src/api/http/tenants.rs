use axum::{extract::{Path, State}, Json};
use std::sync::Arc;
use uuid::Uuid;
use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::require_permission;
use logisticos_errors::AppError;
use crate::{
    api::http::AppState,
    application::commands::{CreateTenantCommand, FinalizeTenantCommand, UpdateTenantCommand, UpgradeTierCommand},
    infrastructure::db::NewAuditEntry,
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
/// caller must hold TENANT_UPDATE_SELF *and* the path id must match their own
/// tenant_id (cross-tenant edits are NotFound rather than Forbidden so we
/// don't leak existence to other tenants).
///
/// Was TENANT_MANAGE, which no role grants, so this returned 403 to everyone.
/// It is not simply granted, because the same constant gates the tier upgrade
/// and the platform-wide pricing matrix below.
pub async fn update_tenant(
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<UpdateTenantCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::TENANT_UPDATE_SELF);
    if claims.tenant_id != id {
        return Err(AppError::NotFound { resource: "Tenant", id: id.to_string() });
    }
    let tenant_id = logisticos_types::TenantId::from_uuid(id);
    let tenant = state.tenant_service.update_tenant(&tenant_id, cmd).await?;
    let audit = Arc::clone(&state.audit_log);
    let entry = NewAuditEntry {
        tenant_id:   claims.tenant_id,
        actor_id:    claims.user_id,
        actor_email: claims.email.clone(),
        action:      "tenant.updated".into(),
        resource:    tenant.name.clone(),
    };
    tokio::spawn(async move { let _ = audit.append(&entry).await; });
    Ok(Json(serde_json::json!({ "data": tenant })))
}

/// PUT /v1/tenants/:id/tier — set the subscription tier for a tenant.
///
/// Requires `TENANT_MANAGE` and the path id must be the caller's own tenant
/// (cross-tenant edits return NotFound). The new tier is validated against the
/// known snake_case variants: `starter | growth | business | enterprise`.
/// An audit entry is written for every successful change.
pub async fn upgrade_tier(
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<UpgradeTierCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::TENANT_MANAGE);
    if claims.tenant_id != id {
        return Err(AppError::NotFound { resource: "Tenant", id: id.to_string() });
    }
    let tenant_id = logisticos_types::TenantId::from_uuid(id);
    let tier = state.tenant_service.upgrade_tier(&tenant_id, cmd).await?;
    let audit = Arc::clone(&state.audit_log);
    let tier_str = format!("{:?}", tier).to_lowercase();
    let entry = NewAuditEntry {
        tenant_id:   claims.tenant_id,
        actor_id:    claims.user_id,
        actor_email: claims.email.clone(),
        action:      "tenant.tier_updated".into(),
        resource:    tier_str.clone(),
    };
    tokio::spawn(async move { let _ = audit.append(&entry).await; });
    Ok(Json(serde_json::json!({ "data": { "subscription_tier": tier_str } })))
}

/// PUT /v1/internal/tenants/:id/tier — grant a tenant a subscription tier.
///
/// The system counterpart to `upgrade_tier` above, and the reason that one can
/// stay behind a permission nobody holds.
///
/// `upgrade_tier` requires `tenants:manage`, which no role grants and which
/// `libs/auth/src/rbac.rs` has a test to keep ungranted: the same permission
/// gates `PUT /v1/pricing/features/:key/tiers`, which rewrites the pricing
/// matrix for every tenant on the platform. Granting it so a tenant could
/// upgrade themselves would hand them everyone's prices and a free jump to
/// Enterprise in one move.
///
/// So the tier is not something a tenant asks for. It is something the platform
/// grants once `services/payments` has captured a payment for it — and this is
/// where that grant lands. There is no principal, no tenant claim and no
/// permission check, because there is no user here; the caller is
/// `payments::infrastructure::external::identity_client`, authenticated by the
/// `X-Internal-Secret` guard on the whole `/v1/internal` scope (which the
/// api-gateway strips on ingress, so the header can never arrive from the
/// public internet).
///
/// Idempotent: setting the tier a tenant already has is a no-op that still
/// answers 200, because the caller's retry sweep will do exactly that whenever
/// its own record of the grant failed to save.
pub async fn set_tier_internal(
    Path(id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<UpgradeTierCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    let tenant_id = logisticos_types::TenantId::from_uuid(id);
    let tier = state.tenant_service.upgrade_tier(&tenant_id, cmd).await?;
    let tier_str = format!("{:?}", tier).to_lowercase();

    // Audited with the tenant as its own actor rather than a fabricated user:
    // there is no person here, and inventing one would put a name against a
    // change nobody made.
    let audit = Arc::clone(&state.audit_log);
    let entry = NewAuditEntry {
        tenant_id:   id,
        actor_id:    id,
        actor_email: "system:payments".into(),
        action:      "tenant.tier_granted_by_subscription".into(),
        resource:    tier_str.clone(),
    };
    tokio::spawn(async move { let _ = audit.append(&entry).await; });

    tracing::info!(tenant_id = %id, tier = %tier_str, "tier granted from a subscription payment");
    Ok(Json(serde_json::json!({ "data": { "subscription_tier": tier_str } })))
}
