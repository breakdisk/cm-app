use std::sync::Arc;

use axum::{extract::{Path, State}, http::StatusCode, routing::{delete, get, post}, Json, Router};
use logisticos_auth::middleware::AuthClaims;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::http::AppState;

#[derive(Debug, Deserialize)]
pub struct AddLineRequest {
    pub vendor_id: Uuid,
    pub item_id:   Uuid,
    #[serde(default = "one")]
    pub qty:       i32,
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

    Ok(Json(BasketResponse {
        id: b.id,
        status: b.status.as_str().to_string(),
        goods_total_cents: b.goods_total_cents(),
        lines_awaiting_review: b.lines_awaiting_review().len(),
        conflicts: b.conflicts.clone(),
    }))
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

    Ok(Json(BasketResponse {
        id: b.id,
        status: b.status.as_str().to_string(),
        goods_total_cents: b.goods_total_cents(),
        lines_awaiting_review: b.lines_awaiting_review().len(),
        conflicts: b.conflicts.clone(),
    }))
}

impl BasketResponse {
    fn of(b: &crate::domain::entities::Basket) -> Self {
        Self {
            id: b.id,
            status: b.status.as_str().to_string(),
            goods_total_cents: b.goods_total_cents(),
            lines_awaiting_review: b.lines_awaiting_review().len(),
            conflicts: b.conflicts.clone(),
        }
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

    let basket = st
        .baskets
        .add_item(claims.tenant_id, basket_id, req.vendor_id, req.item_id, req.qty)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "add line failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "could not add the item".into())
        })?;

    Ok(Json(BasketResponse::of(&basket)))
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

    Ok(Json(BasketResponse::of(&basket)))
}
