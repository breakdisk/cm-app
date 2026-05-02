use axum::{extract::{Path, State}, http::StatusCode, Json};
use std::sync::Arc;
use uuid::Uuid;
use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::require_permission;
use logisticos_errors::AppError;
use logisticos_types::{DriverId, TenantId};
use crate::{api::http::AppState, application::commands::QuickDispatchCommand};

pub async fn quick_dispatch(
    AuthClaims(claims): AuthClaims,
    Path(shipment_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::DISPATCH_ASSIGN);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);

    let preferred_driver_id = body.get("preferred_driver_id")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<Uuid>().ok());

    let cmd = QuickDispatchCommand { shipment_id, preferred_driver_id };
    let assignment = state.dispatch_service.quick_dispatch(tenant_id, cmd).await?;

    Ok(Json(serde_json::json!({
        "data": {
            "assignment_id": assignment.id,
            "driver_id": assignment.driver_id.inner(),
            "status": "pending"
        }
    })))
}

/// Internal: POST /v1/internal/shipments/:shipment_id/requeue
///
/// Re-queues a delivery-failed shipment back to `pending` so the dispatch
/// engine can assign a new driver. Called by the business-logic ECA engine
/// when a `DELIVERY_FAILED` event fires the failed-delivery rule.
/// No auth required — only callable from inside the service mesh.
pub async fn requeue_shipment(
    Path(shipment_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<StatusCode, AppError> {
    state.queue_repo
        .reset_to_pending(shipment_id)
        .await
        .map_err(AppError::Internal)?;
    tracing::info!(shipment_id = %shipment_id, "Shipment requeued for dispatch retry via internal endpoint");
    Ok(StatusCode::NO_CONTENT)
}

/// Admin: POST /v1/drivers/:id/cancel-assignment
///
/// Cancels any active (`pending` / `accepted`) dispatch assignment for the
/// driver, re-entering them into the auto-dispatch candidate pool.
/// `:id` is the driver's `drivers.id` UUID (from the dispatch service's
/// driver profile, visible in the dispatch queue `/v1/drivers` list).
pub async fn cancel_driver_assignment(
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::DISPATCH_ASSIGN);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let driver_id = DriverId::from_uuid(id);

    let cancelled = state.dispatch_service
        .admin_cancel_driver_assignment(driver_id, &tenant_id)
        .await?;

    Ok(Json(serde_json::json!({
        "data": {
            "driver_id": id,
            "assignment_cancelled": cancelled
        }
    })))
}
