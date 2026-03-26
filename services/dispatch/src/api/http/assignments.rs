use axum::{extract::{Path, State}, Json};
use std::sync::Arc;
use uuid::Uuid;
use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;
use logisticos_types::{TenantId, DriverId};
use crate::{
    api::http::AppState,
    application::commands::{AutoAssignDriverCommand, AcceptAssignmentCommand, RejectAssignmentCommand},
};

pub async fn auto_assign(
    AuthClaims(claims): AuthClaims,
    Path(route_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::DISPATCH_ASSIGN);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);

    let preferred_driver_id = body.get("preferred_driver_id")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<Uuid>().ok());

    let cmd = AutoAssignDriverCommand { route_id, preferred_driver_id };
    let assignment = state.dispatch_service.auto_assign_driver(tenant_id, cmd).await?;

    Ok(Json(serde_json::json!({
        "data": {
            "assignment_id": assignment.id,
            "driver_id": assignment.driver_id,
            "route_id": assignment.route_id,
            "status": "pending"
        }
    })))
}

pub async fn accept(
    AuthClaims(claims): AuthClaims,
    Path(assignment_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<axum::http::StatusCode, AppError> {
    // Drivers accept their own assignments — no special permission needed beyond being authenticated
    // The service layer validates the assignment belongs to this driver.
    let driver_id = DriverId::from_uuid(
        claims.user_id  // user_id == driver_id for driver role users
    );
    let cmd = AcceptAssignmentCommand { assignment_id };
    state.dispatch_service.accept_assignment(&driver_id, cmd).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn reject(
    AuthClaims(claims): AuthClaims,
    Path(assignment_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> Result<axum::http::StatusCode, AppError> {
    let driver_id = DriverId::from_uuid(claims.user_id);
    let reason = body.get("reason")
        .and_then(|v| v.as_str())
        .unwrap_or("No reason provided")
        .to_string();

    let cmd = RejectAssignmentCommand { assignment_id, reason };
    state.dispatch_service.reject_assignment(&driver_id, cmd).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
