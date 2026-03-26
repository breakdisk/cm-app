use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use uuid::Uuid;

use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::rbac::permissions;
use logisticos_errors::AppError;

use crate::application::services::{OnboardCarrierCommand};
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/carriers",                  get(list_carriers).post(onboard_carrier))
        .route("/v1/carriers/rate-shop",        get(rate_shop))
        .route("/v1/carriers/:id",              get(get_carrier))
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
    claims.require_permission(permissions::CARRIERS_READ)?;
    let carriers = state.carrier_svc.list(&claims.tenant_id, q.limit.unwrap_or(50), q.offset.unwrap_or(0)).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"carriers": carriers, "count": carriers.len()}))))
}

async fn onboard_carrier(
    State(state): State<AppState>,
    claims: AuthClaims,
    Json(cmd): Json<OnboardCarrierCommand>,
) -> impl IntoResponse {
    claims.require_permission(permissions::CARRIERS_MANAGE)?;
    let carrier = state.carrier_svc.onboard(&claims.tenant_id, cmd).await?;
    Ok::<_, AppError>((StatusCode::CREATED, Json(carrier)))
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
    claims.require_permission(permissions::CARRIERS_READ)?;
    let quotes = state.carrier_svc.shop_rates(&claims.tenant_id, &q.service_type, q.weight_kg).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"quotes": quotes}))))
}
