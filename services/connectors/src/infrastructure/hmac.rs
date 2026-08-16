//! HMAC-SHA256 verification for inbound platform webhooks.
//!
//! Both Shopify and WooCommerce use base64-encoded HMAC-SHA256 of the raw request body,
//! signed with a platform-specific secret. Verification uses constant-time comparison
//! via the `subtle` crate to prevent timing-attack leakage of the expected value.

use base64::Engine as _;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;

/// Verify a Shopify webhook HMAC.
///
/// Shopify sends `X-Shopify-Hmac-Sha256: <base64>` where the value is
/// `base64(HMAC-SHA256(secret, raw_body))`.
///
/// Returns `true` if the signature matches, `false` otherwise (including
/// when the header value is malformed base64).
pub fn verify_shopify(secret: &[u8], body: &[u8], hmac_b64: &str) -> bool {
    verify_hmac_base64(secret, body, hmac_b64)
}

/// Verify a WooCommerce webhook signature.
///
/// WooCommerce sends `X-WC-Webhook-Signature: <base64>` using the same
/// HMAC-SHA256 + base64 scheme as Shopify.
pub fn verify_woocommerce(secret: &[u8], body: &[u8], sig_b64: &str) -> bool {
    verify_hmac_base64(secret, body, sig_b64)
}

fn verify_hmac_base64(secret: &[u8], body: &[u8], expected_b64: &str) -> bool {
    // Decode expected digest — malformed base64 means verification fails immediately.
    let expected = match base64::engine::general_purpose::STANDARD.decode(expected_b64) {
        Ok(bytes) => bytes,
        Err(_)    => return false,
    };

    // Compute HMAC-SHA256 of the body using the stored secret.
    let mut mac = match HmacSha256::new_from_slice(secret) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(body);
    let computed = mac.finalize().into_bytes();

    // Constant-time comparison — prevents timing attacks that could recover the secret.
    computed.as_slice().ct_eq(&expected).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    fn make_sig(secret: &[u8], body: &[u8]) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret).unwrap();
        mac.update(body);
        let result = mac.finalize().into_bytes();
        base64::engine::general_purpose::STANDARD.encode(result)
    }

    #[test]
    fn shopify_valid_signature_passes() {
        let secret = b"my_shopify_secret";
        let body   = b"{\"id\":12345,\"gateway\":\"cash_on_delivery\"}";
        let sig    = make_sig(secret, body);
        assert!(verify_shopify(secret, body, &sig));
    }

    #[test]
    fn shopify_wrong_secret_fails() {
        let body = b"{\"id\":12345}";
        let sig  = make_sig(b"correct_secret", body);
        assert!(!verify_shopify(b"wrong_secret", body, &sig));
    }

    #[test]
    fn shopify_tampered_body_fails() {
        let secret = b"my_secret";
        let sig    = make_sig(secret, b"original body");
        assert!(!verify_shopify(secret, b"tampered body", &sig));
    }

    #[test]
    fn malformed_base64_fails() {
        assert!(!verify_shopify(b"secret", b"body", "not-valid-base64!!!"));
    }

    #[test]
    fn woocommerce_valid_signature_passes() {
        let secret = b"woo_webhook_secret";
        let body   = b"{\"id\":99,\"payment_method\":\"cod\"}";
        let sig    = make_sig(secret, body);
        assert!(verify_woocommerce(secret, body, &sig));
    }

    #[test]
    fn woocommerce_wrong_secret_fails() {
        let body = b"{\"id\":99}";
        let sig  = make_sig(b"correct", body);
        assert!(!verify_woocommerce(b"wrong", body, &sig));
    }
}
