use std::sync::Arc;

use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use logisticos_auth::middleware::AuthClaims;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::http::AppState;
use crate::application::services::CheckoutError;

#[derive(Debug, Deserialize)]
pub struct CheckoutRequest {
    pub basket_id: Uuid,
    #[serde(default)]
    pub tip_cents: i64,
    pub delivery_lat: f64,
    pub delivery_lng: f64,
    /// "Unit 12B, gate code 4417." Optional, and the only free text a client
    /// controls that a courier is asked to act on — bounded server-side.
    #[serde(default)]
    pub delivery_note: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CheckoutResponse {
    pub order_id:          Uuid,
    pub grand_total_cents: i64,
    pub stops:             usize,
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/v1/omnideliv/orders/checkout", post(checkout))
}

async fn checkout(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Json(req): Json<CheckoutRequest>,
) -> Result<Json<CheckoutResponse>, (StatusCode, String)> {
    if req.tip_cents < 0 {
        return Err((StatusCode::BAD_REQUEST, "tip cannot be negative".into()));
    }

    // Tenant from the validated token, never from app state or the body. This
    // is the money path: a tenant the caller could influence would let one
    // tenant's checkout settle against another tenant's basket and vendors.
    let order = st
        .checkout
        .place(claims.tenant_id, req.basket_id, req.tip_cents, req.delivery_lat, req.delivery_lng,
               &claims.email, claims.phone.as_deref(), req.delivery_note.as_deref())
        .await
        .map_err(|e| match e {
            // A basket awaiting review is the client's cue to show Screen C,
            // not an error to surface as a failure.
            CheckoutError::AwaitingReview(_) => (StatusCode::CONFLICT, e.to_string()),
            CheckoutError::BasketNotFound(_) => (StatusCode::NOT_FOUND, e.to_string()),
            CheckoutError::EmptyBasket
            | CheckoutError::VendorUnavailable(_) => (StatusCode::BAD_REQUEST, e.to_string()),
            // No courier means no charge — 503 tells the client to retry rather
            // than to treat the basket as spent.
            CheckoutError::NoCourier => (StatusCode::SERVICE_UNAVAILABLE, e.to_string()),
            CheckoutError::Other(inner) => {
                tracing::error!(err = %inner, "checkout failed");
                (StatusCode::INTERNAL_SERVER_ERROR, "checkout failed".into())
            }
        })?;

    // The courier is already offered the job at this point, so a persist
    // failure leaves a dispatched order we have no record of. Logged loudly
    // with the order id so it can be reconciled by hand; the alternative —
    // dispatching only after a successful save — trades this for orders that
    // are recorded but never delivered.
    st.orders.save(&order).await.map_err(|e| {
        tracing::error!(err = %e, order_id = %order.id, courier_task_id = ?order.courier_task_id,
                        "order persist failed AFTER courier dispatch — needs manual reconciliation");
        (StatusCode::INTERNAL_SERVER_ERROR, "checkout failed".into())
    })?;

    // Placed. Published after persistence so a customer is never told about an
    // order that failed to save; a publish failure loses the confirmation, not
    // the order.
    if let Err(e) = st.order_events.order_placed(&order).await {
        tracing::error!(err = %e, order_id = %order.id, "order.placed publish failed");
    }

    Ok(Json(CheckoutResponse {
        order_id:          order.id,
        grand_total_cents: order.grand_total_cents,
        stops:             order.legs.len(),
    }))
}
