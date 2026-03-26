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
