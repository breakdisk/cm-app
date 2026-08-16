use axum::{extract::{Path, Query, State}, http::StatusCode, Json};
use std::sync::Arc;
use serde::Deserialize;
use uuid::Uuid;
use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::require_permission;
use logisticos_errors::AppError;
use logisticos_types::{Coordinates, DriverId, TenantId};
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

/// Internal (no JWT): POST /v1/internal/shipments/:shipment_id/assign
///
/// Called by the AI layer's `assign_driver` MCP tool within the service mesh.
/// Accepts an optional `driver_id` to override heuristic selection.
/// Istio mTLS gates caller identity — no JWT required.
pub async fn internal_assign(
    Path(shipment_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    let tenant_id_str = body["tenant_id"].as_str().unwrap_or("");
    let tenant_id = tenant_id_str
        .parse::<Uuid>()
        .map(TenantId::from_uuid)
        .map_err(|_| AppError::Validation("tenant_id is required and must be a valid UUID".into()))?;

    let preferred_driver_id = body["driver_id"]
        .as_str()
        .and_then(|s| s.parse::<Uuid>().ok());

    let cmd = QuickDispatchCommand { shipment_id, preferred_driver_id };
    let assignment = state.dispatch_service.quick_dispatch(tenant_id, cmd).await?;

    Ok(Json(serde_json::json!({
        "data": {
            "assignment_id": assignment.id,
            "driver_id":     assignment.driver_id.inner(),
            "status":        "pending"
        }
    })))
}

/// Internal (no JWT): GET /v1/internal/drivers/available
///
/// Called by the AI layer's `get_available_drivers` MCP tool.
/// Returns online, unassigned drivers near the given lat/lng within radius_km.
#[derive(Debug, Deserialize)]
pub struct AvailableDriversQuery {
    lat:       f64,
    lng:       f64,
    radius_km: Option<f64>,
    tenant_id: Uuid,
}

pub async fn internal_available_drivers(
    Query(q): Query<AvailableDriversQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let tenant_id = TenantId::from_uuid(q.tenant_id);
    let anchor    = Coordinates { lat: q.lat, lng: q.lng };
    let radius_km = q.radius_km.unwrap_or(10.0).clamp(1.0, 50.0);

    let drivers = state
        .dispatch_service
        .list_available_drivers(&tenant_id, anchor, radius_km)
        .await?;

    let data: Vec<serde_json::Value> = drivers.iter().map(|d| serde_json::json!({
        "driver_id":        d.driver_id.inner(),
        "name":             d.name,
        "distance_km":      d.distance_km,
        "active_stop_count": d.active_stop_count,
        "vehicle_type":     d.vehicle_type,
        "location": {
            "lat": d.location.lat,
            "lng": d.location.lng,
        }
    })).collect();

    Ok(Json(serde_json::json!({ "drivers": data, "count": data.len() })))
}

/// Admin: POST /v1/queue/:shipment_id/cancel-dispatch
///
/// Cancels a previously dispatched shipment — resets the queue row back to
/// `pending` so the dispatcher can reassign. Scoped to the caller's tenant.
/// Call this when a driver's assignment needs to be undone from the dispatch console.
pub async fn cancel_queue_dispatch(
    AuthClaims(claims): AuthClaims,
    Path(shipment_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::DISPATCH_ASSIGN);

    let cancelled = state.queue_repo
        .cancel_dispatch(shipment_id, claims.tenant_id)
        .await
        .map_err(AppError::Internal)?;

    if !cancelled {
        return Err(AppError::NotFound {
            resource: "Dispatched shipment",
            id: shipment_id.to_string(),
        });
    }

    tracing::info!(
        shipment_id = %shipment_id,
        tenant_id   = %claims.tenant_id,
        "Dispatch cancelled by ops — shipment returned to pending queue"
    );

    Ok(Json(serde_json::json!({
        "data": {
            "shipment_id": shipment_id,
            "status":      "pending",
            "message":     "Dispatch cancelled — shipment returned to queue"
        }
    })))
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
