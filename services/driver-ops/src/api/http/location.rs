use axum::{extract::State, Json};
use std::sync::Arc;
use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;
use logisticos_types::{TenantId, DriverId};
use crate::{
    api::http::{AppState, LocationBroadcast},
    application::commands::UpdateLocationCommand,
};

pub async fn update_location(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<UpdateLocationCommand>,
) -> Result<axum::http::StatusCode, AppError> {
    let driver_id = DriverId::from_uuid(claims.user_id);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);

    let lat = cmd.lat;
    let lng = cmd.lng;
    let heading = cmd.heading;
    let speed_kmh = cmd.speed_kmh;

    state.location_service.update_location(&driver_id, &tenant_id, cmd).await?;

    // Broadcast to all connected WebSocket subscribers for this tenant.
    // Failures are silently ignored — no subscribers is not an error.
    let _ = state.location_tx.send(LocationBroadcast {
        driver_id: claims.user_id,
        tenant_id: claims.tenant_id,
        lat,
        lng,
        heading,
        speed_kmh,
    });

    Ok(axum::http::StatusCode::NO_CONTENT)
}
