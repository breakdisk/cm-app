//! POST /v1/shipments/quote — authoritative, server-priced quote for a
//! shipment the customer is about to book. Returns a signed, short-TTL token
//! carrying the priced amount; `POST /v1/shipments` re-verifies it rather
//! than trusting a client-supplied amount. AE-region (AED) tenants only —
//! other currencies keep using the existing cash-at-pickup flow and never
//! call this endpoint.

use axum::{extract::State, http::StatusCode, response::{IntoResponse, Json}};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;

use crate::api::http::AppState;
use crate::domain::entities::shipment::{ae_base_fee_for, ae_piece_fee_for};
use crate::domain::value_objects::quote_token::{self, QuoteTokenPayload};
use crate::domain::value_objects::{ServiceType, ShipmentDimensions};

/// Quote token validity — short enough that a stale review screen can't be
/// used to lock in a price from an hour ago, long enough to cover filling out
/// the rest of the booking form.
const QUOTE_TTL_MINUTES: i64 = 15;

/// Billable weight is `max(scale, volumetric)`, at the DIM 5000 cm³/kg factor
/// the Review screen states to the customer.
///
/// Delegates to `ShipmentDimensions::volumetric_weight_grams` — that is where the
/// DIM factor lives and `Piece::billable_weight_grams` already uses it. A second
/// copy of the arithmetic here is one that would drift from the piece-level one.
///
/// With any dimension absent there is no volume, so scale weight stands: an
/// unscanned item must never price at zero.
fn billable_weight_grams(
    scale_grams: u32,
    length_cm: Option<u32>,
    width_cm: Option<u32>,
    height_cm: Option<u32>,
) -> u32 {
    let volumetric = match (length_cm, width_cm, height_cm) {
        (Some(length_cm), Some(width_cm), Some(height_cm)) => {
            ShipmentDimensions { length_cm, width_cm, height_cm }.volumetric_weight_grams()
        }
        _ => 0,
    };
    scale_grams.max(volumetric)
}

#[derive(Deserialize)]
pub struct QuoteRequest {
    pub service_type: String,
    pub weight_grams: u32,
    #[serde(default)]
    pub pieces: Option<Vec<QuotePieceInput>>,
}

#[derive(Deserialize)]
pub struct QuotePieceInput {
    pub weight_grams: u32,
}

#[derive(Serialize)]
pub struct QuoteResponse {
    pub amount_cents: i64,
    pub currency: String,
    pub quote_token: String,
    pub expires_at: DateTime<Utc>,
}

/// `POST /v1/shipments/quote` — JWT-authenticated (any role). Gated on the
/// tenant's billing currency, not a role/permission, since this is purely a
/// pricing lookup: any authenticated caller for an AE-region tenant may quote.
pub async fn get_quote(
    State(s): State<AppState>,
    claims: AuthClaims,
    Json(req): Json<QuoteRequest>,
) -> impl IntoResponse {
    // Checked first, ahead of the AED-tenant business rule below: an
    // unconfigured deployment can't quote for anyone regardless of tenant
    // currency, and 503 (not 422) tells the caller this is a deployment
    // state, not something about their request.
    let payment = match s.svc.payment.as_ref() {
        Some(p) => p,
        None => {
            return Err(AppError::ServiceUnavailable(
                "Online payment is not configured for this deployment — no quote can be issued".into(),
            ));
        }
    };

    if claims.currency.as_deref() != Some("AED") {
        return Err::<_, AppError>(AppError::Validation(
            "Online quotes are only available for AE-region (AED) tenants".into(),
        ));
    }

    let service_type = match ServiceType::parse(&req.service_type) {
        Ok(st) => st,
        Err(e) => return Err(AppError::Validation(e)),
    };

    let amount_cents = match (&req.pieces, service_type) {
        (Some(inputs), ServiceType::Balikbayan | ServiceType::International) if !inputs.is_empty() => {
            let weights: Vec<u32> = inputs.iter().map(|p| p.weight_grams).collect();
            ae_piece_fee_for(&weights).amount
        }
        _ => ae_base_fee_for(service_type, req.weight_grams).amount,
    };

    let expires_at = Utc::now() + Duration::minutes(QUOTE_TTL_MINUTES);
    let payload = QuoteTokenPayload {
        tenant_id: claims.tenant_id,
        service_type: req.service_type.clone(),
        weight_grams: req.weight_grams,
        amount_cents,
        currency: "AED".into(),
        expires_at,
    };
    let quote_token = quote_token::sign(payment.quote_token_secret.as_bytes(), &payload);

    Ok((StatusCode::OK, Json(QuoteResponse {
        amount_cents,
        currency: "AED".into(),
        quote_token,
        expires_at,
    })))
}

#[cfg(test)]
mod billable_weight_tests {
    use super::*;

    /// The handoff's worked example: a 1.6 m3 sofa at 187 kg on the scale prices
    /// on volume. 160x100x100 cm = 1_600_000 cm3 / 5000 = 320 kg volumetric.
    #[test]
    fn a_light_bulky_load_prices_on_volume() {
        let g = billable_weight_grams(187_000, Some(160), Some(100), Some(100));
        assert_eq!(g, 320_000);
    }

    /// A dense load prices on the scale.
    #[test]
    fn a_dense_load_prices_on_the_scale() {
        let g = billable_weight_grams(400_000, Some(50), Some(40), Some(40));
        assert_eq!(g, 400_000, "80 kg volumetric must not beat 400 kg on the scale");
    }

    /// Missing dimensions fall back to scale weight. An unscanned item must not
    /// price at zero.
    #[test]
    fn missing_dimensions_fall_back_to_scale_weight() {
        assert_eq!(billable_weight_grams(187_000, None, None, None), 187_000);
        assert_eq!(billable_weight_grams(187_000, Some(160), None, Some(100)), 187_000);
    }
}
