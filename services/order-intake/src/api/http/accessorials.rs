//! `GET /v1/accessorials` — what this tenant offers, for rendering the toggles.
//!
//! Response shape is exactly the one in `design/Accessorial Config.dc.html`, so
//! the consumer app's hardcoded `ACCESSORIALS` object is replaced one-for-one.
//!
//! `threshold` is absent on purpose. It is a promise about where the driver puts
//! the item, not a charge, so it carries no rate and the UI renders its toggle
//! without one.

use axum::{extract::State, http::StatusCode, response::Json};
use serde::Serialize;

use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;

use crate::api::http::AppState;
use crate::config::AccessorialBasis;

#[derive(Serialize)]
pub struct AccessorialItem {
    pub code: &'static str,
    pub amount_cents: i64,
    /// `"booking"` or `"stair_flight"`.
    pub basis: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_units: Option<u16>,
}

#[derive(Serialize)]
pub struct AccessorialsResponse {
    pub currency: String,
    pub items: Vec<AccessorialItem>,
}

/// `GET /v1/accessorials` — JWT-authenticated, any role. Like the quote itself
/// this is a pricing lookup, not a mutation, so it is gated on the tenant's
/// configuration rather than a permission.
pub async fn list_accessorials(
    State(s): State<AppState>,
    claims: AuthClaims,
) -> Result<(StatusCode, Json<AccessorialsResponse>), AppError> {
    let currency = claims.currency.clone().unwrap_or_else(|| "PHP".into());

    // Only entries priced in the caller's own currency are offered. A card left
    // configured for another market would otherwise render a toggle that the
    // quote then refuses with a currency mismatch.
    let items = s
        .svc
        .accessorials
        .offered()
        .into_iter()
        .filter(|(_, rate)| rate.currency == currency)
        .map(|(code, rate)| AccessorialItem {
            code,
            // Billed only. paid_cents is settlement data and never leaves the server.
            amount_cents: rate.billed_cents,
            basis: match rate.basis {
                AccessorialBasis::Booking => "booking",
                AccessorialBasis::StairFlight => "stair_flight",
            },
            max_units: rate.max_units,
        })
        .collect();

    Ok((StatusCode::OK, Json(AccessorialsResponse { currency, items })))
}
