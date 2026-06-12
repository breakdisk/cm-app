//! Gig offer endpoints — the driver-facing grab surface plus the ops-console
//! broadcast action.
//!
//! Claim contract: `200 {data:{assignment_id,...}}` on win; `409` with error
//! message `OFFER_TAKEN` (lost the race / TTL elapsed) or `DRIVER_BUSY`
//! (driver already holds an active assignment — e.g. won another offer in the
//! same instant). `seen` and `pass` are fire-and-forget bookkeeping.

use axum::{extract::{Path, State}, Json};
use std::sync::Arc;
use uuid::Uuid;
use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::require_permission;
use logisticos_errors::AppError;
use logisticos_types::{DriverId, TenantId};
use crate::api::http::AppState;

/// `GET /v1/offers/open` — live offers for the authenticated driver.
/// App-restart / missed-FCM recovery path.
pub async fn list_open(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let driver_id = DriverId::from_uuid(claims.user_id);
    let offers = state.offer_service.open_for_driver(&driver_id).await?;
    Ok(Json(serde_json::json!({ "data": offers })))
}

/// `POST /v1/offers/:id/claim` — the atomic grab.
pub async fn claim(
    AuthClaims(claims): AuthClaims,
    Path(offer_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let driver_id = DriverId::from_uuid(claims.user_id);
    let won = state.offer_service.claim(&driver_id, offer_id).await?;
    Ok(Json(serde_json::json!({ "data": won })))
}

/// `POST /v1/offers/:id/pass` — penalty-free; excludes the driver from later
/// waves of this offer only. Never touches decline_count.
pub async fn pass(
    AuthClaims(claims): AuthClaims,
    Path(offer_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<axum::http::StatusCode, AppError> {
    let driver_id = DriverId::from_uuid(claims.user_id);
    state.offer_service.pass(&driver_id, offer_id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// `POST /v1/offers/:id/seen` — client-verified impression, fired at first
/// card render. The acceptance-rate denominator counts ONLY these (FCM
/// delivery is not an impression — dead zones must not tank driver metrics).
pub async fn seen(
    AuthClaims(claims): AuthClaims,
    Path(offer_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<axum::http::StatusCode, AppError> {
    let driver_id = DriverId::from_uuid(claims.user_id);
    state.offer_service.seen(&driver_id, offer_id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// `POST /v1/queue/:shipment_id/broadcast` — ops-console action: broadcast a
/// pending shipment to the gig pool instead of 1:1 quick dispatch.
pub async fn broadcast(
    AuthClaims(claims): AuthClaims,
    Path(shipment_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::DISPATCH_ASSIGN);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let offer = state.offer_service.broadcast(tenant_id, shipment_id).await?;
    Ok(Json(serde_json::json!({
        "data": {
            "offer_id":   offer.id,
            "wave":       offer.wave,
            "expires_at": offer.expires_at,
        }
    })))
}
