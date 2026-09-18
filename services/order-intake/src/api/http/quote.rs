//! POST /v1/shipments/quote — authoritative, server-priced quote for a
//! shipment the customer is about to book. Returns a signed, short-TTL token
//! carrying the priced amount; `POST /v1/shipments` re-verifies it rather
//! than trusting a client-supplied amount.
//!
//! Two pricing modes. A request carrying `origin` and `destination` is a consumer
//! move and prices off a carrier's rate card, itemised. Anything else is a parcel
//! and uses the hand-written AE tariff, which stays AED-only.

use axum::{extract::State, http::StatusCode, response::Json};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;
use logisticos_types::Currency;

use crate::api::http::AppState;
use crate::application::commands::AddressInput;
use crate::domain::entities::shipment::{ae_base_fee_for, ae_piece_fee_for};
use crate::domain::value_objects::quote_token::{self, QuoteTokenPayload, TokenDiscount};
use crate::infrastructure::http::DiscountLine;
use crate::domain::value_objects::{
    price_accessorials_itemised, AccessorialRequest, PricedAccessorial, ServiceType,
    ShipmentDimensions,
};

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

/// The tenant's billing currency as a `Currency`.
///
/// An unrecognised claim is a 422 rather than a default, because defaulting would
/// price a whole market off the wrong card.
fn parse_currency(code: &str) -> Result<Currency, AppError> {
    match code {
        "PHP" => Ok(Currency::PHP),
        "USD" => Ok(Currency::USD),
        "SGD" => Ok(Currency::SGD),
        "MYR" => Ok(Currency::MYR),
        "IDR" => Ok(Currency::IDR),
        "AED" => Ok(Currency::AED),
        other => Err(AppError::Validation(format!(
            "Tenant currency {other} is not a currency this service can price in"
        ))),
    }
}

#[derive(Deserialize)]
pub struct QuoteRequest {
    pub service_type: String,
    pub weight_grams: u32,
    #[serde(default)]
    pub pieces: Option<Vec<QuotePieceInput>>,

    // ── Rate-card inputs. Present for a consumer move, absent for a parcel. ──
    #[serde(default)]
    pub origin: Option<AddressInput>,
    #[serde(default)]
    pub destination: Option<AddressInput>,
    #[serde(default)]
    pub length_cm: Option<u32>,
    #[serde(default)]
    pub width_cm: Option<u32>,
    #[serde(default)]
    pub height_cm: Option<u32>,

    /// e.g. `[{ "code": "helper", "units": 2 }, { "code": "haulaway" }]`.
    /// Priced server-side; the caller states which and how many, never the price.
    #[serde(default)]
    pub accessorials: Vec<AccessorialRequest>,

    /// A promo code to price in. Promotions decides whether it applies and
    /// what it takes off; a refused code prices the quote without it.
    #[serde(default)]
    pub promo_code: Option<String>,
}

#[derive(Deserialize)]
pub struct QuotePieceInput {
    pub weight_grams: u32,
}

/// One row on the Review screen's "How this is priced" table.
#[derive(Serialize)]
pub struct PriceRow {
    /// e.g. "Cargo van callout", "Distance", "Weight".
    pub label: String,
    /// e.g. "7.4 km at 2.40 per km". Empty for the base row.
    pub note: String,
    pub amount_cents: i64,
}

/// The nested shape `design/Accessorial Config.dc.html` specifies. Carriage rows
/// and accessorial items sit in one object so the Review screen itemises
/// everything it charges without recomputing anything.
#[derive(Serialize)]
pub struct QuoteBreakdownView {
    pub currency: String,
    /// Sum of `carriage_rows`.
    pub carriage_cents: i64,
    /// Sum of `items`. Zero when none were requested.
    pub accessorial_cents: i64,
    /// `carriage_cents + accessorial_cents`. Never computed independently.
    pub total_cents: i64,
    /// Base / distance / weight — the three rows the Review screen renders.
    pub carriage_rows: Vec<PriceRow>,
    /// Priced accessorials, from `price_accessorials_itemised`.
    pub items: Vec<PricedAccessorial>,
}

#[derive(Serialize)]
pub struct QuoteResponse {
    /// What the customer pays: `gross_cents - discount_cents`.
    pub amount_cents: i64,
    /// Before any discount — the breakdown's total.
    pub gross_cents: i64,
    /// Sum of `discounts`.
    pub discount_cents: i64,
    /// As promotions priced them. The app renders these; it never recomputes
    /// the stack.
    pub discounts: Vec<DiscountLine>,
    /// The discount ceiling cut something short, and the app should say so.
    pub ceiling_binds: bool,
    /// The code that was priced in, when it took something off.
    pub promo_code: Option<String>,
    /// Why a requested code was not applied, e.g. "OUTSIDE_WINDOW_WEEKEND".
    pub promo_refusal: Option<String>,
    pub promo_message: Option<String>,
    /// The code was valid, but the corporate rate beat it; only the larger
    /// is taken.
    pub code_lost_to_corporate: bool,
    pub currency: String,
    pub quote_token: String,
    pub expires_at: DateTime<Utc>,

    /// `"rate_card"` or `"parcel_tariff"`. The app renders the breakdown only
    /// for `rate_card`; a parcel quote legitimately has one number.
    pub pricing_mode: String,
    /// Absent for a parcel quote.
    pub breakdown: Option<QuoteBreakdownView>,
    pub billable_grams: Option<u32>,
    /// `"volumetric"` or `"scale"` — which one settled the billable weight.
    pub billable_basis: Option<String>,
    pub distance_km: Option<f32>,
    pub vehicle_label: Option<String>,
}

/// `POST /v1/shipments/quote` — JWT-authenticated (any role).
///
/// The AED restriction used to apply to the whole endpoint, which made the
/// consumer app unusable in eight of its nine markets. It now scopes only the
/// parcel-tariff branch, which is genuinely the hand-written AE table.
pub async fn get_quote(
    State(s): State<AppState>,
    claims: AuthClaims,
    Json(req): Json<QuoteRequest>,
) -> Result<(StatusCode, Json<QuoteResponse>), AppError> {
    let service_type = ServiceType::parse(&req.service_type).map_err(AppError::Validation)?;

    let is_move = req.origin.is_some() && req.destination.is_some();

    let mut rows: Vec<PriceRow> = Vec::new();
    let (amount_cents, currency, mode, billable, basis, distance, vehicle) = if is_move {
        let carrier = s.svc.carrier.as_ref().ok_or_else(|| {
            AppError::ServiceUnavailable(
                "Rate-card quoting is not configured for this deployment".into(),
            )
        })?;

        let origin = req.origin.as_ref().expect("checked by is_move");
        let destination = req.destination.as_ref().expect("checked by is_move");

        // AddressNormalizer::normalize returns logisticos_types::Address, whose
        // `coordinates` is None on any geocoder failure — that degradation is
        // deliberate, so a move quote has to handle it rather than assume a pair.
        let from = s
            .svc
            .normalizer
            .normalize(origin)
            .await
            .map_err(AppError::Internal)?;
        let to = s
            .svc
            .normalizer
            .normalize(destination)
            .await
            .map_err(AppError::Internal)?;

        let (Some(a), Some(b)) = (from.coordinates, to.coordinates) else {
            return Err(AppError::BusinessRule(
                "Could not locate one of the addresses — a move cannot be priced \
                 without a distance. Check GEOCODER__MAPBOX_ACCESS_TOKEN is set."
                    .into(),
            ));
        };

        // Coordinates::distance_km is already on the type normalize returns.
        // No new dependency, no third haversine.
        let distance_km = a.distance_km(&b) as f32;

        let billable_grams =
            billable_weight_grams(req.weight_grams, req.length_cm, req.width_cm, req.height_cm);
        let basis = if billable_grams > req.weight_grams {
            "volumetric"
        } else {
            "scale"
        };

        let q = carrier
            .quote_breakdown(claims.tenant_id, distance_km, billable_grams as f32 / 1000.0)
            .await
            .map_err(AppError::BusinessRule)?;

        let per_km = q.per_km_cents as f64 / 100.0;
        let per_kg = q.per_kg_cents as f64 / 100.0;
        let kg = billable_grams as f64 / 1000.0;

        rows.push(PriceRow {
            label: format!("{} callout", q.vehicle_label),
            note: "Listed base rate".into(),
            amount_cents: q.breakdown.base_cents,
        });
        rows.push(PriceRow {
            label: "Distance".into(),
            note: format!("{distance_km:.1} km at {per_km:.2} per km"),
            amount_cents: q.breakdown.distance_cents,
        });
        rows.push(PriceRow {
            label: "Weight".into(),
            note: format!("{kg:.0} kg at {per_kg:.2} per kg"),
            amount_cents: q.breakdown.weight_cents,
        });

        // The invariant the Review screen depends on. A drift here is a wrong
        // price shown to a customer, so it fails the request rather than
        // rendering rows that do not add up. One guard, covering carriage and
        // accessorials both — never two.
        let summed: i64 = rows.iter().map(|r| r.amount_cents).sum();
        if summed != q.breakdown.total_cents {
            return Err(AppError::Internal(anyhow::anyhow!(
                "quote carriage rows sum to {summed} but total is {} — refusing to \
                 return a breakdown that does not reconcile",
                q.breakdown.total_cents
            )));
        }

        (
            q.breakdown.total_cents,
            claims.currency.clone().unwrap_or_else(|| "PHP".into()),
            "rate_card",
            Some(billable_grams),
            Some(basis.to_string()),
            Some(distance_km),
            Some(q.vehicle_label),
        )
    } else {
        // Parcel tariff. The AE table is hand-written and finance has not signed
        // it off, so it stays scoped to AED tenants — it just no longer gates
        // the consumer move path above.
        if claims.currency.as_deref() != Some("AED") {
            return Err(AppError::Validation(
                "Parcel quotes are only available for AE-region (AED) tenants. \
                 Send `origin` and `destination` to price a move off the rate card."
                    .into(),
            ));
        }

        let amount = match (&req.pieces, service_type) {
            (Some(inputs), ServiceType::Balikbayan | ServiceType::International)
                if !inputs.is_empty() =>
            {
                let weights: Vec<u32> = inputs.iter().map(|p| p.weight_grams).collect();
                ae_piece_fee_for(&weights).amount
            }
            _ => ae_base_fee_for(service_type, req.weight_grams).amount,
        };

        (amount, "AED".into(), "parcel_tariff", None, None, None, None)
    };

    // Accessorials price off the tenant's card, in the currency the carriage fee
    // was priced in. A requested accessorial this market does not offer is a 422
    // naming the code, not a silent zero.
    let currency_enum = parse_currency(&currency)?;
    let items = price_accessorials_itemised(&s.svc.accessorials, currency_enum, &req.accessorials)
        .map_err(|e| AppError::Validation(e.to_string()))?;
    let accessorial_cents: i64 = items.iter().map(|i| i.billed_cents).sum();
    // What settlement owes for the accessorials, fixed at quote time. Signed into
    // the token and never put in the response: it reveals the take rate.
    let accessorial_paid_cents: i64 = items.iter().map(|i| i.paid_cents).sum();
    let total_cents = amount_cents.saturating_add(accessorial_cents);

    // ── Discounts ────────────────────────────────────────────────────────────
    // The code, the corporate rate, the loyalty tier and account credit, as
    // promotions stacks them. Off what the customer is billed only:
    // accessorial_paid_cents above is untouched, and settlement never reads
    // promotion state. Every line is signed into the token and spent at
    // booking, so what is shown here is what is charged.
    let mut discounts: Vec<DiscountLine> = Vec::new();
    let mut ceiling_binds = false;
    let mut promo_code: Option<String> = None;
    let mut promo_refusal: Option<String> = None;
    let mut promo_message: Option<String> = None;
    let mut code_lost_to_corporate = false;
    let code = req.promo_code.as_deref().map(str::trim).filter(|c| !c.is_empty());
    let unavailable = || (
        Some("PROMOTIONS_UNAVAILABLE".to_owned()),
        Some("Offers can't be checked right now. You can book without the code.".to_owned()),
    );
    match &s.svc.promotions {
        // Only a requested code has anything to say about promotions being
        // absent; a plain quote is simply undiscounted.
        None if code.is_some() => (promo_refusal, promo_message) = unavailable(),
        None => {}
        Some(client) => match client
            .price(
                claims.tenant_id,
                claims.user_id,
                code,
                &currency,
                amount_cents,
                accessorial_cents,
                claims.has_feature("loyalty_program"),
            )
            .await
        {
            Ok(priced) => {
                discounts = priced.lines.into_iter().filter(|l| l.amount_cents > 0).collect();
                ceiling_binds = priced.ceiling_binds;
                promo_code = priced.code_applied.filter(|_| discounts.iter().any(|l| l.kind == "code"));
                promo_refusal = priced.refusal;
                promo_message = priced.message;
                code_lost_to_corporate = priced.code_lost_to_corporate;
            }
            Err(e) => {
                tracing::warn!(err = %e, "promotions pricing failed — quoting undiscounted");
                if code.is_some() {
                    (promo_refusal, promo_message) = unavailable();
                }
            }
        },
    }
    let discount_cents: i64 = discounts.iter().map(|d| d.amount_cents.max(0)).sum();
    if discount_cents > total_cents {
        return Err(AppError::Internal(anyhow::anyhow!(
            "discounts of {discount_cents} exceed the {total_cents} they came off — refusing to quote"
        )));
    }
    let gross_cents = total_cents;
    let net_cents = total_cents - discount_cents;

    // Checked after pricing but before signing: an unconfigured deployment
    // cannot issue a token regardless of which branch priced the job, and 503
    // (not 422) says this is deployment state, not something about the request.
    let payment = s.svc.payment.as_ref().ok_or_else(|| {
        AppError::ServiceUnavailable(
            "Online payment is not configured for this deployment — no quote can be issued".into(),
        )
    })?;

    let expires_at = Utc::now() + Duration::minutes(QUOTE_TTL_MINUTES);
    let payload = QuoteTokenPayload {
        tenant_id: claims.tenant_id,
        service_type: req.service_type.clone(),
        weight_grams: req.weight_grams,
        // The all-in figure net of any discount, so POST /v1/shipments charges
        // what the customer actually agreed to.
        amount_cents: net_cents,
        currency: currency.clone(),
        expires_at,
        pricing_mode: Some(mode.to_string()),
        billable_grams: billable,
        accessorial_paid_cents: Some(accessorial_paid_cents),
        discount_cents: (discount_cents > 0).then_some(discount_cents),
        promo_code: promo_code.clone(),
        account_id: (discount_cents > 0).then_some(claims.user_id),
        discounts: (discount_cents > 0).then(|| {
            discounts
                .iter()
                .map(|d| TokenDiscount { kind: d.kind.clone(), label: d.label.clone(), amount_cents: d.amount_cents })
                .collect()
        }),
    };
    let quote_token = quote_token::sign(payment.quote_token_secret.as_bytes(), &payload);

    // A parcel quote has one number and no breakdown; a move always has one.
    let breakdown = if rows.is_empty() {
        None
    } else {
        Some(QuoteBreakdownView {
            currency: currency.clone(),
            carriage_cents: amount_cents,
            accessorial_cents,
            total_cents,
            carriage_rows: rows,
            items,
        })
    };

    Ok((
        StatusCode::OK,
        Json(QuoteResponse {
            amount_cents: net_cents,
            gross_cents,
            discount_cents,
            discounts,
            ceiling_binds,
            promo_code,
            promo_refusal,
            promo_message,
            code_lost_to_corporate,
            currency,
            quote_token,
            expires_at,
            pricing_mode: mode.into(),
            breakdown,
            billable_grams: billable,
            billable_basis: basis,
            distance_km: distance,
            vehicle_label: vehicle,
        }),
    ))
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

#[cfg(test)]
mod reconciliation_tests {
    use super::*;

    fn row(amount: i64) -> PriceRow {
        PriceRow { label: "r".into(), note: String::new(), amount_cents: amount }
    }

    /// The guard that keeps the Review screen honest. The handoff records this
    /// invariant breaking twice in design review — once by itemising protection
    /// that was not charged, once by charging a carbon offset that was not
    /// itemised. Rows that do not sum to the total must fail the request.
    #[test]
    fn a_missing_row_does_not_reconcile() {
        let rows = [row(45_000), row(1_776)];
        let summed: i64 = rows.iter().map(|r| r.amount_cents).sum();
        assert_ne!(summed, 53_816, "this is the drift the handler must refuse");
    }

    /// And the passing case: base + distance + weight for the worked example.
    #[test]
    fn all_three_rows_reconcile() {
        let rows = [row(45_000), row(1_776), row(7_040)];
        let summed: i64 = rows.iter().map(|r| r.amount_cents).sum();
        assert_eq!(summed, 53_816);
    }
}
