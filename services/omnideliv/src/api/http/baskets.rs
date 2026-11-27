use std::sync::Arc;

use axum::{extract::{Path, State}, http::StatusCode, routing::{delete, get, post}, Json, Router};
use logisticos_auth::middleware::AuthClaims;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::http::AppState;
use crate::domain::entities::{ModifierError, SelectedModifier};

#[derive(Debug, Deserialize)]
pub struct AddLineRequest {
    pub vendor_id: Uuid,
    pub item_id:   Uuid,
    #[serde(default = "one")]
    pub qty:       i32,
    /// Chosen modifier option ids. Ids only — the prices behind them are read
    /// from the catalog server-side, for the same reason the base price is.
    /// Absent means "no modifiers", which is the common case.
    #[serde(default)]
    pub modifiers: Vec<Uuid>,
}

fn one() -> i32 { 1 }

#[derive(Debug, Serialize)]
pub struct BasketResponse {
    pub id:                Uuid,
    pub status:            String,
    pub goods_total_cents: i64,
    pub lines_awaiting_review: usize,
    /// What the mesh's verification found, restated at the point of decision.
    /// Empty for a manually built basket — nothing proposed it.
    pub conflicts:         Vec<crate::domain::entities::BasketConflict>,
    /// What is actually in the basket.
    ///
    /// The response carried a total and nothing else, so the review screen could
    /// tell a customer what they owed but not what for — and gave them no way to
    /// remove a line they did not want. A total with no itemisation is the one
    /// thing a checkout screen must not be.
    pub lines:             Vec<BasketLineView>,
}

#[derive(Debug, Serialize)]
pub struct BasketLineView {
    pub id:                Uuid,
    pub item_id:           Uuid,
    pub vendor_id:         Uuid,
    /// Resolved from the catalog at read time. An item deleted since it was
    /// added still has a line and still has to render — hence the fallback
    /// rather than dropping the line or failing the request.
    pub name:              String,
    pub qty:               i32,
    /// Already includes the modifier deltas below; `subtotal` is this × qty.
    pub unit_price_cents:  i64,
    pub subtotal_cents:    i64,
    /// `proposed` | `accepted` | `substituted` | `rejected`. A substituted line
    /// is what `lines_awaiting_review` counts and what blocks checkout.
    pub state:             String,
    pub modifiers:         Vec<SelectedModifier>,
}

// Namespaced under `/v1/omnideliv` per ADR-0015's API-contract rule: product
// services do not take flat resource names in the shared `/v1` namespace.
// `/v1/orders` is the case that forces this — it already routes to
// order-intake, where a POST would not 404 but succeed and create a real
// shipment. Prefixing every OmniDeliv route keeps that class of mistake
// impossible rather than merely unlikely.
pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/omnideliv/baskets", post(create))
        .route("/v1/omnideliv/baskets/:id", get(fetch))
        .route("/v1/omnideliv/baskets/:id/lines", post(add_line))
        .route("/v1/omnideliv/baskets/:id/lines/:line_id", delete(remove_line))
}

/// Both the tenant and the customer come from the validated JWT rather than a
/// request body: a caller who can name the customer a basket belongs to can
/// open a basket in someone else's name.
async fn create(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
) -> Result<Json<BasketResponse>, StatusCode> {
    let b = st.baskets.create(claims.tenant_id, claims.user_id).await.map_err(|e| {
        tracing::error!(err = %e, "basket create failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(basket_view(&st, claims.tenant_id, &b).await))
}

async fn fetch(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> Result<Json<BasketResponse>, StatusCode> {
    // Tenant scoping happens in the repository query, so a basket belonging to
    // another tenant reads as 404 rather than leaking its existence.
    let b = st
        .baskets
        .get(claims.tenant_id, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(basket_view(&st, claims.tenant_id, &b).await))
}

/// Build the wire view of a basket, resolving item names.
///
/// One `find_items` for the whole basket rather than a lookup per line. Async,
/// which is why this replaced the plain `BasketResponse::of` — every caller now
/// awaits it, and none of them can render a basket without the names.
async fn basket_view(
    st: &AppState,
    tenant_id: Uuid,
    b: &crate::domain::entities::Basket,
) -> BasketResponse {
    let mut ids: Vec<Uuid> = b.lines.iter().map(|l| l.item_id).collect();
    ids.sort_unstable();
    ids.dedup();

    // A failed lookup degrades to unnamed lines rather than failing the request:
    // the customer can still see quantities, prices and their total, and can
    // still remove something. Losing the basket entirely because one name could
    // not be read would be the worse trade.
    let names: std::collections::HashMap<Uuid, String> = match st.catalog.find_items(tenant_id, &ids).await {
        Ok(items) => items.into_iter().map(|i| (i.id, i.name)).collect(),
        Err(e) => {
            tracing::warn!(err = %e, "could not resolve basket item names");
            std::collections::HashMap::new()
        }
    };

    BasketResponse {
        id: b.id,
        status: b.status.as_str().to_string(),
        goods_total_cents: b.goods_total_cents(),
        lines_awaiting_review: b.lines_awaiting_review().len(),
        conflicts: b.conflicts.clone(),
        lines: b
            .lines
            .iter()
            .map(|l| BasketLineView {
                id:               l.id,
                item_id:          l.item_id,
                vendor_id:        l.vendor_id,
                name:             names
                    .get(&l.item_id)
                    .cloned()
                    // Said plainly. A blank would read as a rendering bug; this
                    // reads as what it is, and the line is still removable.
                    .unwrap_or_else(|| "Item no longer listed".to_string()),
                qty:              l.qty,
                unit_price_cents: l.unit_price_cents,
                subtotal_cents:   l.subtotal_cents(),
                state:            format!("{:?}", l.state).to_lowercase(),
                modifiers:        l.modifiers.clone(),
            })
            .collect(),
    }
}

/// Add a catalog item by hand — the path that works with the mesh switched off.
///
/// The body carries only *what* and *how many*. Price and vertical are read
/// from the catalog server-side: a client-supplied price is a client-supplied
/// discount, and a client-supplied vertical files an order into the wrong
/// partition.
async fn add_line(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(basket_id): Path<Uuid>,
    Json(req): Json<AddLineRequest>,
) -> Result<Json<BasketResponse>, (StatusCode, String)> {
    if req.qty < 1 {
        return Err((StatusCode::BAD_REQUEST, "qty must be at least 1".into()));
    }

    // What makes a table order a VENUE order.
    //
    // `vendor_id` comes from the client, so without this a diner sitting in one
    // restaurant could add items from a vendor across town — the QR would bind
    // them to a table and to nothing else. The venue comes from the session row
    // rather than the token, so a session that has since been ended or expired
    // stops ordering even while its JWT still verifies.
    if claims.table_session {
        let session = st
            .venues
            .find_live_session(claims.tenant_id, claims.user_id, chrono::Utc::now())
            .await
            .map_err(|e| {
                tracing::error!(err = %e, "table session lookup failed");
                (StatusCode::INTERNAL_SERVER_ERROR, "could not add the item".into())
            })?
            .ok_or((
                StatusCode::UNAUTHORIZED,
                "This table session has ended. Scan the code again.".to_string(),
            ))?;

        let at_venue = st
            .venues
            .vendor_is_at_venue(claims.tenant_id, session.venue_id, req.vendor_id)
            .await
            .map_err(|e| {
                tracing::error!(err = %e, "venue vendor check failed");
                (StatusCode::INTERNAL_SERVER_ERROR, "could not add the item".into())
            })?;

        if !at_venue {
            tracing::warn!(
                venue_id = %session.venue_id, vendor_id = %req.vendor_id,
                "table session tried to order from a vendor outside its venue",
            );
            return Err((
                StatusCode::FORBIDDEN,
                "That item is not sold at this venue.".into(),
            ));
        }
    }

    let basket = st
        .baskets
        .add_item(
            claims.tenant_id,
            basket_id,
            req.vendor_id,
            req.item_id,
            req.qty,
            &req.modifiers,
        )
        .await
        .map_err(|e| {
            // A rejected modifier selection is the caller's to fix — an option
            // that is not on this item, a repeat, or a required group left
            // empty. Answering 500 would tell the customer to try again later
            // for something retrying can never resolve, and would bury a real
            // client bug in the error log as a server fault.
            if let Some(me) = e.downcast_ref::<ModifierError>() {
                return (StatusCode::BAD_REQUEST, me.to_string());
            }
            tracing::error!(err = %e, "add line failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "could not add the item".into())
        })?;

    Ok(Json(basket_view(&st, claims.tenant_id, &basket).await))
}

async fn remove_line(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path((basket_id, line_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<BasketResponse>, (StatusCode, String)> {
    let (basket, removed) = st
        .baskets
        .remove_item(claims.tenant_id, basket_id, line_id)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "remove line failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "could not remove the item".into())
        })?;

    // 404 rather than a cheerful 200: reporting success for a line that was
    // never there hides a client bug and confuses a retry.
    if !removed {
        return Err((StatusCode::NOT_FOUND, "no such line".into()));
    }

    Ok(Json(basket_view(&st, claims.tenant_id, &basket).await))
}
