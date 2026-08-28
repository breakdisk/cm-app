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
use crate::domain::value_objects::ServiceType;

/// Quote token validity — short enough that a stale review screen can't be
/// used to lock in a price from an hour ago, long enough to cover filling out
/// the rest of the booking form.
const QUOTE_TTL_MINUTES: i64 = 15;

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
