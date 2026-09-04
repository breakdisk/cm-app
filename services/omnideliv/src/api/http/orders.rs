use std::sync::Arc;

use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use logisticos_auth::middleware::AuthClaims;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::http::AppState;
use crate::application::services::{CheckoutError, PlaceOutcome};
use crate::domain::entities::PaymentMethod;

#[derive(Debug, Deserialize)]
pub struct CheckoutRequest {
    pub basket_id: Uuid,
    #[serde(default)]
    pub tip_cents: i64,
    /// Where the courier is going.
    ///
    /// **Optional only because a diner has none.** A table session orders food
    /// to the table it is sitting at; there is no address, and the scan
    /// response does not carry the venue's coordinates for the client to echo
    /// back. Requiring them would leave a web diner client sending a magic
    /// `0,0` -- a real place, in the Gulf of Guinea, that something downstream
    /// would eventually treat as one.
    ///
    /// Still REQUIRED for a delivery order, enforced below against the
    /// principal rather than by serde, so an omitted field cannot silently
    /// become `0.0` and dispatch a courier to the Atlantic.
    #[serde(default)]
    pub delivery_lat: Option<f64>,
    #[serde(default)]
    pub delivery_lng: Option<f64>,
    /// "Unit 12B, gate code 4417." Optional, and the only free text a client
    /// controls that a courier is asked to act on — bounded server-side.
    #[serde(default)]
    pub delivery_note: Option<String>,
    /// `"cod"` or `"online"`. Defaults to `Cod` — see `PaymentMethod::default`
    /// — so a client that predates this field (every OmniDeliv app build
    /// before this feature) keeps getting exactly today's checkout with no
    /// changes on its end.
    #[serde(default)]
    pub payment_method: PaymentMethod,
}

#[derive(Debug, Serialize)]
pub struct CheckoutResponse {
    pub order_id:          Uuid,
    pub grand_total_cents: i64,
    pub stops:             usize,
    /// Present only for `payment_method: "online"` — the hosted-checkout page
    /// the client must send the customer to before a courier is ever offered
    /// the job. `null` for `"cod"`, which needs no such page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkout_url:      Option<String>,
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

    // A client that ignores the flag on the scan response must not get a
    // generic 500 from a gateway that was never going to answer. This is the
    // one place that knows the difference between "payment failed" and
    // "payment was never available here".
    if req.payment_method == PaymentMethod::Online && !st.online_payment_enabled {
        return Err((
            StatusCode::BAD_REQUEST,
            "Online payment is not available here. Please pay on collection.".into(),
        ));
    }

    // Dine-in has no destination; delivery must have one. Checked against the
    // principal, never a default, so a delivery client that omits the fields
    // gets a 400 instead of an order routed to (0, 0).
    let (delivery_lat, delivery_lng) = if claims.table_session {
        (req.delivery_lat.unwrap_or(0.0), req.delivery_lng.unwrap_or(0.0))
    } else {
        match (req.delivery_lat, req.delivery_lng) {
            (Some(lat), Some(lng)) => (lat, lng),
            _ => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "delivery_lat and delivery_lng are required for a delivery order".into(),
                ))
            }
        }
    };

    // Tenant from the validated token, never from app state or the body. This
    // is the money path: a tenant the caller could influence would let one
    // tenant's checkout settle against another tenant's basket and vendors.
    let outcome = st
        .checkout
        .place(claims.tenant_id, req.basket_id, req.tip_cents, delivery_lat, delivery_lng,
               &claims.email, claims.phone.as_deref(), req.delivery_note.as_deref(),
               req.payment_method,
               // Derived from the principal, never from the request body. A
               // client-supplied fulfilment would let any caller declare their
               // delivery order dine-in and skip the delivery fee.
               if claims.table_session {
                   crate::domain::entities::Fulfilment::DineIn
               } else {
                   crate::domain::entities::Fulfilment::Delivery
               })
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

    let PlaceOutcome { order, checkout_url } = outcome;

    // For COD, the courier is already offered the job at this point, so a
    // persist failure leaves a dispatched order we have no record of. For
    // online, `payments.authorize` has already opened a real hold, so the
    // same reasoning applies to the money instead of the courier. Either way
    // this is logged loudly with the order id so it can be reconciled by
    // hand; the alternative — acting only after a successful save — trades
    // this for orders that are recorded but never delivered.
    st.orders.save(&order).await.map_err(|e| {
        tracing::error!(err = %e, order_id = %order.id, courier_task_id = ?order.courier_task_id,
                        "order persist failed AFTER checkout side effects — needs manual reconciliation");
        (StatusCode::INTERNAL_SERVER_ERROR, "checkout failed".into())
    })?;

    // Placed. Published after persistence so a customer is never told about an
    // order that failed to save; a publish failure loses the confirmation, not
    // the order.
    if let Err(e) = st.order_events.order_placed(&order).await {
        tracing::error!(err = %e, order_id = %order.id, "order.placed publish failed");
    }

    // Tell each store about its own leg — the thing that, before ADR-0017, never
    // happened at all: a restaurant found out it had an order when a courier
    // walked in.
    //
    // COD only. An `Online` order has an authorization hold and nothing more
    // until the customer finishes the hosted checkout page, and a kitchen told
    // to start cooking then would be cooking for an order that may never be
    // paid. That branch publishes from the `payment.intent.authorized`
    // consumer instead — the same place, and for the same reason, that the
    // courier offer is deferred to.
    if order.payment_method == PaymentMethod::Cod {
        for leg in &order.legs {
            if let Err(e) = st
                .vendor_events
                .leg_received(&crate::infrastructure::messaging::LegRef::of(leg))
                .await
            {
                // The queue endpoint is the record, so a store still sees this
                // order on its next read. Losing the nudge is not losing the work.
                tracing::warn!(err = %e, order_id = %order.id, vendor_id = %leg.vendor_id,
                    "vendor.leg.received publish failed — the queue is still correct");
            }
        }
    }

    Ok(Json(CheckoutResponse {
        order_id:          order.id,
        grand_total_cents: order.grand_total_cents,
        stops:             order.legs.len(),
        checkout_url,
    }))
}
