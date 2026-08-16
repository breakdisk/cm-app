use axum::{extract::{Path, State}, Json};
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::require_permission;
use logisticos_errors::AppError;
use crate::api::http::AppState;

#[derive(Debug, Serialize)]
pub struct PricingFeatureDto {
    pub feature_key:      String,
    pub feature_name:     String,
    pub feature_category: String,
    pub description:      Option<String>,
    pub enabled_tiers:    Vec<String>,
    pub is_system:        bool,
    pub sort_order:       i32,
}

/// GET /v1/pricing/features — list the entire feature matrix.
/// Any authenticated user may read this; only admins may mutate it.
pub async fn list_features(
    AuthClaims(_claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let features = state.pricing_feature_repo
        .list_all()
        .await
        .map_err(AppError::Internal)?;

    let dtos: Vec<PricingFeatureDto> = features.into_iter().map(|f| PricingFeatureDto {
        feature_key:      f.feature_key,
        feature_name:     f.feature_name,
        feature_category: f.feature_category,
        description:      f.description,
        enabled_tiers:    f.enabled_tiers,
        is_system:        f.is_system,
        sort_order:       f.sort_order,
    }).collect();

    Ok(Json(serde_json::json!({ "data": dtos })))
}

/// GET /v1/pricing/features/active — returns only the features enabled for
/// the caller's current subscription tier. Useful for service-level enforcement.
pub async fn list_active_features(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let tier = &claims.subscription_tier;
    let features = state.pricing_feature_repo
        .list_for_tier(tier)
        .await
        .map_err(AppError::Internal)?;

    let keys: Vec<&str> = features.iter().map(|f| f.feature_key.as_str()).collect();
    Ok(Json(serde_json::json!({ "data": { "tier": tier, "enabled_features": keys } })))
}

#[derive(Debug, Deserialize)]
pub struct SetTiersPayload {
    /// The complete list of tier slugs that should have access to this feature.
    pub enabled_tiers: Vec<String>,
}

const VALID_TIERS: &[&str] = &["starter", "growth", "business", "enterprise"];

/// PUT /v1/pricing/features/:key/tiers — update which tiers include a feature.
/// Requires `tenants:manage` (platform admin only). Writes an audit entry.
pub async fn set_feature_tiers(
    AuthClaims(claims): AuthClaims,
    Path(feature_key): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SetTiersPayload>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::TENANT_MANAGE);

    for t in &payload.enabled_tiers {
        if !VALID_TIERS.contains(&t.as_str()) {
            return Err(AppError::Validation(format!(
                "Invalid tier '{}'. Valid values: starter, growth, business, enterprise", t
            )));
        }
    }

    state.pricing_feature_repo
        .set_enabled_tiers(&feature_key, &payload.enabled_tiers)
        .await
        .map_err(|e| {
            if e.to_string().contains("not found") {
                AppError::NotFound { resource: "PricingFeature", id: feature_key.clone() }
            } else {
                AppError::Internal(e)
            }
        })?;

    use crate::infrastructure::db::NewAuditEntry;
    let audit = Arc::clone(&state.audit_log);
    let entry = NewAuditEntry {
        tenant_id:   claims.tenant_id,
        actor_id:    claims.user_id,
        actor_email: claims.email.clone(),
        action:      "pricing_feature.tiers_updated".into(),
        resource:    feature_key.clone(),
    };
    tokio::spawn(async move { let _ = audit.append(&entry).await; });

    Ok(Json(serde_json::json!({
        "data": {
            "feature_key":   feature_key,
            "enabled_tiers": payload.enabled_tiers,
        }
    })))
}
