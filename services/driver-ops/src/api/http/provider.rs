//! Provider profiles: operations onboard a driver for freight or home moves
//! (and local or international, one truck or a fleet); the lead sets their own
//! working days and off days; order-intake asks how many home-move teams a
//! date can take.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{Duration, NaiveDate, Utc};
use serde::Deserialize;
use uuid::Uuid;

use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::require_permission;
use logisticos_errors::AppError;

use crate::api::http::AppState;
use crate::domain::value_objects::provider::{home_capacity, working_days, ProviderProfile};

fn not_found(id: Uuid) -> AppError {
    AppError::NotFound { resource: "Driver", id: id.to_string() }
}

/// `GET /v1/drivers/:id/provider` — the onboarding profile, and the lead's
/// availability, for operations.
pub async fn get_provider(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::FLEET_VIEW);
    let profile = state.providers.get(claims.tenant_id, id).await.map_err(AppError::Internal)?.ok_or_else(|| not_found(id))?;
    let off_days = state.providers.off_days(claims.tenant_id, id, Utc::now().date_naive()).await.map_err(AppError::Internal)?;
    Ok(Json(serde_json::json!({ "data": { "profile": profile, "off_days": off_days } })))
}

/// `PUT /v1/drivers/:id/provider` — onboarding: which jobs this driver is
/// offered. Operations only; a driver cannot put themselves on home moves.
pub async fn put_provider(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(body): Json<ProviderProfile>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::FLEET_MANAGE);
    let profile = body.validated().map_err(AppError::Validation)?;
    if !state.providers.put(claims.tenant_id, id, &profile).await.map_err(AppError::Internal)? {
        return Err(not_found(id));
    }
    tracing::info!(
        tenant_id = %claims.tenant_id, driver_id = %id, by = %claims.user_id,
        lines = ?profile.service_lines, coverage = ?profile.coverage,
        multi_truck = profile.multi_truck_capable, "provider profile set"
    );
    Ok(Json(serde_json::json!({ "data": profile })))
}

/// `GET /v1/drivers/me/provider` — the lead's own profile and availability.
pub async fn get_my_provider(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let me = claims.user_id;
    let profile = state.providers.get(claims.tenant_id, me).await.map_err(AppError::Internal)?.ok_or_else(|| not_found(me))?;
    let off_days = state.providers.off_days(claims.tenant_id, me, Utc::now().date_naive()).await.map_err(AppError::Internal)?;
    Ok(Json(serde_json::json!({ "data": { "profile": profile, "off_days": off_days } })))
}

#[derive(Debug, Deserialize)]
pub struct AvailabilityBody {
    pub working_days: Vec<i16>,
    #[serde(default)]
    pub off_days: Vec<NaiveDate>,
}

/// `PUT /v1/drivers/me/availability` — the lead's working days and the dates
/// they are off, replaced whole. Off days in the past are dropped; a year
/// ahead is the limit.
pub async fn put_my_availability(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(body): Json<AvailabilityBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let days = working_days(&body.working_days).map_err(AppError::Validation)?;
    let today = Utc::now().date_naive();
    let horizon = today + Duration::days(366);
    let mut off: Vec<NaiveDate> = body.off_days.into_iter().filter(|d| *d >= today).collect();
    off.sort_unstable();
    off.dedup();
    if off.iter().any(|d| *d > horizon) {
        return Err(AppError::Validation("Off days can be set up to a year ahead".into()));
    }
    if !state.providers.set_availability(claims.tenant_id, claims.user_id, &days, &off).await.map_err(AppError::Internal)? {
        return Err(not_found(claims.user_id));
    }
    Ok(Json(serde_json::json!({ "data": { "working_days": days, "off_days": off } })))
}

#[derive(Debug, Deserialize)]
pub struct CapacityQuery {
    pub tenant_id: Uuid,
    pub from: NaiveDate,
    pub to: NaiveDate,
}

/// `GET /v1/internal/home-leads/capacity` — per date, the home-move teams
/// working and the jobs they can take. Order-intake subtracts what it has
/// booked. Mesh-internal: not behind the JWT layer, refused by the gateway.
pub async fn internal_home_capacity(
    State(state): State<Arc<AppState>>,
    Query(q): Query<CapacityQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    if q.to < q.from || (q.to - q.from).num_days() > 60 {
        return Err(AppError::Validation("from..to is at most 60 days".into()));
    }
    let leads = state.providers.home_leads(q.tenant_id, q.from).await.map_err(AppError::Internal)?;
    Ok(Json(serde_json::json!({ "data": home_capacity(&leads, q.from, q.to) })))
}
