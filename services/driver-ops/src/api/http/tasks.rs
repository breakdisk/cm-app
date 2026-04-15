use axum::{extract::{Path, State}, Json};
use std::sync::Arc;
use uuid::Uuid;
use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;
use logisticos_types::{TenantId, DriverId};
use crate::{
    api::http::AppState,
    application::commands::{StartTaskCommand, CompleteTaskCommand, FailTaskCommand},
};

pub async fn list_my_tasks(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let driver_id = DriverId::from_uuid(claims.user_id);
    let tasks = state.task_service.list_my_tasks(&driver_id).await?;
    Ok(Json(serde_json::json!({ "data": tasks })))
}

pub async fn start_task(
    AuthClaims(claims): AuthClaims,
    Path(task_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<axum::http::StatusCode, AppError> {
    let driver_id = DriverId::from_uuid(claims.user_id);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    state.task_service.start_task(&driver_id, &tenant_id, StartTaskCommand { task_id }).await?;
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

// ── Admin/dispatcher overrides ────────────────────────────────────────
// These endpoints let an admin or dispatcher act on behalf of any driver,
// for ops console state manipulation and recovery when a driver's device
// is offline. Require `admin` or `dispatcher` role.

fn require_ops_role(claims: &logisticos_auth::claims::Claims) -> Result<(), AppError> {
    let allowed = claims.roles.iter().any(|r| r == "admin" || r == "dispatcher");
    if allowed {
        Ok(())
    } else {
        Err(AppError::Forbidden { resource: "admin task operation".into() })
    }
}

pub async fn admin_list_tasks(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_ops_role(&claims)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let tasks = state.task_service.list_tenant_tasks(&tenant_id).await?;
    Ok(Json(serde_json::json!({ "data": tasks })))
}

pub async fn admin_start_task(
    AuthClaims(claims): AuthClaims,
    Path(task_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<axum::http::StatusCode, AppError> {
    require_ops_role(&claims)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    state.task_service.admin_start_task(&tenant_id, task_id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[derive(serde::Deserialize, Default)]
pub struct AdminCompleteTaskBody {
    pub pod_id: Option<Uuid>,
}

pub async fn admin_complete_task(
    AuthClaims(claims): AuthClaims,
    Path(task_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    body: Option<Json<AdminCompleteTaskBody>>,
) -> Result<axum::http::StatusCode, AppError> {
    require_ops_role(&claims)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let pod_id = body.and_then(|Json(b)| b.pod_id);
    state.task_service.admin_complete_task(&tenant_id, task_id, pod_id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
