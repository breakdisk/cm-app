use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::rbac::permissions;
use logisticos_errors::AppError;

use crate::application::services::{
    CompleteMaintenanceCommand, CreateVehicleCommand, ScheduleMaintenanceCommand,
};
use crate::domain::entities::{Vehicle, VehicleStatus, VehicleType};
use crate::AppState;

// ── Router ────────────────────────────────────────────────────────────────────

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/fleet/vehicles",                            get(list_vehicles).post(create_vehicle))
        .route("/v1/fleet/summary",                             get(fleet_summary))
        .route("/v1/fleet/vehicles/maintenance-alerts",         get(maintenance_alerts))
        .route("/v1/fleet/vehicles/:id",                        get(get_vehicle).delete(decommission_vehicle))
        .route("/v1/fleet/vehicles/:id/assign-driver",          post(assign_driver))
        .route("/v1/fleet/vehicles/:id/unassign-driver",        post(unassign_driver))
        .route("/v1/fleet/vehicles/:id/maintenance",            post(schedule_maintenance))
        .route("/v1/fleet/vehicles/:id/maintenance/complete",   post(complete_maintenance))
}

// ── API response projection ───────────────────────────────────────────────────
//
// Domain `Vehicle` → wire shape the portals and apps expect.
// Fields the fleet service doesn't own (fuel_pct, km_today, location, GPS) are
// zero/null here; the telematics add-on overlays them via the /v2/telemetry
// endpoint once that service ships.

#[derive(Debug, Serialize)]
struct VehicleApiResponse {
    id:              String,
    plate:           String,
    #[serde(rename = "type")]
    vehicle_type:    VehicleType,
    driver_id:       Option<String>,
    driver_name:     Option<String>,
    status:          String,
    fuel_pct:        f64,
    km_today:        f64,
    location:        Option<String>,
    lat:             Option<f64>,
    lng:             Option<f64>,
    next_service_km: i64,
    alerts:          Vec<String>,
}

impl From<Vehicle> for VehicleApiResponse {
    fn from(v: Vehicle) -> Self {
        let status = match &v.status {
            VehicleStatus::Active if v.assigned_driver_id.is_some() => "active",
            VehicleStatus::Active                                    => "idle",
            VehicleStatus::UnderMaintenance                         => "maintenance",
            VehicleStatus::Decommissioned                           => "offline",
        };
        let alerts: Vec<String> = v.maintenance_history
            .iter()
            .filter(|r| r.is_overdue())
            .map(|_| "Overdue maintenance".to_owned())
            .collect();
        Self {
            id:              v.id.inner().to_string(),
            plate:           v.plate_number,
            vehicle_type:    v.vehicle_type,
            driver_id:       v.assigned_driver_id.map(|u| u.to_string()),
            driver_name:     None,
            status:          status.to_owned(),
            fuel_pct:        0.0,
            km_today:        0.0,
            location:        None,
            lat:             None,
            lng:             None,
            next_service_km: 0,
            alerts,
        }
    }
}

// ── Query structs ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ListQuery {
    page:     Option<u32>,
    per_page: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct AlertsQuery {
    within_days: Option<i64>,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn list_vehicles(
    State(state): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<ListQuery>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::FLEET_READ)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);

    let page     = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(50).clamp(1, 200);
    let limit    = per_page as i64;
    let offset   = ((page - 1) * per_page) as i64;

    let (vehicles, total) = tokio::try_join!(
        state.fleet_svc.list(&tenant_id, limit, offset),
        state.fleet_svc.count(&tenant_id),
    )?;

    let total_pages = ((total as f64) / (per_page as f64)).ceil() as i64;
    let data: Vec<VehicleApiResponse> = vehicles.into_iter().map(VehicleApiResponse::from).collect();

    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({
        "data":        data,
        "total":       total,
        "page":        page,
        "per_page":    per_page,
        "total_pages": total_pages,
    }))))
}

async fn create_vehicle(
    State(state): State<AppState>,
    claims: AuthClaims,
    Json(cmd): Json<CreateVehicleCommand>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::FLEET_MANAGE)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let vehicle   = state.fleet_svc.create(&tenant_id, cmd).await?;
    let resp      = VehicleApiResponse::from(vehicle);
    Ok::<_, AppError>((StatusCode::CREATED, Json(serde_json::json!({ "data": resp }))))
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
    let resp = VehicleApiResponse::from(vehicle);
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({ "data": resp }))))
}

async fn fleet_summary(
    State(state): State<AppState>,
    claims: AuthClaims,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::FLEET_READ)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let summary   = state.fleet_svc.summary(&tenant_id).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({ "data": summary }))))
}

async fn decommission_vehicle(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::FLEET_MANAGE)?;
    let vehicle = state.fleet_svc.decommission(id).await?;
    let resp    = VehicleApiResponse::from(vehicle);
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({ "data": resp }))))
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
    let resp    = VehicleApiResponse::from(vehicle);
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({ "data": resp }))))
}

async fn unassign_driver(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::FLEET_MANAGE)?;
    let vehicle = state.fleet_svc.unassign_driver(id).await?;
    let resp    = VehicleApiResponse::from(vehicle);
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({ "data": resp }))))
}

async fn schedule_maintenance(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(cmd): Json<ScheduleMaintenanceCommand>,
) -> impl IntoResponse {
    claims.require_permission(permissions::FLEET_MANAGE)?;
    let vehicle = state.fleet_svc.schedule_maintenance(id, cmd).await?;
    let resp    = VehicleApiResponse::from(vehicle);
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({ "data": resp }))))
}

async fn complete_maintenance(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(cmd): Json<CompleteMaintenanceCommand>,
) -> impl IntoResponse {
    claims.require_permission(permissions::FLEET_MANAGE)?;
    let vehicle = state.fleet_svc.complete_maintenance(id, cmd).await?;
    let resp    = VehicleApiResponse::from(vehicle);
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({ "data": resp }))))
}

async fn maintenance_alerts(
    State(state): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<AlertsQuery>,
) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::FLEET_READ)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let vehicles  = state.fleet_svc.maintenance_due_alerts(&tenant_id, q.within_days.unwrap_or(7)).await?;
    let data: Vec<VehicleApiResponse> = vehicles.into_iter().map(VehicleApiResponse::from).collect();
    let count = data.len();
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({ "data": data, "count": count }))))
}
