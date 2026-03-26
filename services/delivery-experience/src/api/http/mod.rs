use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use serde::Deserialize;
use uuid::Uuid;

use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::rbac::permissions;
use logisticos_errors::AppError;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        // Public — no auth required
        .route("/track/:tracking_number",         get(public_track))
        // Authenticated — merchant/ops
        .route("/v1/tracking/:shipment_id",       get(get_by_shipment_id))
        .route("/v1/tracking",                    get(list_shipments))
}

// ---------------------------------------------------------------------------
// GET /track/:tracking_number — public customer tracking page
// ---------------------------------------------------------------------------

async fn public_track(
    State(state): State<AppState>,
    Path(tracking_number): Path<String>,
) -> impl IntoResponse {
    match state.tracking_svc.get_public(&tracking_number).await {
        Ok(record) => {
            // Strip internal fields not appropriate for public display.
            let public = serde_json::json!({
                "tracking_number":    record.tracking_number,
                "status":             record.current_status,
                "status_label":       record.current_status.display_label(),
                "origin":             record.origin_address,
                "destination":        record.destination_address,
                "estimated_delivery": record.estimated_delivery,
                "delivered_at":       record.delivered_at,
                "attempt_number":     record.attempt_number,
                "next_attempt_at":    record.next_attempt_at,
                "recipient_name":     record.recipient_name,
                "history":            record.status_history,
                // Show driver position only for active deliveries
                "driver_location":    if record.current_status == crate::domain::entities::TrackingStatus::OutForDelivery
                                         || record.current_status == crate::domain::entities::TrackingStatus::AssignedToDriver {
                    record.driver_position.as_ref().map(|p| serde_json::json!({"lat": p.lat, "lng": p.lng}))
                } else {
                    None
                },
            });
            (StatusCode::OK, Json(public)).into_response()
        }
        Err(AppError::NotFound(_)) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Tracking number not found"})),
        ).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        ).into_response(),
    }
}

// ---------------------------------------------------------------------------
// GET /v1/tracking/:shipment_id — authenticated detailed view
// ---------------------------------------------------------------------------

async fn get_by_shipment_id(
    State(state): State<AppState>,
    claims: AuthClaims,
    Path(shipment_id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_READ)?;

    let record = state.tracking_svc.get_by_shipment_id(shipment_id).await?;

    // Tenants can only see their own shipments.
    if record.tenant_id != claims.tenant_id {
        return Err(AppError::Forbidden("Access denied".into()));
    }

    Ok::<_, AppError>((StatusCode::OK, Json(record)))
}

// ---------------------------------------------------------------------------
// GET /v1/tracking?limit=&offset=
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub limit:  Option<i64>,
    pub offset: Option<i64>,
}

async fn list_shipments(
    State(state): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<ListQuery>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_READ)?;

    let records = state
        .tracking_svc
        .list(&claims.tenant_id, q.limit.unwrap_or(50), q.offset.unwrap_or(0))
        .await?;

    Ok::<_, AppError>((
        StatusCode::OK,
        Json(serde_json::json!({"shipments": records, "count": records.len()})),
    ))
}
