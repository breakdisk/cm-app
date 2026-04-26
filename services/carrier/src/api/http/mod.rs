use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post, put},
    Router,
};
use serde::Deserialize;
use uuid::Uuid;

use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::rbac::permissions;
use logisticos_errors::AppError;

use crate::application::services::{OnboardCarrierCommand, UpdateCarrierCommand};
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/carriers",                  get(list_carriers).post(onboard_carrier))
        .route("/v1/carriers/me",               get(get_my_carrier))
        .route("/v1/carriers/rate-shop",        get(rate_shop))
        .route("/v1/carriers/:id",              get(get_carrier).put(update_carrier))
        .route("/v1/carriers/:id/activate",     post(activate_carrier))
        .route("/v1/carriers/:id/suspend",      post(suspend_carrier))
}

#[derive(Debug, Deserialize)]
struct ListQuery { limit: Option<i64>, offset: Option<i64> }

async fn list_carriers(
    State(state): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<ListQuery>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CARRIERS_READ)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let carriers = state.carrier_svc.list(&tenant_id, q.limit.unwrap_or(50), q.offset.unwrap_or(0)).await?;
    let count = carriers.len();
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"carriers": carriers, "count": count}))))
}

async fn onboard_carrier(
    State(state): State<AppState>,
    claims: AuthClaims,
    Json(cmd): Json<OnboardCarrierCommand>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CARRIERS_MANAGE)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let carrier = state.carrier_svc.onboard(&tenant_id, cmd).await?;
    Ok::<_, AppError>((StatusCode::CREATED, Json(carrier)))
}

/// GET /v1/carriers/me — returns the carrier whose contact_email matches
/// the authenticated user's JWT email. Used by the partner portal so it
/// doesn't need to know its own carrier UUID.
async fn get_my_carrier(
    State(state): State<AppState>,
    claims: AuthClaims,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CARRIERS_READ)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let carrier = state.carrier_svc.get_by_email(&tenant_id, &claims.email).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(carrier)))
}

async fn get_carrier(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::CARRIERS_READ)?;
    let carrier = state.carrier_svc.get(id).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(carrier)))
}

async fn update_carrier(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(cmd): Json<UpdateCarrierCommand>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CARRIERS_MANAGE)?;
    // Tenant guard: refuse cross-tenant updates even if a leaked admin
    // token was used. Same shape as get_carrier returning 404 instead of
    // 403 to avoid leaking carrier existence to other tenants.
    let existing = state.carrier_svc.get(id).await?;
    let claim_tenant = TenantId::from_uuid(claims.tenant_id);
    if existing.tenant_id.inner() != claim_tenant.inner() {
        return Err(AppError::NotFound { resource: "Carrier", id: id.to_string() });
    }
    let carrier = state.carrier_svc.update(id, cmd).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(carrier)))
}

async fn activate_carrier(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::CARRIERS_MANAGE)?;
    let carrier = state.carrier_svc.activate(id).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(carrier)))
}

#[derive(Debug, Deserialize)]
struct SuspendBody { reason: String }

async fn suspend_carrier(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(body): Json<SuspendBody>,
) -> impl IntoResponse {
    claims.require_permission(permissions::CARRIERS_MANAGE)?;
    let carrier = state.carrier_svc.suspend(id, body.reason).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(carrier)))
}

#[derive(Debug, Deserialize)]
struct RateShopQuery {
    service_type: String,
    weight_kg: f32,
}

async fn rate_shop(
    State(state): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<RateShopQuery>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::CARRIERS_READ)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let quotes = state.carrier_svc.shop_rates(&tenant_id, &q.service_type, q.weight_kg).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"quotes": quotes}))))
}
