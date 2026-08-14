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
    /// Which tenant to build the public photo URL under. The app holds a slug,
    /// not this id, and the photo route is tenant-scoped like every other read.
    pub tenant_id:           Uuid,
    pub name:                String,
    pub price_cents:         i64,
    /// Whether a photo exists. Not a URL — the client derives the path, so a
    /// moved backing store does not strand links in old responses.
    pub has_photo:           bool,
    /// Groups the browse list. `None` = uncategorised, rendered last.
    pub category:            Option<String>,
    pub availability:        String,
    /// Surfaced so the caller can see *why* a substitute was proposed.
    pub warrants_substitute: bool,
}

#[derive(Debug, Deserialize)]
pub struct AvailabilityPatch {
    pub state: String,
}

/// Routes that must sit **outside** this service's auth layer.
///
/// Allowlisting `/v1/omnideliv/public/` at the API gateway is only half of it:
/// omnideliv wraps its own router in `require_auth`, so a path the gateway
/// waves through still meets a 401 one hop later. Two doors, and the first fix
/// looked complete because the gateway binary genuinely had the prefix in it.
pub fn public_routes() -> Router<Arc<AppState>> {
    Router::new().route(
        "/v1/omnideliv/public/catalog/:tenant_id/items/:id/photo",
        get(get_item_photo),
    )
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
        .route("/v1/omnideliv/catalog/items/:id/photo", post(upload_item_photo))
        .route("/v1/omnideliv/catalog/confirm-all", post(confirm_all))
        .route("/v1/omnideliv/catalog/ingest", post(ingest))
        .route("/v1/omnideliv/catalog/ingest/csv", post(ingest_csv))
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
    /// "Mains", "Beverages"… omitted or null = uncategorised.
    #[serde(default)]
    pub category:     Option<String>,
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
                category:       req.category.clone(),
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
    /// `Some(None)` clears the category; omitted leaves it alone. Needs
    /// `double_option` or serde collapses `null` into "absent".
    #[serde(default, deserialize_with = "double_option")]
    pub category:      Option<Option<String>>,
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
            category:     req.category.clone(),
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

/// `POST /v1/omnideliv/catalog/ingest/csv` — a vendor uploads their spreadsheet.
///
/// The adapter for the vendor class the storefront exists to serve: no Shopify,
/// no WooCommerce, no POS — a spreadsheet. It needs no credentials and no second
/// system, which makes it the only ingest a store can use on its first day.
///
/// Takes the raw file as the body rather than multipart: there is exactly one
/// file and no other fields, and multipart would add a parser to the request
/// path for no information gained.
///
/// Rows that cannot be imported come back **with their line numbers**. A vendor
/// holding a 200-row spreadsheet cannot act on "12 rejected"; they can act on
/// "line 47: could not read the price". A file whose header is unusable is
/// refused whole, because importing three of two hundred rows silently is worse
/// than importing none.
///
/// Confirms nothing and declares nothing, exactly like every other ingest —
/// `CatalogSource::Csv` is not `is_human()`, so the vendor still confirms stock
/// and states contents in the console. Uploading a file at 9am is not a claim
/// about the shelf at 7pm.
async fn ingest_csv(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    body: String,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let parsed = crate::application::csv_import::parse(&body)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    let vendor = st
        .catalog
        .vendor_for_user(claims.tenant_id, claims.user_id)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "vendor lookup failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "could not resolve your store".into())
        })?
        .ok_or((StatusCode::NOT_FOUND, "you do not operate a store".into()))?;

    // A file that parsed but yielded nothing usable is a failure to report, not
    // a sync of zero items — otherwise a vendor who exported the wrong sheet
    // sees "imported 0" and reads it as success.
    if parsed.items.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "no usable rows in that file ({} row{} could not be read)",
                parsed.errors.len(),
                if parsed.errors.len() == 1 { "" } else { "s" },
            ),
        ));
    }

    let report = st
        .catalog
        .ingest_for_vendor(claims.tenant_id, vendor.id, CatalogSource::Csv, parsed.items)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "csv ingest failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "import failed".into())
        })?;

    tracing::info!(
        vendor_id = %vendor.id, created = report.created, updated = report.updated,
        rejected = report.rejected, unreadable_rows = parsed.errors.len(),
        "csv catalog import applied",
    );

    Ok(Json(serde_json::json!({
        "created":  report.created,
        "updated":  report.updated,
        "rejected": report.rejected,
        // Per-row, with line numbers. The whole reason this is not a bare count.
        "row_errors": parsed.errors,
        "next_step": "Open your Storefront and confirm stock — imported items \
                      are substituted until a person confirms them.",
    })))
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
        // The console builds the public photo URL from this. Kept explicit in
        // the path rather than looked up by bare item id, so a photo read is
        // tenant-scoped like every other read in this service.
        tenant_id:   claims.tenant_id,
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
                // A flag, not a URL. The public photo path is derivable from
                // (tenant, item) and a stored URL would go stale the moment the
                // backing store moves; the client builds it when this is true.
                has_photo: s.item_with_availability.item.image_key.is_some(),
                category:  s.item_with_availability.item.category.clone(),
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
    has_photo:    bool,
    category:     Option<String>,
    warrants_substitute: bool,
}

#[derive(Debug, Serialize)]
struct MyCatalogResponse {
    tenant_id:   Uuid,
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
                tenant_id:           claims.tenant_id,
                has_photo:           h.item_with_availability.item.image_key.is_some(),
                category:            h.item_with_availability.item.category.clone(),
                name:                h.item_with_availability.item.name,
                price_cents:         h.item_with_availability.item.price_cents,
                availability:        h.item_with_availability.availability.state.as_str().to_string(),
                warrants_substitute: h.warrants_substitute,
            })
            .collect(),
    ))
}

// ── Product photos ──────────────────────────────────────────────────────────

/// `POST /v1/omnideliv/catalog/items/:id/photo` — multipart, field name `file`.
///
/// Bytes go through the service rather than a presigned URL. The bucket is
/// cluster-internal (minio publishes no port and has no Traefik route), so a
/// presigned URL would name somewhere the vendor's browser cannot reach.
async fn upload_item_photo(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    mut multipart: axum::extract::Multipart,
) -> Result<StatusCode, (StatusCode, String)> {
    let Some(storage) = st.photos.clone() else {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "photo storage is not configured on this deployment".to_string(),
        ));
    };

    let mut bytes: Option<Vec<u8>> = None;
    while let Some(field) = multipart.next_field().await.map_err(|e| {
        (StatusCode::BAD_REQUEST, format!("could not read the upload: {e}"))
    })? {
        if field.name() == Some("file") {
            let data = field.bytes().await.map_err(|e| {
                (StatusCode::BAD_REQUEST, format!("could not read the file: {e}"))
            })?;
            bytes = Some(data.to_vec());
            break;
        }
    }

    let bytes = bytes.ok_or((
        StatusCode::BAD_REQUEST,
        "expected a multipart field named 'file'".to_string(),
    ))?;

    if bytes.len() > crate::infrastructure::storage::MAX_PHOTO_BYTES {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                "that image is {} KB; the limit is {} KB",
                bytes.len() / 1024,
                crate::infrastructure::storage::MAX_PHOTO_BYTES / 1024
            ),
        ));
    }

    // Sniffed from the bytes, never taken from the request's Content-Type —
    // that header is supplied by the caller and so is a claim, not evidence.
    // It also keeps SVG out, which is an image to a person and a script host
    // to a browser.
    let content_type = crate::infrastructure::storage::sniff_image(&bytes).ok_or((
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "only JPEG, PNG and WebP images are accepted".to_string(),
    ))?;

    // Tenant-prefixed so one tenant's keys can never collide with another's,
    // and random per upload so replacing a photo cannot be served stale from
    // any cache sitting in front of this.
    let key = format!("catalog/{}/{}/{}", claims.tenant_id, id, Uuid::new_v4());

    storage.put(&key, bytes, content_type).await.map_err(|e| {
        tracing::error!(error = ?e, "photo upload failed");
        (StatusCode::INTERNAL_SERVER_ERROR, "could not store that image".to_string())
    })?;

    let owned = st.catalog
        .set_own_item_photo(claims.tenant_id, claims.user_id, id, Some(&key))
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "photo attach failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "could not attach that image".to_string())
        })?;

    if !owned {
        // Written before the ownership check resolved, so remove it rather
        // than leave an orphan billing storage forever.
        let _ = storage.delete(&key).await;
        return Err((StatusCode::NOT_FOUND, "no such item in your store".to_string()));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// `GET /v1/omnideliv/public/catalog/items/:id/photo` — unauthenticated.
///
/// Public on purpose: a product photo is the thing a customer looks at while
/// deciding, before they have any relationship with the vendor. An `<img>` tag
/// cannot send an Authorization header, so gating this would mean no pictures
/// anywhere. The id is a UUID and the response carries no other item data.
async fn get_item_photo(
    State(st): State<Arc<AppState>>,
    Path((tenant_id, id)): Path<(Uuid, Uuid)>,
) -> Result<axum::response::Response, StatusCode> {
    let storage = st.photos.clone().ok_or(StatusCode::NOT_FOUND)?;

    let key = st.catalog
        .item_photo_key(tenant_id, id)
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "photo lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let (bytes, content_type) = storage
        .get(&key)
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "photo fetch failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        // The row outlived its object. A 404 is the honest answer; a 500 would
        // page somebody for a missing picture.
        .ok_or(StatusCode::NOT_FOUND)?;

    use axum::response::IntoResponse;
    Ok((
        [
            (axum::http::header::CONTENT_TYPE, content_type),
            // Immutable: the key is random per upload, so a changed photo is a
            // different URL and this can never serve a stale one.
            (axum::http::header::CACHE_CONTROL, "public, max-age=31536000, immutable".to_string()),
        ],
        bytes,
    )
        .into_response())
}
