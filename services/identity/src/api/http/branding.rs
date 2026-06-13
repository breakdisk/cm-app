use axum::{extract::{Query, State}, Json};
use serde::Deserialize;
use std::sync::Arc;
use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::require_permission;
use logisticos_errors::AppError;
use crate::{
    api::http::AppState,
    application::commands::UpdateBrandingCommand,
    domain::entities::Branding,
    infrastructure::db::NewAuditEntry,
};

/// GET /v1/tenants/me/branding — the caller's own branding. Read-only; any
/// authenticated member of the tenant may read it (portals/apps need it to
/// theme themselves). Falls back to the default LogisticOS brand when the
/// tenant has no custom branding row.
pub async fn get_self_branding(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let tenant_id = logisticos_types::TenantId::from_uuid(claims.tenant_id);
    let branding = state.branding_repo
        .find_by_tenant(&tenant_id).await
        .map_err(AppError::Internal)?
        .unwrap_or_else(|| Branding::default_logisticos(tenant_id));
    Ok(Json(serde_json::json!({ "data": branding })))
}

/// PUT /v1/tenants/me/branding — upsert the caller's branding. Gated on the
/// white-label entitlement (Enterprise tier) AND `tenants:manage`. Partial:
/// omitted fields keep their current value (or the default on first write).
pub async fn put_self_branding(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<UpdateBrandingCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::TENANT_MANAGE);

    // Entitlement gate — only Enterprise tenants may customise branding.
    if !claims.can_use_white_label() {
        return Err(AppError::BusinessRule(
            "White-label branding requires the Enterprise plan. Upgrade your subscription to customise branding.".into()
        ));
    }

    use validator::Validate;
    cmd.validate().map_err(|e| AppError::Validation(e.to_string()))?;

    // Validate any supplied hex colours up front.
    for color in [&cmd.primary_color, &cmd.secondary_color, &cmd.accent_color]
        .into_iter()
        .flatten()
    {
        Branding::validate_hex(color).map_err(|e| AppError::Validation(e.to_string()))?;
    }

    let tenant_id = logisticos_types::TenantId::from_uuid(claims.tenant_id);

    // Start from the existing row, or a default skeleton on first write.
    let mut branding = state.branding_repo
        .find_by_tenant(&tenant_id).await
        .map_err(AppError::Internal)?
        .unwrap_or_else(|| Branding::default_logisticos(tenant_id.clone()));

    // Merge — only overwrite fields the caller supplied.
    if let Some(v) = cmd.display_name    { branding.display_name = v; }
    if let Some(v) = cmd.app_tagline     { branding.app_tagline = Some(v); }
    if let Some(v) = cmd.logo_url        { branding.logo_url = Some(v); }
    if let Some(v) = cmd.logo_dark_url   { branding.logo_dark_url = Some(v); }
    if let Some(v) = cmd.favicon_url     { branding.favicon_url = Some(v); }
    if let Some(v) = cmd.primary_color   { branding.primary_color = Some(v); }
    if let Some(v) = cmd.secondary_color { branding.secondary_color = Some(v); }
    if let Some(v) = cmd.accent_color    { branding.accent_color = Some(v); }
    if let Some(v) = cmd.support_email   { branding.support_email = Some(v); }
    if let Some(v) = cmd.support_phone   { branding.support_phone = Some(v); }
    if let Some(v) = cmd.legal_text      { branding.legal_text = v; }
    branding.updated_at = chrono::Utc::now();

    state.branding_repo.upsert(&branding).await.map_err(AppError::Internal)?;

    // Audit the mutation (actor, tenant, timestamp) — fire-and-forget.
    let audit = Arc::clone(&state.audit_log);
    let entry = NewAuditEntry {
        tenant_id:   claims.tenant_id,
        actor_id:    claims.user_id,
        actor_email: claims.email.clone(),
        action:      "tenant.branding_updated".into(),
        resource:    branding.display_name.clone(),
    };
    tokio::spawn(async move { let _ = audit.append(&entry).await; });

    tracing::info!(tenant_id = %branding.tenant_id, "Tenant branding updated");
    Ok(Json(serde_json::json!({ "data": branding })))
}

#[derive(Debug, Deserialize)]
pub struct PublicBrandingQuery {
    pub slug: Option<String>,
    pub host: Option<String>,
}

/// GET /v1/public/branding?slug=<slug>|host=<hostname> — unauthenticated brand
/// resolution for pre-login screens, customer tracking, and subdomain/custom-
/// domain routing. Returns the resolved brand or the default LogisticOS brand
/// when the tenant is unknown or has no custom branding. Safe fields only —
/// no PII beyond the support contact the tenant chose to publish.
pub async fn get_public_branding(
    State(state): State<Arc<AppState>>,
    Query(q): Query<PublicBrandingQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let resolved = if let Some(host) = q.host.as_deref().filter(|s| !s.is_empty()) {
        state.branding_repo.find_by_custom_domain(host).await.map_err(AppError::Internal)?
    } else if let Some(slug) = q.slug.as_deref().filter(|s| !s.is_empty()) {
        state.branding_repo.find_by_slug(slug).await.map_err(AppError::Internal)?
    } else {
        return Err(AppError::Validation("provide a 'slug' or 'host' query parameter".into()));
    };

    let branding = resolved
        .unwrap_or_else(|| Branding::default_logisticos(logisticos_types::TenantId::from_uuid(uuid::Uuid::nil())));
    Ok(Json(serde_json::json!({ "data": branding })))
}
