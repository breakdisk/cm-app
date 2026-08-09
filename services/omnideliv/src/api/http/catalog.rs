use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, patch, post},
    Json, Router,
};
use logisticos_auth::middleware::AuthClaims;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::http::AppState;
use crate::application::services::{IngestReport, ItemDraft, ItemPatch};
use crate::domain::entities::{CatalogSource, IngestedItem};

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
        // Combined on one `.route` call: two calls on the same path panic at
        // startup in axum, which is a boot failure rather than a 404.
        .route("/v1/omnideliv/catalog/items", post(create_item))
        .route(
            "/v1/omnideliv/catalog/items/:id",
            patch(update_item).delete(delist_item),
        )
        .route("/v1/omnideliv/catalog/items/:id/availability", patch(set_availability))
        .route("/v1/omnideliv/catalog/items/:id/allergens", patch(declare_allergens))
        .route("/v1/omnideliv/catalog/confirm-all", post(confirm_all))
        .route("/v1/omnideliv/catalog/ingest", post(ingest))
        // Mesh-internal. `/internal/` is refused by the API gateway's route
        // table before any tier prefix is considered, so this is reachable from
        // inside the cluster and nowhere else — see
        // `internal_routes_stay_unreachable_through_a_tier_prefix`.
        .route("/v1/omnideliv/internal/catalog/ingest", post(ingest_for_vendor))
}

/// The permission a multi-vendor adapter's token must carry.
///
/// Checked rather than inferred from `tenant_slug == "service"`: a string on a
/// token that anything could set is not an authorisation decision. A caller
/// without this cannot name a vendor, which leaves it exactly the self-scoped
/// `/catalog/ingest` route every vendor already has.
const INGEST_PERMISSION: &str = "catalog:ingest";

#[derive(Debug, Deserialize)]
pub struct CreateItemRequest {
    pub sku:         String,
    pub name:        String,
    #[serde(default)]
    pub description: Option<String>,
    pub price_cents: i64,
    /// Absent means "not stated". An empty array means "I confirm it contains
    /// none of these" — a real declaration. The two must not collapse, which is
    /// why this is `Option<Vec<_>>` and not `#[serde(default)] Vec<_>`.
    #[serde(default)]
    pub allergens:    Option<Vec<String>>,
    #[serde(default)]
    pub dietary_tags: Vec<String>,
    #[serde(default = "empty_array")]
    pub modifiers:      serde_json::Value,
    #[serde(default = "empty_object")]
    pub vertical_attrs: serde_json::Value,
}

fn empty_array()  -> serde_json::Value { serde_json::json!([]) }
fn empty_object() -> serde_json::Value { serde_json::json!({}) }

/// `POST /v1/omnideliv/catalog/items` — a vendor adds an item by hand.
///
/// The store is resolved from the token; there is deliberately no vendor_id in
/// the body. Until this route existed the only way to put an item in a catalog
/// was an INSERT by hand, which is why a storefront could be approved and still
/// have nothing to sell.
async fn create_item(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Json(req): Json<CreateItemRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, String)> {
    let created = st
        .catalog
        .create_own_item(
            claims.tenant_id,
            claims.user_id,
            ItemDraft {
                sku:            req.sku,
                name:           req.name,
                description:    req.description,
                price_cents:    req.price_cents,
                allergens:      req.allergens,
                dietary_tags:   req.dietary_tags,
                modifiers:      req.modifiers,
                vertical_attrs: req.vertical_attrs,
            },
        )
        .await
        // The service rejects blank names, negative prices and duplicate SKUs.
        // Those are the caller's fault, so they surface as 400 with the reason
        // rather than a 500 the vendor cannot act on.
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let item = created.ok_or((StatusCode::NOT_FOUND, "you do not operate a store".into()))?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "id": item.id,
            "sku": item.sku,
            "name": item.name,
            "price_cents": item.price_cents,
            // Stated back explicitly: a client that assumed a new item was
            // ready to sell would be wrong, and this is where it finds out.
            "confirmed": false,
        })),
    ))
}

#[derive(Debug, Default, Deserialize)]
pub struct UpdateItemRequest {
    pub name:         Option<String>,
    /// Double option: absent leaves the description alone, explicit `null`
    /// clears it. Flattening these would make "don't touch" indistinguishable
    /// from "erase".
    #[serde(default, deserialize_with = "double_option")]
    pub description:  Option<Option<String>>,
    pub price_cents:  Option<i64>,
    pub dietary_tags: Option<Vec<String>>,
    pub is_listed:    Option<bool>,
    pub modifiers:    Option<serde_json::Value>,
}

fn double_option<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    serde::Deserialize::deserialize(d).map(Some)
}

/// `PATCH /v1/omnideliv/catalog/items/:id` — edit one of your own items.
async fn update_item(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(item_id): Path<Uuid>,
    Json(req): Json<UpdateItemRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let updated = st
        .catalog
        .update_own_item(claims.tenant_id, claims.user_id, item_id, ItemPatch {
            name:         req.name,
            description:  req.description,
            price_cents:  req.price_cents,
            dietary_tags: req.dietary_tags,
            is_listed:    req.is_listed,
            modifiers:    req.modifiers,
        })
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    if !updated {
        // Same answer for "no such item" and "not yours" — which of the two it
        // is, is itself information about a competitor's catalog.
        return Err((StatusCode::NOT_FOUND, "no such item in your store".into()));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /v1/omnideliv/catalog/items/:id` — take it off the menu.
///
/// Delists; it does not erase. Baskets and settled order legs reference these
/// rows, so a real delete would either fail on a foreign key or destroy the
/// record of what a customer bought.
async fn delist_item(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(item_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    let ok = st
        .catalog
        .delist_own_item(claims.tenant_id, claims.user_id, item_id)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "delist failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "could not delist".into())
        })?;

    if !ok {
        return Err((StatusCode::NOT_FOUND, "no such item in your store".into()));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /v1/omnideliv/catalog/confirm-all` — one tap, whole store.
///
/// The counterweight to making imports arrive unconfirmed: without a bulk path
/// a vendor who just synced 200 items would face 200 taps, do none of them, and
/// the freshness signal would decay into noise operators ignore.
///
/// Only touches items currently marked available — confirming a store is
/// saying "what is listed is on the shelf", not silently un-marking the things
/// the vendor flagged as gone.
async fn confirm_all(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let confirmed = st
        .catalog
        .confirm_all_own_items(claims.tenant_id, claims.user_id)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "bulk confirm failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "could not confirm".into())
        })?
        .ok_or((StatusCode::NOT_FOUND, "you do not operate a store".into()))?;

    Ok(Json(serde_json::json!({ "confirmed": confirmed })))
}

#[derive(Debug, Deserialize)]
pub struct IngestRequest {
    /// `shopify`, `woocommerce`, `csv`, `pos`. Not `manual` — that path is
    /// `POST /catalog/items`, and letting an ingest claim to be hand-typed would
    /// erase the provenance distinction the whole port rests on.
    pub source: String,
    pub items:  Vec<IngestedItem>,
}

/// `POST /v1/omnideliv/catalog/ingest` — the pluggable ingest port.
///
/// One route for every source. A Shopify product sync, a WooCommerce webhook, a
/// CSV upload and a POS push all translate their own payload into
/// `IngestedItem` and arrive here; none of them get their own write path into
/// the catalog, so none of them can invent their own merge rules.
///
/// Scoped to the caller's own store today, which covers the console's CSV
/// upload and a merchant's own script. An unattended adapter running for many
/// vendors needs a service token and an explicit vendor_id — the service method
/// underneath already takes one, so that is a route change and not a redesign.
///
/// Never confirms stock, for any source, however real-time it claims to be.
async fn ingest(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Json(req): Json<IngestRequest>,
) -> Result<Json<IngestReport>, (StatusCode, String)> {
    let source = CatalogSource::parse(&req.source)
        .filter(|s| !s.is_human())
        .ok_or((
            StatusCode::BAD_REQUEST,
            format!("unknown or non-ingest source: {}", req.source),
        ))?;

    let vendor = st
        .catalog
        .vendor_for_user(claims.tenant_id, claims.user_id)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "vendor lookup failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "could not resolve your store".into())
        })?
        .ok_or((StatusCode::NOT_FOUND, "you do not operate a store".into()))?;

    let report = st
        .catalog
        .ingest_for_vendor(claims.tenant_id, vendor.id, source, req.items)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "catalog ingest failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "ingest failed".into())
        })?;

    tracing::info!(
        vendor_id = %vendor.id, source = %source.as_str(),
        created = report.created, updated = report.updated, rejected = report.rejected,
        "catalog ingest applied",
    );
    Ok(Json(report))
}

#[derive(Debug, Deserialize)]
pub struct VendorIngestRequest {
    /// Which store this batch is for. Caller-supplied because an unattended
    /// adapter has no user to resolve one from — and therefore checked against
    /// the token's tenant before anything is written. `catalog_items.vendor_id`
    /// references `vendors(id)` without a tenant predicate, so an id from
    /// another tenant would satisfy the foreign key and file rows under the
    /// wrong one; `ingest_for_vendor` refuses it.
    pub vendor_id: Uuid,
    pub source:    String,
    pub items:     Vec<IngestedItem>,
}

/// `POST /v1/omnideliv/internal/catalog/ingest` — one adapter, many vendors.
///
/// The unattended half of the ingest port. A sync worker holding bindings for
/// hundreds of stores cannot use `/catalog/ingest`, which resolves exactly one
/// vendor from the caller's own login; this route lets it name the store.
///
/// Three things stop that from being a hole:
///
/// 1. **Not routable from outside.** The API gateway refuses any path
///    containing `/internal/` before it considers the `/v1/omnideliv` prefix,
///    so this is reachable only from inside the mesh.
/// 2. **A permission, not a naming convention.** The token must carry
///    `catalog:ingest`. A service token is minted the same way
///    `FieldOpsDispatch` mints one — short TTL, signed with the shared secret —
///    but with this permission added.
/// 3. **Tenant still comes from the token.** Only `vendor_id` is caller-supplied,
///    and it is proved to belong to that tenant before the first write.
///
/// It confirms nothing, exactly like every other ingest path.
async fn ingest_for_vendor(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Json(req): Json<VendorIngestRequest>,
) -> Result<Json<IngestReport>, (StatusCode, String)> {
    if !claims.has_permission(INGEST_PERMISSION) {
        // 403 rather than 404: this route's existence is not a secret, and a
        // misconfigured adapter needs to learn that its token is short a
        // permission rather than hunt a phantom routing bug.
        return Err((
            StatusCode::FORBIDDEN,
            format!("this token may not ingest for a named vendor (needs {INGEST_PERMISSION})"),
        ));
    }

    let source = CatalogSource::parse(&req.source)
        .filter(|s| !s.is_human())
        .ok_or((
            StatusCode::BAD_REQUEST,
            format!("unknown or non-ingest source: {}", req.source),
        ))?;

    let report = st
        .catalog
        .ingest_for_vendor(claims.tenant_id, req.vendor_id, source, req.items)
        .await
        .map_err(|e| {
            // The tenant check surfaces here. It is the caller's mistake — or
            // an attempt — so it answers 400 with the reason rather than a 500.
            tracing::warn!(err = %e, vendor_id = %req.vendor_id, "vendor ingest refused");
            (StatusCode::BAD_REQUEST, e.to_string())
        })?;

    tracing::info!(
        vendor_id = %req.vendor_id, source = %source.as_str(), actor = %claims.user_id,
        created = report.created, updated = report.updated, rejected = report.rejected,
        "catalog ingest applied for a named vendor",
    );
    Ok(Json(report))
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
                description: s.item_with_availability.item.description.clone(),
                price_cents: s.item_with_availability.item.price_cents,
                allergens:   s.item_with_availability.item.allergens.clone(),
                allergens_declared: s.item_with_availability.item.allergens_declared_at.is_some(),
                is_listed:   s.item_with_availability.item.is_listed,
                availability: s.item_with_availability.availability.state.as_str().to_string(),
                confirmed_at: s.item_with_availability.availability.confirmed_at,
                source:      s.item_with_availability.item.source.as_str().to_string(),
                synced_at:   s.item_with_availability.item.synced_at,
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
    description:  Option<String>,
    price_cents:  i64,
    allergens:    Vec<String>,
    /// False means nobody has said what is in this. Not the same as "no
    /// allergens" — an undeclared item is refused to any customer who states
    /// an allergy, so this is the vendor's most consequential empty field.
    allergens_declared: bool,
    is_listed:    bool,
    availability: String,
    /// When a **human** last confirmed this. `null` means nobody ever has —
    /// which is where every imported item starts. The freshness clock runs from
    /// here and from nothing else.
    confirmed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Where the row's facts came from: `manual`, `shopify`, `woocommerce`,
    /// `csv`, `pos`. Shown so a vendor can tell what they typed from what their
    /// shop pushed — and so "why did my price change" has an answer.
    source:       String,
    /// When an ingest last touched it. Distinct from `confirmed_at` on purpose:
    /// a sync that ran a minute ago still confirms nothing.
    synced_at:    Option<chrono::DateTime<chrono::Utc>>,
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
