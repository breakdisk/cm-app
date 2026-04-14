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

use crate::application::services::{
    CompleteMaintenanceCommand, CreateVehicleCommand, ScheduleMaintenanceCommand,
};
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/vehicles",                              get(list_vehicles).post(create_vehicle))
        .route("/v1/vehicles/maintenance-alerts",           get(maintenance_alerts))
        .route("/v1/vehicles/:id",                          get(get_vehicle).delete(decommission_vehicle))
        .route("/v1/vehicles/:id/assign-driver",            post(assign_driver))
        .route("/v1/vehicles/:id/unassign-driver",          post(unassign_driver))
        .route("/v1/vehicles/:id/maintenance",              post(schedule_maintenance))
        .route("/v1/vehicles/:id/maintenance/complete",     post(complete_maintenance))
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    limit:  Option<i64>,
    offset: Option<i64>,
}

async fn list_vehicles(
    State(state): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<ListQuery>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::FLEET_READ)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let vehicles = state.fleet_svc.list(&tenant_id, q.limit.unwrap_or(50), q.offset.unwrap_or(0)).await?;
    let count = vehicles.len();
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"vehicles": vehicles, "count": count}))))
}

async fn create_vehicle(
    State(state): State<AppState>,
    claims: AuthClaims,
    Json(cmd): Json<CreateVehicleCommand>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::FLEET_MANAGE)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let vehicle = state.fleet_svc.create(&tenant_id, cmd).await?;
    Ok::<_, AppError>((StatusCode::CREATED, Json(vehicle)))
}

async fn get_vehicle(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::FLEET_READ)?;
    let vehicle = state.fleet_svc.get(id).await?;
    if vehicle.tenant_id != TenantId::from_uuid(claims.tenant_id) {
        return Err(AppError::Forbidden { resource: "vehicle".to_owned() });
    }
    Ok::<_, AppError>((StatusCode::OK, Json(vehicle)))
}

async fn decommission_vehicle(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::FLEET_MANAGE)?;
    let vehicle = state.fleet_svc.decommission(id).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(vehicle)))
}

#[derive(Debug, Deserialize)]
struct AssignDriverBody { driver_id: Uuid }

async fn assign_driver(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(body): Json<AssignDriverBody>,
) -> impl IntoResponse {
    claims.require_permission(permissions::FLEET_MANAGE)?;
    let vehicle = state.fleet_svc.assign_driver(id, body.driver_id).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(vehicle)))
}

async fn unassign_driver(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::FLEET_MANAGE)?;
    let vehicle = state.fleet_svc.unassign_driver(id).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(vehicle)))
}

async fn schedule_maintenance(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(cmd): Json<ScheduleMaintenanceCommand>,
) -> impl IntoResponse {
    claims.require_permission(permissions::FLEET_MANAGE)?;
    let vehicle = state.fleet_svc.schedule_maintenance(id, cmd).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(vehicle)))
}

async fn complete_maintenance(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(cmd): Json<CompleteMaintenanceCommand>,
) -> impl IntoResponse {
    claims.require_permission(permissions::FLEET_MANAGE)?;
    let vehicle = state.fleet_svc.complete_maintenance(id, cmd).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(vehicle)))
}

#[derive(Debug, Deserialize)]
struct AlertsQuery { within_days: Option<i64> }

async fn maintenance_alerts(
    State(state): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<AlertsQuery>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::FLEET_READ)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let vehicles = state.fleet_svc.maintenance_due_alerts(&tenant_id, q.within_days.unwrap_or(7)).await?;
    let count = vehicles.len();
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"alerts": vehicles, "count": count}))))
}
