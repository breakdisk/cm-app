use std::sync::Arc;

use axum::{extract::{Path, State}, http::StatusCode, routing::post, Json, Router};
use logisticos_auth::middleware::AuthClaims;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::http::AppState;
use crate::domain::entities::ProductKey;

#[derive(Debug, Deserialize)]
pub struct OfferRequest {
    pub product:      ProductKey,
    pub external_ref: Uuid,
    pub lat:          f64,
    pub lng:          f64,
    #[serde(default = "default_radius_km")]
    pub radius_km:    f64,
    #[serde(default = "default_fanout")]
    pub fanout:       i64,
}

fn default_radius_km() -> f64 { 5.0 }
fn default_fanout() -> i64 { 5 }

#[derive(Debug, Serialize)]
pub struct OfferResponse {
    pub assignment_ids: Vec<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct ClaimResponse {
    pub won: bool,
}

#[derive(Debug, Deserialize)]
pub struct PositionRequest {
    pub lat:              f64,
    pub lng:              f64,
    pub device_timestamp: Option<chrono::DateTime<chrono::Utc>>,
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/assignments/offer", post(offer))
        .route("/v1/assignments/:id/claim", post(claim))
        .route("/v1/couriers/:id/position", post(position))
}

// Tenant comes from the validated JWT on every handler below, never from the
// body, a path segment, or shared app state. This service has no database-level
// isolation — the tenant_id bound into each repository query is the whole of
// it — so a tenant the caller could name would be no isolation at all.

async fn offer(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Json(req): Json<OfferRequest>,
) -> Result<Json<OfferResponse>, StatusCode> {
    let offers = st
        .dispatch
        .offer_to_nearest(
            claims.tenant_id, req.product, req.external_ref,
            req.lat, req.lng, req.radius_km, req.fanout,
        )
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "offer failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(OfferResponse { assignment_ids: offers.iter().map(|a| a.id).collect() }))
}

async fn claim(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> Result<Json<ClaimResponse>, StatusCode> {
    // A lost race is 200 { won: false }, not an error status. The client needs
    // to distinguish "someone else got it" from "the request failed".
    let won = st.dispatch.claim(claims.tenant_id, id).await.map_err(|e| {
        tracing::error!(err = %e, "claim failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(ClaimResponse { won }))
}

async fn position(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(courier_id): Path<Uuid>,
    Json(req): Json<PositionRequest>,
) -> Result<StatusCode, StatusCode> {
    st.dispatch
        .record_position(claims.tenant_id, courier_id, req.lat, req.lng, req.device_timestamp)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "position ingest failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(StatusCode::ACCEPTED)
}
