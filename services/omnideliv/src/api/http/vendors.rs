//! Vendor read/write surface.
//!
//! `/me` resolves the vendor from the caller's claims — a vendor id in the path
//! would let any signed-in vendor read or edit another's store.

use std::sync::Arc;

use axum::{extract::{Query, State}, http::StatusCode, routing::get, Json, Router};
use logisticos_auth::middleware::AuthClaims;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::http::AppState;
use crate::domain::entities::Vertical;

#[derive(Debug, Deserialize)]
pub struct NearQuery {
    pub vertical: String,
    pub lat: f64,
    pub lng: f64,
    #[serde(default = "default_radius")]
    pub radius_km: f64,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_radius() -> f64 { 5.0 }
fn default_limit() -> i64 { 20 }

#[derive(Debug, Serialize)]
pub struct VendorSummary {
    pub id: Uuid,
    pub name: String,
    pub address: String,
    pub prep_time_minutes: i32,
}

#[derive(Debug, Serialize)]
pub struct VendorProfile {
    pub id: Uuid,
    pub name: String,
    pub address: String,
    pub prep_time_minutes: i32,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct ProfilePatch {
    pub prep_time_minutes: Option<i32>,
    pub status: Option<String>,
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/omnideliv/vendors", get(list_near))
        .route("/v1/omnideliv/vendors/me", get(me).patch(patch_me))
}

async fn list_near(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Query(q): Query<NearQuery>,
) -> Result<Json<Vec<VendorSummary>>, StatusCode> {
    let vertical = match q.vertical.as_str() {
        "restaurant" => Vertical::Restaurant,
        "grocery"    => Vertical::Grocery,
        "pharmacy"   => Vertical::Pharmacy,
        "florist"    => Vertical::Florist,
        "retail"     => Vertical::Retail,
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    let vendors = st
        .catalog
        .vendors_near(claims.tenant_id, vertical, q.lat, q.lng, q.radius_km, q.limit)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "vendor lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(
        vendors.into_iter()
            .map(|v| VendorSummary {
                id: v.id,
                name: v.name,
                address: v.address,
                prep_time_minutes: v.prep_time_minutes,
            })
            .collect(),
    ))
}

async fn me(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
) -> Result<Json<VendorProfile>, StatusCode> {
    let vendor = st
        .catalog
        .vendor_for_user(claims.tenant_id, claims.user_id)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "vendor lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        // 404 rather than 403: a customer hitting this runs no store, which is
        // an absence rather than a permission failure.
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(VendorProfile {
        id: vendor.id,
        name: vendor.name,
        address: vendor.address,
        prep_time_minutes: vendor.prep_time_minutes,
        status: vendor.status.as_str().to_string(),
    }))
}

async fn patch_me(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Json(p): Json<ProfilePatch>,
) -> Result<StatusCode, (StatusCode, String)> {
    // A vendor may pause or resume itself. It may not offboard itself, or mark
    // itself active while still onboarding — those are Partner decisions.
    if let Some(s) = p.status.as_deref() {
        if !matches!(s, "active" | "paused") {
            return Err((StatusCode::FORBIDDEN, "that status is not yours to set".into()));
        }
    }
    if let Some(m) = p.prep_time_minutes {
        // A negative prep time would sort this vendor first in every
        // consolidation plan; three hours is already generous for a kitchen.
        if !(0..=180).contains(&m) {
            return Err((StatusCode::BAD_REQUEST, "prep time must be 0-180 minutes".into()));
        }
    }

    let updated = st
        .catalog
        .update_own_vendor(claims.tenant_id, claims.user_id, p.prep_time_minutes, p.status)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "vendor profile update failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "could not save".into())
        })?;

    if !updated {
        return Err((StatusCode::NOT_FOUND, "you do not operate a store".into()));
    }

    Ok(StatusCode::NO_CONTENT)
}
