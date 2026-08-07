use std::sync::Arc;

use axum::{extract::{Path, State}, http::StatusCode, routing::{get, post}, Json, Router};
use logisticos_auth::middleware::AuthClaims;
use serde::Serialize;
use uuid::Uuid;

use crate::api::http::AppState;

#[derive(Debug, Serialize)]
pub struct BasketResponse {
    pub id:                Uuid,
    pub status:            String,
    pub goods_total_cents: i64,
    pub lines_awaiting_review: usize,
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
    }))
}
