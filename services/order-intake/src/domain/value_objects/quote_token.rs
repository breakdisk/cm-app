//! Signed, short-TTL quote token. `POST /v1/shipments/quote` mints one;
//! `POST /v1/shipments` re-verifies one before trusting its amount to charge.
//! No database row is created for a quote nobody completes.

use base64::Engine as _;
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuoteTokenPayload {
    pub tenant_id: Uuid,
    pub service_type: String,
    pub weight_grams: u32,
    pub amount_cents: i64,
    pub currency: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum QuoteTokenError {
    #[error("malformed quote token")]
    Malformed,
    #[error("quote token signature is invalid")]
    BadSignature,
    #[error("quote token has expired")]
    Expired,
}

/// Sign a payload into `base64(json).base64(hmac-sha256)`.
pub fn sign(secret: &[u8], payload: &QuoteTokenPayload) -> String {
    let json = serde_json::to_vec(payload).expect("QuoteTokenPayload always serializes");
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
    let payload: QuoteTokenPayload = serde_json::from_slice(&json)
        .map_err(|_| QuoteTokenError::Malformed)?;

    if payload.expires_at < Utc::now() {
        return Err(QuoteTokenError::Expired);
    }

    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn make_payload(ttl_minutes: i64) -> QuoteTokenPayload {
        QuoteTokenPayload {
            tenant_id: Uuid::new_v4(),
            service_type: "standard".into(),
            weight_grams: 1_500,
            amount_cents: 2_200,
            currency: "AED".into(),
            expires_at: Utc::now() + Duration::minutes(ttl_minutes),
        }
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
