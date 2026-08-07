//! Screen D's data. Poll rather than stream: after checkout the interesting
//! events are minutes apart, so an SSE connection held open across a delivery
//! costs a socket per in-flight order to save a request every few seconds.

use std::sync::Arc;

use axum::{extract::{Path, State}, http::StatusCode, routing::get, Json, Router};
use logisticos_auth::middleware::AuthClaims;
use serde::Serialize;
use uuid::Uuid;

use crate::api::http::AppState;

#[derive(Debug, Serialize)]
pub struct TimelineEntry {
    pub event_type: String,
    /// The device clock where there was one, so the timeline shows when things
    /// happened rather than when the server heard about them.
    pub at:         chrono::DateTime<chrono::Utc>,
    pub payload:    serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct TrackResponse {
    pub order_id:          Uuid,
    pub status:            String,
    pub grand_total_cents: i64,
    pub stops_total:       usize,
    pub stops_collected:   usize,
    pub delivered_at:      Option<chrono::DateTime<chrono::Utc>>,
    pub timeline:          Vec<TimelineEntry>,
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/v1/omnideliv/orders/:id/track", get(track))
}

async fn track(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(order_id): Path<Uuid>,
) -> Result<Json<TrackResponse>, StatusCode> {
    let order = st
        .orders
        .find_by_id(claims.tenant_id, order_id)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "order lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        // Tenant-scoped in the query, so another tenant's order reads as absent
        // rather than leaking that it exists.
        .ok_or(StatusCode::NOT_FOUND)?;

    // A missing timeline is not a missing order. Tracking still answers with
    // the order's state — degrading the detail rather than the whole screen.
    let timeline = st
        .telemetry
        .timeline(claims.tenant_id, order_id)
        .await
        .unwrap_or_else(|e| {
            tracing::error!(err = %e, %order_id, "timeline lookup failed; returning state only");
            Vec::new()
        });

    let stops_collected = order
        .legs
        .iter()
        .filter(|l| l.status == crate::domain::entities::LegStatus::PickedUp)
        .count();

    Ok(Json(TrackResponse {
        order_id:          order.id,
        status:            order.status.as_str().to_string(),
        grand_total_cents: order.grand_total_cents,
        stops_total:       order.legs.len(),
        stops_collected,
        delivered_at:      order.delivered_at,
        timeline: timeline
            .into_iter()
            .map(|e| TimelineEntry {
                at:         e.sla_timestamp(),
                event_type: e.event_type,
                payload:    e.payload,
            })
            .collect(),
    }))
}
