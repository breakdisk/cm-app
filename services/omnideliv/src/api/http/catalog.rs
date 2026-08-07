use std::sync::Arc;

use axum::{extract::{Query, State}, http::StatusCode, routing::get, Json, Router};
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

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/v1/omnideliv/catalog/search", get(search))
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
