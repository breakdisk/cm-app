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
        .route("/v1/omnideliv/catalog/mine", get(my_items))
        .route("/v1/omnideliv/catalog/items/:id/availability", patch(set_availability))
        .route("/v1/omnideliv/catalog/items/:id/allergens", patch(declare_allergens))
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

/// `PATCH /v1/omnideliv/catalog/items/:id/allergens` — declare what is in it.
///
/// An empty list is a real answer: "I confirm it contains none of these". That
/// is the statement an undeclared item cannot make, and until a vendor makes it
/// the item is refused to any customer who states an allergy.
async fn declare_allergens(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(item_id): Path<Uuid>,
    Json(req): Json<DeclareAllergensRequest>,
) -> Result<StatusCode, StatusCode> {
    let ok = st
        .catalog
        .declare_own_item_allergens(claims.tenant_id, claims.user_id, item_id, req.allergens)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "allergen declaration failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // 404 for a foreign item, matching availability: a vendor must not be able
    // to probe whether an item id belongs to a competitor.
    if ok { Ok(StatusCode::NO_CONTENT) } else { Err(StatusCode::NOT_FOUND) }
}

#[derive(Debug, Deserialize)]
pub struct DeclareAllergensRequest {
    /// Empty means "none of them", not "unknown".
    pub allergens: Vec<String>,
}

/// `GET /v1/omnideliv/catalog/mine` — the authenticated vendor's catalog.
///
/// The store is resolved from the token. Every item carries its availability
/// and `warrants_substitute`, which is the thing a vendor is really managing:
/// an item nobody has confirmed inside the freshness window reads as uncertain
/// and the agent proposes a substitute for it. Without this endpoint a vendor
/// cannot see, let alone refresh, that state — and after the window everything
/// they sell is quietly being swapped.
async fn my_items(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
) -> Result<Json<MyCatalogResponse>, StatusCode> {
    let found = st
        .catalog
        .own_items(claims.tenant_id, claims.user_id)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "own catalog lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // 404 rather than an empty list: "you run no store" and "your store has no
    // items" are different answers, and a console needs to tell them apart.
    let (vendor, items) = found.ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(MyCatalogResponse {
        vendor_id:   vendor.id,
        vendor_name: vendor.name,
        items: items
            .iter()
            .map(|s| MyItem {
                id:    s.item_with_availability.item.id,
                name:  s.item_with_availability.item.name.clone(),
                sku:   s.item_with_availability.item.sku.clone(),
                price_cents: s.item_with_availability.item.price_cents,
                allergens:   s.item_with_availability.item.allergens.clone(),
                allergens_declared: s.item_with_availability.item.allergens_declared_at.is_some(),
                is_listed:   s.item_with_availability.item.is_listed,
                availability: s.item_with_availability.availability.state.as_str().to_string(),
                confirmed_at: s.item_with_availability.availability.updated_at,
                warrants_substitute: s.warrants_substitute,
            })
            .collect(),
    }))
}

#[derive(Debug, Serialize)]
struct MyItem {
    id:           Uuid,
    name:         String,
    sku:          String,
    price_cents:  i64,
    allergens:    Vec<String>,
    /// False means nobody has said what is in this. Not the same as "no
    /// allergens" — an undeclared item is refused to any customer who states
    /// an allergy, so this is the vendor's most consequential empty field.
    allergens_declared: bool,
    is_listed:    bool,
    availability: String,
    /// When the vendor last confirmed this. The freshness clock runs from here.
    confirmed_at: chrono::DateTime<chrono::Utc>,
    /// True when the agent will line up a substitute — either because the item
    /// is out of stock or limited, or because the confirmation has gone stale.
    warrants_substitute: bool,
}

#[derive(Debug, Serialize)]
struct MyCatalogResponse {
    vendor_id:   Uuid,
    vendor_name: String,
    items:       Vec<MyItem>,
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
