use std::sync::Arc;

use axum::{extract::{Path, Query, State}, http::StatusCode, routing::{get, patch}, Json, Router};
use logisticos_auth::middleware::AuthClaims;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::http::AppState;

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub vendor_id: Uuid,
    pub q:         String,
    /// Comma-separated allergens to exclude.
    #[serde(default)]
    pub avoid:     String,
    #[serde(default = "default_limit")]
    pub limit:     i64,
}

fn default_limit() -> i64 { 20 }

#[derive(Debug, Serialize)]
pub struct SearchHit {
    pub item_id:             Uuid,
    pub name:                String,
    pub price_cents:         i64,
    pub availability:        String,
    /// Surfaced so the caller can see *why* a substitute was proposed.
    pub warrants_substitute: bool,
}

#[derive(Debug, Deserialize)]
pub struct AvailabilityPatch {
    pub state: String,
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/omnideliv/catalog/search", get(search))
        .route("/v1/omnideliv/catalog/items/:id/availability", patch(set_availability))
}

/// A vendor declaring stock.
///
/// This is the only input to the freshness model. Without it every
/// availability row keeps its creation timestamp, ages past the window, and
/// `warrants_substitute` turns true for the entire catalog — the substitution
/// logic working exactly as designed on data nobody refreshes.
async fn set_availability(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(item_id): Path<Uuid>,
    Json(p): Json<AvailabilityPatch>,
) -> Result<StatusCode, (StatusCode, String)> {
    let state = match p.state.as_str() {
        "available"    => crate::domain::entities::AvailabilityState::Available,
        "limited"      => crate::domain::entities::AvailabilityState::Limited,
        "out_of_stock" => crate::domain::entities::AvailabilityState::OutOfStock,
        other => return Err((StatusCode::BAD_REQUEST, format!("unknown state: {other}"))),
    };

    let updated = st
        .catalog
        .set_own_item_availability(claims.tenant_id, claims.user_id, item_id, state)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "availability update failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "could not save".into())
        })?;

    if !updated {
        return Err((StatusCode::NOT_FOUND, "no such item in your store".into()));
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn search(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Query(q): Query<SearchQuery>,
) -> Result<Json<Vec<SearchHit>>, StatusCode> {
    let avoid: Vec<String> = q
        .avoid
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();

    let hits = st
        .catalog
        // Tenant comes from the validated JWT, never from the caller. A
        // tenant_id query parameter would let any authenticated user read any
        // tenant's catalog — the repository signature is this service's only
        // isolation boundary, so what gets bound to it has to be trusted.
        .search(claims.tenant_id, q.vendor_id, &q.q, &avoid, q.limit)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "catalog search failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(
        hits.into_iter()
            .map(|h| SearchHit {
                item_id:             h.item_with_availability.item.id,
                name:                h.item_with_availability.item.name,
                price_cents:         h.item_with_availability.item.price_cents,
                availability:        h.item_with_availability.availability.state.as_str().to_string(),
                warrants_substitute: h.warrants_substitute,
            })
            .collect(),
    ))
}
