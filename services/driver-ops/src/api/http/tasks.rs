use axum::{extract::{Path, Query, State}, Json};
use chrono::NaiveDate;
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;
use logisticos_auth::{middleware::AuthClaims, require_permission};
use logisticos_errors::AppError;
use logisticos_types::{TenantId, DriverId};
use crate::{
    api::http::AppState,
    application::commands::{StartTaskCommand, CompleteTaskCommand, FailTaskCommand, AttemptTaskCommand},
};

pub async fn list_my_tasks(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let driver_id = DriverId::from_uuid(claims.user_id);
    let tasks = state.task_service.list_my_tasks(&driver_id).await?;
    Ok(Json(serde_json::json!({ "data": tasks })))
}

#[derive(Debug, Deserialize)]
pub struct ManifestQuery {
    /// Target date in YYYY-MM-DD. Required; we intentionally do not default
    /// to "today" so the caller is forced to think about timezone semantics.
    pub date: NaiveDate,
    /// Optional — when set, only include drivers with `drivers.carrier_id
    /// = carrier_id`. Falls back to the entire tenant when omitted.
    #[serde(default)]
    pub carrier_id: Option<Uuid>,
}

/// `GET /v1/tasks/manifest?date=YYYY-MM-DD&carrier_id=<uuid>`
///
/// Aggregated daily manifest per (driver, task_type). Used by the partner
/// portal's /manifests page; admins can call with no carrier_id to see the
/// whole-tenant view.
pub async fn list_manifest(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Query(q): Query<ManifestQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let entries = state.task_service.list_manifest(&tenant_id, q.carrier_id, q.date).await?;
    Ok(Json(serde_json::json!({
        "data":       entries,
        "date":       q.date,
        "carrier_id": q.carrier_id,
    })))
}

/// GET /v1/tasks/history
///
/// Returns completed and failed tasks for the authenticated driver.
/// Useful for the driver app's history tab and the admin audit view.
pub async fn list_task_history(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let driver_id = DriverId::from_uuid(claims.user_id);
    let all_tasks = state.task_service.list_all_tasks(&driver_id).await?;
    // Filter to terminal statuses (completed / failed / cancelled)
    let history: Vec<_> = all_tasks
        .into_iter()
        .filter(|t| {
            matches!(
                t.status.as_str(),
                "completed" | "failed" | "cancelled"
            )
        })
        .collect();
    let count = history.len();
    Ok(Json(serde_json::json!({ "data": history, "count": count })))
}

/// `GET /v1/drivers/:id/tasks/history`
///
/// Admin variant — returns completed/failed/cancelled tasks for any driver by UUID.
/// Requires `FLEET_VIEW` permission (dispatcher / admin role).
pub async fn admin_list_driver_task_history(
    AuthClaims(claims): AuthClaims,
    Path(driver_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::FLEET_VIEW);
    let driver_id = DriverId::from_uuid(driver_id);
    let all_tasks = state.task_service.list_all_tasks(&driver_id).await?;
    let history: Vec<_> = all_tasks
        .into_iter()
        .filter(|t| {
            matches!(
                t.status.as_str(),
                "completed" | "failed" | "cancelled"
            )
        })
        .collect();
    let count = history.len();
    Ok(Json(serde_json::json!({ "data": history, "count": count })))
}

pub async fn start_task(
    AuthClaims(claims): AuthClaims,
    Path(task_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<axum::http::StatusCode, AppError> {
    let driver_id = DriverId::from_uuid(claims.user_id);
    state.task_service.start_task(&driver_id, StartTaskCommand { task_id }).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn complete_task(
    AuthClaims(claims): AuthClaims,
    Path(task_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<CompleteTaskCommand>,
) -> Result<axum::http::StatusCode, AppError> {
    let driver_id = DriverId::from_uuid(claims.user_id);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    // Use the path parameter as authoritative task_id (ignore body's task_id if any)
    let cmd = CompleteTaskCommand { task_id, ..cmd };
    state.task_service.complete_task(&driver_id, &tenant_id, cmd).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn fail_task(
    AuthClaims(claims): AuthClaims,
    Path(task_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<FailTaskCommand>,
) -> Result<axum::http::StatusCode, AppError> {
    let driver_id = DriverId::from_uuid(claims.user_id);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let cmd = FailTaskCommand { task_id, ..cmd };
    state.task_service.fail_task(&driver_id, &tenant_id, cmd).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// `PUT /v1/tasks/:id/attempt`
///
/// Driver knocked on the door but couldn't deliver (nobody home, access denied, etc.).
/// Records the attempt, resets the task to Pending for re-delivery, and emits
/// DELIVERY_ATTEMPTED so engagement can notify the customer.
pub async fn attempt_task(
    AuthClaims(claims): AuthClaims,
    Path(task_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<AttemptTaskCommand>,
) -> Result<axum::http::StatusCode, AppError> {
    let driver_id = DriverId::from_uuid(claims.user_id);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let cmd = AttemptTaskCommand { task_id, ..cmd };
    state.task_service.attempt_task(&driver_id, &tenant_id, cmd).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
