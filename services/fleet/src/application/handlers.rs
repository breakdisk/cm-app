use axum::{extract::{Path, State}, http::StatusCode, Json};
use std::sync::Arc;
use uuid::Uuid;

use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::rbac::permissions;
use logisticos_errors::AppError;
use logisticos_types::TenantId;

use crate::{
    application::services::{CompleteMaintenanceCommand, CreateVehicleCommand, FleetService, ScheduleMaintenanceCommand},
    domain::entities::VehicleStatus,
    AppState,
};

#[derive(serde::Serialize)]
struct VehicleDto {
    id: Uuid,
    plate_number: String,
    vehicle_type: String,
    make: String,
    model: String,
    year: u16,
    color: String,
    status: String,
    assigned_driver_id: Option<Uuid>,
    odometer_km: i32,
    next_maintenance_due: Option<String>,
}

pub async fn list_vehicles(
    State(state): State<Arc<AppState>>,
    claims: AuthClaims,
) -> Result<Json<serde_json::Value>, AppError> {
    claims.require_permission(permissions::FLEET_VIEW)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let vehicles = state.fleet_svc.list(&tenant_id, 50, 0).await?;
    let dtos: Vec<VehicleDto> = vehicles.iter().map(|v| VehicleDto {
        id: v.id.inner(),
        plate_number: v.plate_number.clone(),
        vehicle_type: format!("{:?}", v.vehicle_type),
        make: v.make.clone(),
        model: v.model.clone(),
        year: v.year,
        color: v.color.clone(),
        status: format!("{:?}", v.status),
        assigned_driver_id: v.assigned_driver_id,
        odometer_km: v.odometer_km,
        next_maintenance_due: v.next_maintenance_due.map(|d| d.to_string()),
    }).collect();
    Ok(Json(serde_json::json!({ "data": dtos })))
}

pub async fn get_vehicle(
    State(state): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    claims.require_permission(permissions::FLEET_VIEW)?;
    let vehicle = state.fleet_svc.get(id).await?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    if vehicle.tenant_id.inner() != tenant_id.inner() {
        return Err(AppError::NotFound { resource: "Vehicle", id: id.to_string() });
    }
    let dto = VehicleDto {
        id: vehicle.id.inner(),
        plate_number: vehicle.plate_number.clone(),
        vehicle_type: format!("{:?}", vehicle.vehicle_type),
        make: vehicle.make.clone(),
        model: vehicle.model.clone(),
        year: vehicle.year,
        color: vehicle.color.clone(),
        status: format!("{:?}", vehicle.status),
        assigned_driver_id: vehicle.assigned_driver_id,
        odometer_km: vehicle.odometer_km,
        next_maintenance_due: vehicle.next_maintenance_due.map(|d| d.to_string()),
    };
    Ok(Json(serde_json::json!({ "data": dto })))
}

pub async fn create_vehicle(
    State(state): State<Arc<AppState>>,
    claims: AuthClaims,
    Json(cmd): Json<CreateVehicleCommand>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    claims.require_permission(permissions::FLEET_MANAGE)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let vehicle = state.fleet_svc.create(&tenant_id, cmd).await?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "data": { "vehicle_id": vehicle.id.inner() } }))))
}

pub async fn assign_driver(
    State(state): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    claims.require_permission(permissions::FLEET_MANAGE)?;
    let driver_id = body.get("driver_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| AppError::Validation("driver_id is required".into()))?;
    let vehicle = state.fleet_svc.assign_driver(id, driver_id).await?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    if vehicle.tenant_id.inner() != tenant_id.inner() {
        return Err(AppError::NotFound { resource: "Vehicle", id: id.to_string() });
    }
    Ok(Json(serde_json::json!({ "data": { "status": "assigned" } })))
}

pub async fn schedule_maintenance(
    State(state): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(cmd): Json<ScheduleMaintenanceCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    claims.require_permission(permissions::FLEET_MANAGE)?;
    let vehicle = state.fleet_svc.schedule_maintenance(id, cmd).await?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    if vehicle.tenant_id.inner() != tenant_id.inner() {
        return Err(AppError::NotFound { resource: "Vehicle", id: id.to_string() });
    }
    Ok(Json(serde_json::json!({ "data": { "status": "maintenance_scheduled" } })))
}

pub async fn complete_maintenance(
    State(state): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(cmd): Json<CompleteMaintenanceCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    claims.require_permission(permissions::FLEET_MANAGE)?;
    let vehicle = state.fleet_svc.complete_maintenance(id, cmd).await?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    if vehicle.tenant_id.inner() != tenant_id.inner() {
        return Err(AppError::NotFound { resource: "Vehicle", id: id.to_string() });
    }
    Ok(Json(serde_json::json!({ "data": { "status": "maintenance_completed" } })))
}
