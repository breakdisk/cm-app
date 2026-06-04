use axum::{extract::{Path, Query as AxumQuery, State}, Json};
use std::sync::Arc;
use serde::Deserialize;
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
    carrier_id: Option<Uuid>,
    hub_id:     Option<Uuid>,
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
            carrier_id: d.carrier_id,
            hub_id:     d.hub_id,
            created_at: d.created_at,
            updated_at: d.updated_at,
        }
    }
}

#[derive(serde::Deserialize, Default)]
pub struct ListDriversQuery {
    pub hub_id: Option<uuid::Uuid>,
    pub search: Option<String>,
}

pub async fn list_drivers(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    AxumQuery(q): AxumQuery<ListDriversQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::FLEET_VIEW);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let mut drivers = state.driver_service.list_by_tenant(&tenant_id).await?;

    // Filter by hub_id when provided (Hub Staff tab — assigned scanners).
    if let Some(hub_id) = q.hub_id {
        drivers.retain(|d| d.hub_id == Some(hub_id));
    }

    // Filter by search when provided (Hub Staff assign-modal search).
    if let Some(search) = &q.search {
        let s = search.to_lowercase();
        drivers.retain(|d| {
            d.first_name.to_lowercase().contains(&s)
                || d.last_name.to_lowercase().contains(&s)
                || d.phone.contains(&s)
        });
    }

    let dtos: Vec<DriverDto> = drivers.iter().map(DriverDto::from).collect();
    Ok(Json(serde_json::json!({ "data": dtos })))
}

/// GET /v1/drivers/summary — aggregated KPI strip for the admin roster page.
/// Returns driver status counts (online/offline/break) + today's task throughput.
pub async fn get_summary(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::FLEET_VIEW);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);

    let (drivers, task_summary) = tokio::try_join!(
        state.driver_service.list_by_tenant(&tenant_id),
        state.task_service.tenant_summary(&tenant_id),
    )?;

    let online    = drivers.iter().filter(|d| d.status != DriverStatus::Offline).count() as i64;
    let idle      = drivers.iter().filter(|d| d.status == DriverStatus::Available).count() as i64;
    let on_break  = drivers.iter().filter(|d| d.status == DriverStatus::OnBreak).count() as i64;
    let offline   = drivers.iter().filter(|d| d.status == DriverStatus::Offline).count() as i64;

    Ok(Json(serde_json::json!({
        "data": {
            "online": online,
            "idle": idle,
            "on_break": on_break,
            "offline": offline,
            "total_tasks_assigned": task_summary.total_assigned,
            "total_tasks_completed": task_summary.total_completed,
            "total_tasks_failed": task_summary.total_failed,
            "total_cod_collected": task_summary.cod_collected_cents,
        }
    })))
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

/// `GET /v1/drivers/me`
///
/// Returns the authenticated driver's own profile, including `hub_id` when
/// assigned as a hub scanner. Called by the Android app after OTP login and
/// on every HomeScreen foreground to detect hub assignment changes.
///
/// Note: driver_id == user_id by design in this system.
pub async fn get_me_driver(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let driver = state.driver_service.find_by_user_id(claims.user_id)
        .await?
        .ok_or(AppError::NotFound { resource: "Driver", id: claims.user_id.to_string() })?;
    // Tenant isolation
    if driver.tenant_id.inner() != claims.tenant_id {
        return Err(AppError::NotFound { resource: "Driver", id: claims.user_id.to_string() });
    }
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

/// DELETE /v1/drivers/:id — hard-delete a driver profile.
///
/// Guards (enforced in DriverService::delete_driver):
/// - Driver must be Offline (cannot delete an active courier mid-shift).
/// - Driver must have no active route (prevents orphaning a live assignment).
///
/// The corresponding identity.users row is NOT removed — deactivate that
/// separately in the identity service if the person should also lose login access.
/// Task history rows are preserved (FK → SET NULL keeps the audit trail intact).
pub async fn delete_driver(
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<axum::http::StatusCode, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::FLEET_MANAGE);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let driver_id = DriverId::from_uuid(id);
    state.driver_service.delete_driver(&tenant_id, &driver_id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// GET /v1/drivers/:id/location
///
/// Returns the most-recent known GPS coordinates for a driver.
/// Used by the AI layer's `get_driver_location` MCP tool.
/// Returns 404 if the driver has never broadcast a location.
pub async fn get_driver_location(
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

    let (lat, lng) = match driver.current_location {
        Some(loc) => (loc.lat, loc.lng),
        None => return Err(AppError::NotFound {
            resource: "DriverLocation",
            id: id.to_string(),
        }),
    };

    Ok(Json(serde_json::json!({
        "data": {
            "driver_id":       id,
            "lat":             lat,
            "lng":             lng,
            "last_updated_at": driver.last_location_at,
            "status":          status_str(driver.status),
            "is_online":       driver.status != DriverStatus::Offline,
        }
    })))
}

#[derive(Debug, Deserialize)]
pub struct SendInstructionRequest {
    /// e.g. "return_to_hub", "call_support", "pickup_at_hub", "custom"
    pub instruction_type: String,
    pub message: String,
}

/// POST /v1/drivers/:id/instructions
///
/// Sends an operational instruction to a driver — delivered via FCM push (if
/// the driver app has registered a token) and broadcast to the WebSocket roster
/// so the dispatch console can show confirmation without polling.
/// `:id` is the driver's identity user_id (the UUID shown in the portal).
pub async fn send_driver_instruction(
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(req): Json<SendInstructionRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::FLEET_MANAGE);

    if req.instruction_type.is_empty() || req.message.is_empty() {
        return Err(AppError::Validation(
            "instruction_type and message are required".into(),
        ));
    }

    // Fire FCM push — best-effort, non-blocking
    if let Some(fcm) = &state.fcm {
        let fcm = fcm.clone();
        let itype = req.instruction_type.clone();
        let msg = req.message.clone();
        tokio::spawn(async move {
            fcm.notify_driver_instruction(id, &itype, &msg).await;
        });
    }

    // Broadcast to WebSocket subscribers (dispatch console, monitoring dashboards)
    let _ = state.roster_tx.send(RosterEvent::Instruction {
        driver_id:        id,
        tenant_id:        claims.tenant_id,
        instruction_type: req.instruction_type.clone(),
        message:          req.message.clone(),
    });

    tracing::info!(
        driver_id = %id,
        instruction_type = %req.instruction_type,
        "Instruction sent to driver"
    );

    Ok(Json(serde_json::json!({
        "data": {
            "driver_id":        id,
            "instruction_type": req.instruction_type,
            "message":          req.message,
            "delivered":        state.fcm.is_some(),
        }
    })))
}

/// Admin: POST /v1/drivers/:id/cancel-tasks
///
/// Cancels all `pending` and `in_progress` tasks for the given driver,
/// returning them to a clean slate so auto-dispatch can reassign them.
/// `:id` is the driver's identity user_id (JWT sub / UUID shown in portal).
///
/// Also call `POST /v1/drivers/:id/cancel-assignment` on the dispatch service
/// to unblock the driver from receiving new auto-dispatches.
pub async fn cancel_driver_tasks(
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::FLEET_MANAGE);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);

    let cancelled = state.task_service
        .admin_cancel_driver_tasks(id, &tenant_id)
        .await?;

    Ok(Json(serde_json::json!({
        "data": {
            "driver_user_id": id,
            "tasks_cancelled": cancelled
        }
    })))
}
