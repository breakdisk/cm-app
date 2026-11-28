//! Signed, short-TTL quote token. `POST /v1/shipments/quote` mints one;
//! `POST /v1/shipments` re-verifies one before trusting its amount to charge.
//! No database row is created for a quote nobody completes.

use base64::Engine as _;
use crate::application::commands::AddressInput;
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

/// One discount the quote was priced with, signed so the booking spends
/// exactly what the customer was shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenDiscount {
    /// "code" | "corporate" | "tier" | "credit".
    pub kind: String,
    pub label: String,
    pub amount_cents: i64,
}

/// Current token format. Version 2 binds the route the quote was priced on
/// (`QuoteTokenPayload::route`). Tokens signed before it deserialize with
/// `v == 0` and are refused by `verify()`: the TTL is minutes, so asking the
/// caller to re-quote costs one request, whereas honouring them would keep
/// the unbound-destination hole open for a full TTL after deploy.
pub const TOKEN_VERSION: u8 = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuoteTokenPayload {
    /// Format version; `0` (absent) for tokens from before route binding.
    #[serde(default)]
    pub v: u8,
    /// `route_fingerprint` of the origin and destination the price was
    /// computed on. `Some` for any quote whose price depends on the route
    /// (rate-card and home-move); `None` only for the flat parcel tariff,
    /// which is priced on service type and weight alone.
    #[serde(default)]
    pub route: Option<String>,
    pub tenant_id: Uuid,
    pub service_type: String,
    pub weight_grams: u32,
    pub amount_cents: i64,
    pub currency: String,
    pub expires_at: DateTime<Utc>,
    /// `"rate_card"` or `"parcel_tariff"`. Signed so a rate-card total cannot be
    /// presented to `POST /v1/shipments` and re-verified against the parcel
    /// tariff, or the reverse.
    ///
    /// `#[serde(default)]` on both new fields is deliberate: tokens signed by the
    /// previous build are still inside their 15-minute TTL at the moment this
    /// deploys, and a caller holding one should not get a malformed-token error
    /// for a price we issued ourselves.
    #[serde(default)]
    pub pricing_mode: Option<String>,
    /// Billable weight the price was computed on. Signed for the same reason as
    /// `weight_grams`: the create call must not be able to re-declare a lighter
    /// load than the one that was quoted.
    #[serde(default)]
    pub billable_grams: Option<u32>,
    /// What settlement owes for the accessorials on this quote, fixed when the
    /// price was issued so a later card change cannot re-price a booked job's
    /// driver pay. `#[serde(default)]` for tokens signed before this field.
    #[serde(default)]
    pub accessorial_paid_cents: Option<i64>,
    /// Everything the discounts took off. `amount_cents` is already net of it.
    #[serde(default)]
    pub discount_cents: Option<i64>,
    /// The code that was priced in, to be spent when the booking is created.
    #[serde(default)]
    pub promo_code: Option<String>,
    /// Who the discount was priced for. A code, a tier and credit are all one
    /// account's, so a discounted token books only for that account.
    #[serde(default)]
    pub account_id: Option<Uuid>,
    /// Every discount line, each spent at booking. Absent on tokens signed
    /// before tiers, credit and corporate rates were charged: those carried a
    /// code only, and `discount_lines()` reads them as one code line.
    #[serde(default)]
    pub discounts: Option<Vec<TokenDiscount>>,
}

impl QuoteTokenPayload {
    /// The booking must be for the route the quote was priced on. A token with
    /// no route is accepted only when it is the flat parcel tariff; a
    /// rate-card or home-move quote without one is refused, so stripping the
    /// binding is no way round it.
    pub fn check_route(&self, origin: &AddressInput, destination: &AddressInput) -> Result<(), RouteMismatch> {
        match &self.route {
            Some(fp) if *fp == route_fingerprint(origin, destination) => Ok(()),
            Some(_) => Err(RouteMismatch::Different),
            None if matches!(self.pricing_mode.as_deref(), Some("rate_card") | Some("home_move")) => {
                Err(RouteMismatch::Unbound)
            }
            None => Ok(()),
        }
    }

    /// The lines this quote's booking must spend.
    pub fn discount_lines(&self) -> Vec<TokenDiscount> {
        match (&self.discounts, &self.promo_code, self.discount_cents) {
            (Some(lines), _, _) => lines.clone(),
            (None, Some(code), Some(cents)) if cents > 0 => {
                vec![TokenDiscount { kind: "code".into(), label: code.clone(), amount_cents: cents }]
            }
            _ => Vec::new(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum QuoteTokenError {
    #[error("malformed quote token")]
    Malformed,
    #[error("quote token signature is invalid")]
    BadSignature,
    #[error("quote token has expired")]
    Expired,
    #[error("quote was issued by an older version of the service - request a new quote")]
    Outdated,
}

/// A booking whose origin/destination are not the ones the quote was priced on.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RouteMismatch {
    #[error("this quote was priced for a different origin or destination - request a new quote for this route")]
    Different,
    #[error("this quote carries no route but its pricing needs one - request a new quote")]
    Unbound,
}

/// Lowercased, punctuation and spacing folded to single spaces, so
/// "12  Rizal Ave." and "12 rizal ave" are one address.
fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut gap = true;
    for c in s.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
            gap = false;
        } else if !gap {
            out.push(' ');
            gap = true;
        }
    }
    out.trim_end().to_owned()
}

/// Postal codes compare without any separator ("1605", "1605 ", "SW1A 1AA" = "sw1a1aa").
fn fold_compact(s: &str) -> String {
    fold(s).replace(' ', "")
}

/// A stable digest of the address fields a distance-based price depends on:
/// street line, city, postal code and country, for both ends. Province,
/// barangay, line 2 and the UAE building/unit/area parts are left out: they
/// are formatting-variable across callers (several send province = city) and
/// do not move the geocoded point. The customer pin is left out too: the
/// quote geocodes the text and never reads it.
pub fn route_fingerprint(origin: &AddressInput, destination: &AddressInput) -> String {
    use sha2::Digest as _;
    let leg = |a: &AddressInput| {
        format!(
            "{}|{}|{}|{}",
            fold(&a.line1),
            fold(&a.city),
            fold_compact(&a.postal_code),
            a.country_code.trim().to_ascii_uppercase(),
        )
    };
    let canonical = format!("o:{}\nd:{}", leg(origin), leg(destination));
    let digest = Sha256::digest(canonical.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Sign a payload into `base64(json).base64(hmac-sha256)`.
pub fn sign(secret: &[u8], payload: &QuoteTokenPayload) -> String {
    sign_payload(secret, payload)
}

/// The signing core, for any serializable payload. Callers that sign more
/// than one kind of payload with one secret must put a discriminator in the
/// payload itself (see `home_move::HomeQuotePayload::kind`), so one kind can
/// never be presented as another.
pub fn sign_payload<T: Serialize>(secret: &[u8], payload: &T) -> String {
    let json = serde_json::to_vec(payload).expect("quote payloads always serialize");
    let json_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&json);

    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(json_b64.as_bytes());
    let sig_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());

    format!("{json_b64}.{sig_b64}")
}

/// Verify a token's signature and expiry. Does NOT check that the payload
/// matches the shipment actually being booked — the caller does that, since
/// only it knows what "matches" means for the request in hand.
pub fn verify(secret: &[u8], token: &str) -> Result<QuoteTokenPayload, QuoteTokenError> {
    let payload: QuoteTokenPayload = verify_payload(secret, token)?;
    if payload.expires_at < Utc::now() {
        return Err(QuoteTokenError::Expired);
    }
    if payload.v < TOKEN_VERSION {
        return Err(QuoteTokenError::Outdated);
    }
    Ok(payload)
}

/// The verifying core: the signature, then the shape. Expiry is the caller's,
/// since only it knows which field holds it.
pub fn verify_payload<T: serde::de::DeserializeOwned>(secret: &[u8], token: &str) -> Result<T, QuoteTokenError> {
    let (json_b64, sig_b64) = token.split_once('.').ok_or(QuoteTokenError::Malformed)?;

    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(json_b64.as_bytes());
    let expected = mac.finalize().into_bytes();

    let provided = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(sig_b64)
        .map_err(|_| QuoteTokenError::Malformed)?;

    if expected.as_slice().ct_eq(&provided).unwrap_u8() != 1 {
        return Err(QuoteTokenError::BadSignature);
    }

    let json = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(json_b64)
        .map_err(|_| QuoteTokenError::Malformed)?;
    serde_json::from_slice(&json).map_err(|_| QuoteTokenError::Malformed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn make_payload(ttl_minutes: i64) -> QuoteTokenPayload {
        QuoteTokenPayload {
            v: TOKEN_VERSION,
            route: None,
            tenant_id: Uuid::new_v4(),
            service_type: "standard".into(),
            weight_grams: 1_500,
            amount_cents: 2_200,
            currency: "AED".into(),
            expires_at: Utc::now() + Duration::minutes(ttl_minutes),
            pricing_mode: Some("parcel_tariff".into()),
            billable_grams: None,
            accessorial_paid_cents: None,
            discount_cents: None,
            promo_code: None,
            account_id: None,
            discounts: None,
        }
    }

    #[test]
    fn a_token_from_before_discount_lines_reads_as_one_code_line() {
        let mut p = make_payload(15);
        p.discount_cents = Some(500);
        p.promo_code = Some("MOVE20".into());
        assert_eq!(
            p.discount_lines(),
            vec![TokenDiscount { kind: "code".into(), label: "MOVE20".into(), amount_cents: 500 }]
        );
        p.discounts = Some(vec![TokenDiscount { kind: "credit".into(), label: "Credit".into(), amount_cents: 300 }]);
        assert_eq!(p.discount_lines().len(), 1);
        assert_eq!(p.discount_lines()[0].kind, "credit");
    }

    fn addr(line1: &str, city: &str, postal: &str, country: &str) -> AddressInput {
        AddressInput {
            line1: line1.into(),
            line2: None,
            barangay: None,
            city: city.into(),
            province: "Metro Manila".into(),
            postal_code: postal.into(),
            country_code: country.into(),
            pin_lat: None,
            pin_lng: None,
            building: None,
            unit: None,
            area: None,
        }
    }

    fn route_token(origin: &AddressInput, dest: &AddressInput) -> QuoteTokenPayload {
        QuoteTokenPayload {
            route: Some(route_fingerprint(origin, dest)),
            pricing_mode: Some("rate_card".into()),
            ..make_payload(15)
        }
    }

    #[test]
    fn a_quote_for_one_destination_is_refused_at_another() {
        let origin = addr("123 Warehouse Road", "Pasig", "1605", "PH");
        let near = addr("456 Customer Street", "Quezon City", "1100", "PH");
        let far = addr("9 Harbour Drive", "Davao City", "8000", "PH");
        let p = route_token(&origin, &near);
        assert_eq!(p.check_route(&origin, &far), Err(RouteMismatch::Different));
        assert_eq!(p.check_route(&far, &near), Err(RouteMismatch::Different), "origin is bound too");
        assert_eq!(p.check_route(&origin, &near), Ok(()));
    }

    #[test]
    fn formatting_noise_does_not_break_a_legitimate_redemption() {
        let origin = addr("123 Warehouse Road", "Pasig", "1605", "PH");
        let dest = addr("456 Customer Street", "Quezon City", "1100", "PH");
        let p = route_token(&origin, &dest);
        let mut noisy = addr("  456  CUSTOMER street. ", "quezon  city", "1100 ", " ph");
        noisy.province = "Quezon City".into(); // callers set province = city
        noisy.line2 = Some("Unit 4".into());
        noisy.pin_lat = Some(14.6);
        noisy.pin_lng = Some(121.0);
        assert_eq!(p.check_route(&origin, &noisy), Ok(()));
    }

    #[test]
    fn a_rate_card_or_home_token_without_a_route_is_refused() {
        let a = addr("123 Warehouse Road", "Pasig", "1605", "PH");
        for mode in ["rate_card", "home_move"] {
            let p = QuoteTokenPayload { pricing_mode: Some(mode.into()), ..make_payload(15) };
            assert_eq!(p.check_route(&a, &a), Err(RouteMismatch::Unbound), "{mode}");
        }
        // The flat parcel tariff is not priced on the route.
        assert_eq!(make_payload(15).check_route(&a, &a), Ok(()));
    }

    #[test]
    fn a_token_from_before_route_binding_is_refused_with_a_clear_error() {
        let secret = b"test-secret";
        let legacy = QuoteTokenPayload { v: 0, ..make_payload(15) };
        let err = verify(secret, &sign(secret, &legacy)).expect_err("legacy format must not verify");
        assert!(matches!(err, QuoteTokenError::Outdated));
        assert!(err.to_string().contains("request a new quote"));
        // A token serialized without the field at all reads as v0.
        let mut json = serde_json::to_value(make_payload(15)).expect("json");
        json.as_object_mut().expect("object").remove("v");
        json.as_object_mut().expect("object").remove("route");
        let token = sign_payload(secret, &json);
        assert!(matches!(verify(secret, &token), Err(QuoteTokenError::Outdated)));
    }

    #[test]
    fn sign_then_verify_round_trips_the_payload() {
        let secret = b"test-secret";
        let payload = make_payload(15);
        let token = sign(secret, &payload);
        let verified = verify(secret, &token).expect("must verify");
        assert_eq!(verified, payload);
    }

    #[test]
    fn verify_rejects_a_tampered_payload() {
        let secret = b"test-secret";
        let token = sign(secret, &make_payload(15));
        let (_, sig) = token.split_once('.').unwrap();
        let tampered_payload = QuoteTokenPayload { amount_cents: 1, ..make_payload(15) };
        let tampered_json_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&tampered_payload).unwrap());
        let tampered_token = format!("{tampered_json_b64}.{sig}");
        assert!(matches!(verify(secret, &tampered_token), Err(QuoteTokenError::BadSignature)));
    }

    #[test]
    fn verify_rejects_the_wrong_secret() {
        let token = sign(b"secret-a", &make_payload(15));
        assert!(matches!(verify(b"secret-b", &token), Err(QuoteTokenError::BadSignature)));
    }

    #[test]
    fn verify_rejects_an_expired_token() {
        let secret = b"test-secret";
        let token = sign(secret, &make_payload(-1)); // already expired
        assert!(matches!(verify(secret, &token), Err(QuoteTokenError::Expired)));
    }

    #[test]
    fn verify_rejects_a_malformed_token() {
        assert!(matches!(verify(b"secret", "not-a-valid-token"), Err(QuoteTokenError::Malformed)));
    }
}
