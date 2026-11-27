//! Mesh-internal itemised quote, called by order-intake.
//!
//! No JWT: Istio mTLS asserts the caller, exactly as for /v1/internal/sla-records.
//! This route exists so a consumer can be priced off a carrier's rate card
//! without being granted `marketplace:book` — libs/auth/src/rbac.rs carries a
//! test asserting the `customer` role must never hold it.

use axum::{extract::State, http::StatusCode, response::Json};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use logisticos_errors::AppError;

use crate::domain::entities::{quote_breakdown_cents, QuoteBreakdown};
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct InternalQuoteRequest {
    pub tenant_id: Uuid,
    pub distance_km: f32,
    /// Billable weight — `max(scale, volumetric)`. order-intake computes it;
    /// carrier prices whatever it is handed.
    pub billable_kg: f32,
}

#[derive(Debug, Serialize)]
pub struct InternalQuoteResponse {
    pub listing_id: Uuid,
    pub carrier_id: Uuid,
    pub size_class: String,
    pub vehicle_label: String,
    pub breakdown: QuoteBreakdown,
    pub per_km_cents: i64,
    pub per_kg_cents: i64,
}

/// Cheapest of the already-capacity-filtered candidates.
///
/// Returns `None` for an empty list — the caller turns that into a business-rule
/// error. Never falls back to the largest or the cheapest-regardless.
fn cheapest<T: Copy>(candidates: &[(T, i64)]) -> Option<T> {
    candidates
        .iter()
        .min_by_key(|(_, total)| *total)
        .map(|(id, _)| *id)
}

/// `POST /v1/internal/quote-breakdown`
pub async fn internal_quote_breakdown(
    State(state): State<AppState>,
    Json(req): Json<InternalQuoteRequest>,
) -> Result<(StatusCode, Json<InternalQuoteResponse>), AppError> {
    // Capacity, active status and the idle window are all this service method's
    // existing rules — it calls find_available_listings, which filters on all
    // three. Do not re-filter here; a second availability rule is one that will
    // drift from the marketplace's.
    let listings = state
        .marketplace_svc
        .find_available_listings(req.tenant_id, req.billable_kg, None, 50)
        .await?;

    if listings.is_empty() {
        return Err(AppError::BusinessRule(format!(
            "No listed vehicle is available right now that can carry {:.0} kg",
            req.billable_kg,
        )));
    }

    let priced: Vec<(Uuid, i64)> = listings
        .iter()
        .map(|l| {
            (
                l.id,
                quote_breakdown_cents(l, req.distance_km, req.billable_kg).total_cents,
            )
        })
        .collect();

    let chosen_id = cheapest(&priced).expect("listings is non-empty");

    let listing = listings
        .iter()
        .find(|l| l.id == chosen_id)
        .expect("chosen_id came from this list");

    let breakdown = quote_breakdown_cents(listing, req.distance_km, req.billable_kg);

    Ok((
        StatusCode::OK,
        Json(InternalQuoteResponse {
            listing_id: listing.id,
            carrier_id: listing.carrier_id,
            size_class: listing.size_class.as_str().to_owned(),
            vehicle_label: listing.size_class.label().to_owned(),
            breakdown,
            per_km_cents: listing.per_km_cents,
            per_kg_cents: listing.per_kg_cents.unwrap_or(0),
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Thinking screen's step 2 claims the agent "sized the vehicle". That
    /// has to be a real selection: the cheapest of the listings that can carry
    /// the load. Capacity filtering is `find_available_listings`' job — this
    /// picks the cheapest of what it returns.
    #[test]
    fn it_picks_the_cheapest_of_the_capable_listings() {
        let candidates = vec![("van", 45_000_i64), ("truck", 90_000_i64)];
        assert_eq!(cheapest(&candidates), Some("van"));
    }

    /// An empty candidate list is an error with a reason, not a silent fallback
    /// to the largest or a zero price.
    #[test]
    fn nothing_available_is_an_error_not_a_fallback() {
        let candidates: Vec<(&str, i64)> = vec![];
        assert_eq!(cheapest(&candidates), None);
    }
}
