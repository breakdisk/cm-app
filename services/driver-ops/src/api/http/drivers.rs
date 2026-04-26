use axum::{extract::{Path, State}, Json};
use std::sync::Arc;
use uuid::Uuid;
use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::require_permission;
use logisticos_errors::AppError;
use logisticos_types::{TenantId, DriverId};
use crate::{
    api::http::{AppState, RosterEvent},
    application::commands::{RegisterDriverCommand, UpdateDriverCommand},
    domain::entities::{Driver, DriverStatus},
};

/// Response shape consumed by the partner-portal drivers page.
/// Derives `is_online` from status so the UI doesn't need to know the status taxonomy.
#[derive(Debug, serde::Serialize)]
struct DriverDto {
    id: Uuid,
    user_id: Uuid,
    first_name: String,
    last_name: String,
    phone: String,
    status: String,
    is_online: bool,
    driver_type: String,
    per_delivery_rate_cents: i32,
    cod_commission_rate_bps: i32,
    zone: Option<String>,
    vehicle_type: Option<String>,
    lat: Option<f64>,
    lng: Option<f64>,
    last_location_at: Option<chrono::DateTime<chrono::Utc>>,
    active_route_id: Option<Uuid>,
    is_active: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

fn status_str(s: DriverStatus) -> &'static str {
    match s {
        DriverStatus::Offline    => "offline",
        DriverStatus::Available  => "available",
        DriverStatus::EnRoute    => "en_route",
        DriverStatus::Delivering => "delivering",
        DriverStatus::Returning  => "returning",
        DriverStatus::OnBreak    => "on_break",
    }
}

fn driver_type_str(d: &Driver) -> &'static str {
    use crate::domain::entities::DriverType;
    match d.driver_type {
        DriverType::FullTime => "full_time",
        DriverType::PartTime => "part_time",
    }
}

impl From<&Driver> for DriverDto {
    fn from(d: &Driver) -> Self {
        DriverDto {
            id: d.id.inner(),
            user_id: d.user_id,
            first_name: d.first_name.clone(),
            last_name: d.last_name.clone(),
            phone: d.phone.clone(),
            status: status_str(d.status).to_string(),
            is_online: d.status != DriverStatus::Offline,
            driver_type: driver_type_str(d).to_string(),
            per_delivery_rate_cents: d.per_delivery_rate_cents,
            cod_commission_rate_bps: d.cod_commission_rate_bps,
            zone: d.zone.clone(),
            vehicle_type: d.vehicle_type.clone(),
            lat: d.current_location.map(|c| c.lat),
            lng: d.current_location.map(|c| c.lng),
            last_location_at: d.last_location_at,
            active_route_id: d.active_route_id,
            is_active: d.is_active,
            created_at: d.created_at,
            updated_at: d.updated_at,
        }
    }
}

pub async fn list_drivers(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::FLEET_VIEW);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let drivers = state.driver_service.list_by_tenant(&tenant_id).await?;
    let dtos: Vec<DriverDto> = drivers.iter().map(DriverDto::from).collect();
    Ok(Json(serde_json::json!({ "data": dtos })))
}

pub async fn get_driver(
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::FLEET_VIEW);
    let driver_id = DriverId::from_uuid(id);
    let driver = state.driver_service.get(&driver_id).await?;
    // Tenant isolation
    if driver.tenant_id.inner() != claims.tenant_id {
        return Err(AppError::NotFound { resource: "Driver", id: id.to_string() });
    }
    Ok(Json(serde_json::json!({ "data": DriverDto::from(&driver) })))
}

pub async fn register_driver(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<RegisterDriverCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::FLEET_MANAGE);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let driver = state.driver_service.register(tenant_id, cmd).await?;
    Ok(Json(serde_json::json!({ "data": { "driver_id": driver.id } })))
}

pub async fn update_driver(
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<UpdateDriverCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::FLEET_MANAGE);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let driver_id = DriverId::from_uuid(id);
    let driver = state.driver_service.update(&tenant_id, &driver_id, cmd).await?;
    Ok(Json(serde_json::json!({ "data": DriverDto::from(&driver) })))
}

pub async fn go_online(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<axum::http::StatusCode, AppError> {
    let driver_id = DriverId::from_uuid(claims.user_id);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    state.location_service.go_online(&driver_id, &tenant_id).await?;
    let _ = state.roster_tx.send(RosterEvent::StatusChanged {
        driver_id: claims.user_id,
        tenant_id: claims.tenant_id,
        status: "available".into(),
        is_online: true,
        active_route_id: None,
    });
    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn go_offline(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<axum::http::StatusCode, AppError> {
    let driver_id = DriverId::from_uuid(claims.user_id);
    state.location_service.go_offline(&driver_id).await?;
    let _ = state.roster_tx.send(RosterEvent::StatusChanged {
        driver_id: claims.user_id,
        tenant_id: claims.tenant_id,
        status: "offline".into(),
        is_online: false,
        active_route_id: None,
    });
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[derive(Debug, serde::Deserialize)]
pub struct SetStatusRequest {
    /// "available" | "offline" | "on_break". Other transitions (en_route /
    /// delivering / returning) are state-machine driven and not exposed
    /// here — admins shouldn't manually flip a driver into mid-trip states.
    pub status: String,
}

/// Admin override: PUT /v1/drivers/:id/status — flip a driver's status
/// directly, e.g. ops marks a driver offline who walked off shift without
/// toggling the app, or pulls an idle driver out of the auto-dispatch pool
/// for testing. Authority lives with admin (FLEET_MANAGE) only —
/// dispatchers are read-only on driver state per ADR-0003 RBAC.
pub async fn set_driver_status(
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(req): Json<SetStatusRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::FLEET_MANAGE);

    let new_status = match req.status.as_str() {
        "available" => DriverStatus::Available,
        "offline"   => DriverStatus::Offline,
        "on_break"  => DriverStatus::OnBreak,
        other => return Err(AppError::Validation(format!(
            "Status '{other}' is not admin-settable. Allowed: available, offline, on_break."
        ))),
    };

    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let driver_id = DriverId::from_uuid(id);
    let driver = state.driver_service
        .set_status(&tenant_id, &driver_id, new_status, claims.user_id)
        .await?;

    let _ = state.roster_tx.send(RosterEvent::StatusChanged {
        driver_id: id,
        tenant_id: claims.tenant_id,
        status:    status_str(new_status).into(),
        is_online: matches!(new_status, DriverStatus::Available | DriverStatus::OnBreak),
        active_route_id: driver.active_route_id,
    });

    Ok(Json(serde_json::json!({ "data": DriverDto::from(&driver) })))
}
