use axum::{extract::{Path, State}, Json};
use std::sync::Arc;
use uuid::Uuid;
use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::require_permission;
use logisticos_errors::AppError;
use logisticos_types::{TenantId, DriverId};
use crate::{api::http::AppState, application::commands::RegisterDriverCommand};

pub async fn list_drivers(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::FLEET_VIEW);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let drivers = state.driver_service.list_by_tenant(&tenant_id).await?;
    Ok(Json(serde_json::json!({ "data": drivers })))
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
    Ok(Json(serde_json::json!({ "data": driver })))
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

pub async fn go_online(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<axum::http::StatusCode, AppError> {
    let driver_id = DriverId::from_uuid(claims.user_id);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    state.location_service.go_online(&driver_id, &tenant_id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn go_offline(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<axum::http::StatusCode, AppError> {
    let driver_id = DriverId::from_uuid(claims.user_id);
    state.location_service.go_offline(&driver_id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
