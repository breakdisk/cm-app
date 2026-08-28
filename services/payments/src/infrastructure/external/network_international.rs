//! Network International (NI) hosted-checkout adapter.
//!
//! LogisticOS never collects card data directly — the customer is redirected
//! to NI's own hosted payment page. This keeps `services/payments` at PCI
//! SAQ-A instead of pulling it into full PCI scope.
//!
//! NOTE: the exact request/response JSON shapes below follow NI's (N-Genius)
//! publicly documented hosted-order pattern as of this writing. Confirm
//! field names against NI's live API/sandbox docs during integration testing
//! before going live — this is a deliberate, stated boundary: it fixes the
//! contract our own services expose, not NI's wire format, which needs
//! verification against a real sandbox this plan cannot obtain.
//!
//! ## Webhook verification has two independent modes
//!
//! `verify_webhook` was originally written against a single assumed model:
//! NI signs the raw body with a shared secret and sends the signature as
//! `x-ni-signature: base64(HMAC-SHA256(webhook_secret, raw_body))`. But NI's
//! own webhook configuration UI actually asks the merchant for a "Header
//! Key", a "Header Value", and an "Encryption Key" — and N-Genius's docs
//! describe the Header Key/Value pair as *"identifiers generally
//! corresponding to an account name or number, and the account password,
//! API Key or similar"*, i.e. a static header NI echoes back on every call,
//! not a body signature. If NI only ever sends that static header, the
//! HMAC-only implementation rejected every single webhook while customers
//! were still being charged.
//!
//! Since it isn't yet known which mode a given NI portal configuration uses
//! (and the separate "Encryption Key" field suggests HMAC signing may also
//! be a real option), `verify_webhook` supports both, selected by which
//! fields are present on `NetworkInternationalConfig`:
//!   - `webhook_header_key` + `webhook_header_value` set -> static-header
//!     mode (constant-time compare of the header value).
//!   - `webhook_secret` set -> the original HMAC-SHA256 mode, unchanged.
//!   - Neither set is refused at boot by `Config::load` (see
//!     `NetworkInternationalConfig::has_webhook_verification_mode`) rather
//!     than silently accepting unverified webhooks here.

use async_trait::async_trait;
use base64::Engine as _;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::config::NetworkInternationalConfig;
use crate::domain::repositories::payment_gateway::{
    CreateSessionRequest, GatewaySession, PaymentGateway, WebhookEvent,
};

type HmacSha256 = Hmac<Sha256>;

pub struct NetworkInternationalGateway {
    cfg: NetworkInternationalConfig,
    http: reqwest::Client,
}

impl NetworkInternationalGateway {
    pub fn new(cfg: NetworkInternationalConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("NI HTTP client");
        Self { cfg, http }
    }
}

#[derive(Serialize)]
struct CreateOrderRequest<'a> {
    action: &'a str,
    amount: OrderAmount<'a>,
    merchant_order_reference: String,
    merchant_attributes: MerchantAttributes<'a>,
}

#[derive(Serialize)]
struct OrderAmount<'a> {
    #[serde(rename = "currencyCode")]
    currency_code: &'a str,
    value: i64,
}

#[derive(Serialize)]
struct MerchantAttributes<'a> {
    #[serde(rename = "redirectUrl")]
    redirect_url: &'a str,
}

#[derive(Deserialize)]
struct CreateOrderResponse {
    reference: String,
    #[serde(rename = "_links")]
    links: OrderLinks,
}

#[derive(Deserialize)]
struct OrderLinks {
    payment: LinkHref,
}

#[derive(Deserialize)]
struct LinkHref {
    href: String,
}

#[async_trait]
impl PaymentGateway for NetworkInternationalGateway {
    async fn create_session(&self, req: CreateSessionRequest<'_>) -> anyhow::Result<GatewaySession> {
        let url = format!(
            "{}/transactions/outlets/{}/orders",
            self.cfg.base_url.trim_end_matches('/'),
            self.cfg.outlet_ref,
        );
        let body = CreateOrderRequest {
            action: "SALE",
            amount: OrderAmount { currency_code: req.currency, value: req.amount_cents },
            merchant_order_reference: req.intent_id.to_string(),
            merchant_attributes: MerchantAttributes { redirect_url: req.return_url },
        };
        let resp = self.http.post(&url).bearer_auth(&self.cfg.api_key).json(&body).send().await?;
        if let Err(e) = resp.error_for_status_ref() {
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("NI create_session failed: {e} — body: {body_text}");
        }
        let resp = resp.json::<CreateOrderResponse>().await?;

        Ok(GatewaySession {
            checkout_url: resp.links.payment.href,
            gateway_order_ref: resp.reference,
        })
    }

    fn verify_webhook(&self, headers: &reqwest::header::HeaderMap, raw_body: &[u8]) -> anyhow::Result<WebhookEvent> {
        if let Some((header_key, expected_value)) = self.cfg.webhook_header_pair() {
            // Static-header mode. `HeaderMap::get` takes the key through
            // `http::HeaderName` parsing, which case-folds to lowercase
            // before lookup/storage (confirmed by reading
            // `http::header::name::HEADER_CHARS` — every ASCII letter, upper
            // or lower, maps to its lowercase byte), so this matches
            // regardless of the case NI sends the header name in, or the
            // case configured here.
            let actual_value = headers
                .get(header_key)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| anyhow::anyhow!("missing webhook header {header_key:?}"))?;

            if actual_value
                .as_bytes()
                .ct_eq(expected_value.as_bytes())
                .unwrap_u8()
                != 1
            {
                anyhow::bail!("webhook header value mismatch for {header_key:?}");
            }

            tracing::info!(
                header = %header_key,
                "verify_webhook: verified via static-header mode (NETWORK_INTERNATIONAL__WEBHOOK_HEADER_KEY/VALUE)",
            );
        } else if let Some(webhook_secret) = self.cfg.webhook_secret() {
            let signature_b64 = headers
                .get("x-ni-signature")
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| anyhow::anyhow!("missing webhook signature header"))?;

            let expected = base64::engine::general_purpose::STANDARD
                .decode(signature_b64)
                .map_err(|_| anyhow::anyhow!("malformed webhook signature"))?;

            let mut mac = HmacSha256::new_from_slice(webhook_secret.as_bytes())
                .map_err(|_| anyhow::anyhow!("invalid webhook secret length"))?;
            mac.update(raw_body);
            let computed = mac.finalize().into_bytes();

            if computed.as_slice().ct_eq(&expected).unwrap_u8() != 1 {
                anyhow::bail!("webhook signature verification failed");
            }

            tracing::info!(
                "verify_webhook: verified via HMAC-signature mode (NETWORK_INTERNATIONAL__WEBHOOK_SECRET, x-ni-signature header)",
            );
        } else {
            // Unreachable in a real deployment: `Config::load` refuses to
            // boot a `network_international` config with neither mode set
            // (`NetworkInternationalConfig::has_webhook_verification_mode`).
            // Kept as an explicit error rather than falling through to
            // accepting the webhook unverified, in case this type is ever
            // constructed another way (e.g. directly in a test) that
            // bypasses that startup check.
            anyhow::bail!(
                "Network International webhook verification is not configured (neither \
                 webhook_header_key/webhook_header_value nor webhook_secret is set)"
            );
        }

        let payload: WebhookPayload = serde_json::from_slice(raw_body)?;
        Ok(match payload.status.as_str() {
            "CAPTURED" | "AUTHORISED" => WebhookEvent::Captured {
                gateway_order_ref: payload.order_reference,
                gateway_payment_ref: payload.transaction_reference,
            },
            _ => WebhookEvent::Failed { gateway_order_ref: payload.order_reference },
        })
    }

    async fn refund(&self, gateway_payment_ref: &str, amount_cents: i64) -> anyhow::Result<()> {
        let url = format!(
            "{}/transactions/{}/refund",
            self.cfg.base_url.trim_end_matches('/'),
            gateway_payment_ref,
        );
        let resp = self.http
            .post(&url)
            .bearer_auth(&self.cfg.api_key)
            .json(&serde_json::json!({ "amount": { "value": amount_cents } }))
            .send()
            .await?;
        if let Err(e) = resp.error_for_status_ref() {
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("NI refund failed: {e} — body: {body_text}");
        }
        Ok(())
    }
}

#[derive(Deserialize)]
struct WebhookPayload {
    status: String,
    #[serde(rename = "orderReference")]
    order_reference: String,
    #[serde(rename = "transactionReference")]
    transaction_reference: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// HMAC-signature verification mode — matches the 4 pre-existing tests
    /// below unchanged.
    fn test_gateway() -> NetworkInternationalGateway {
        NetworkInternationalGateway::new(NetworkInternationalConfig {
            base_url: "https://example.invalid".into(),
            api_key: "test-key".into(),
            webhook_secret: Some("test-secret".into()),
            outlet_ref: "outlet-1".into(),
            webhook_header_key: None,
            webhook_header_value: None,
        })
    }

    /// Static-header verification mode. Deliberately also carries a
    /// `webhook_secret` — the same value `sign()` below would produce a
    /// correct HMAC for — so header-mode tests can prove header mode does
    /// not silently fall through to a correct HMAC when the static header
    /// is absent or wrong (header mode is checked first and is exclusive
    /// once both header key/value are configured).
    fn test_gateway_header_mode() -> NetworkInternationalGateway {
        NetworkInternationalGateway::new(NetworkInternationalConfig {
            base_url: "https://example.invalid".into(),
            api_key: "test-key".into(),
            webhook_secret: Some("test-secret".into()),
            outlet_ref: "outlet-1".into(),
            webhook_header_key: Some("X-Merchant-Secret".into()),
            webhook_header_value: Some("correct-horse-battery-staple".into()),
        })
    }

    fn sign(secret: &str, body: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes())
    }

    #[test]
    fn verify_webhook_accepts_a_correctly_signed_captured_payload() {
        let gateway = test_gateway();
        let body = br#"{"status":"CAPTURED","orderReference":"ord-1","transactionReference":"txn-1"}"#;
        let sig = sign("test-secret", body);
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-ni-signature", sig.parse().unwrap());

        let event = gateway.verify_webhook(&headers, body).expect("must verify");
        match event {
            WebhookEvent::Captured { gateway_order_ref, gateway_payment_ref } => {
                assert_eq!(gateway_order_ref, "ord-1");
                assert_eq!(gateway_payment_ref, "txn-1");
            }
            _ => panic!("expected Captured"),
        }
    }

    #[test]
    fn verify_webhook_rejects_a_tampered_body() {
        let gateway = test_gateway();
        let signed_body = br#"{"status":"CAPTURED","orderReference":"ord-1","transactionReference":"txn-1"}"#;
        let sig = sign("test-secret", signed_body);
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-ni-signature", sig.parse().unwrap());

        let tampered = br#"{"status":"CAPTURED","orderReference":"ord-1","transactionReference":"txn-EVIL"}"#;
        assert!(gateway.verify_webhook(&headers, tampered).is_err());
    }

    #[test]
    fn verify_webhook_rejects_a_missing_signature_header() {
        let gateway = test_gateway();
        let headers = reqwest::header::HeaderMap::new();
        let body = br#"{"status":"CAPTURED","orderReference":"ord-1","transactionReference":"txn-1"}"#;
        assert!(gateway.verify_webhook(&headers, body).is_err());
    }

    #[test]
    fn verify_webhook_maps_a_non_captured_status_to_failed() {
        let gateway = test_gateway();
        let body = br#"{"status":"DECLINED","orderReference":"ord-2","transactionReference":"txn-2"}"#;
        let sig = sign("test-secret", body);
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-ni-signature", sig.parse().unwrap());

        match gateway.verify_webhook(&headers, body).expect("must verify") {
            WebhookEvent::Failed { gateway_order_ref } => assert_eq!(gateway_order_ref, "ord-2"),
            _ => panic!("expected Failed"),
        }
    }

    #[test]
    fn verify_webhook_header_mode_accepts_the_configured_header_value() {
        let gateway = test_gateway_header_mode();
        let body = br#"{"status":"CAPTURED","orderReference":"ord-1","transactionReference":"txn-1"}"#;
        let mut headers = reqwest::header::HeaderMap::new();
        // Sent in a different case than configured ("X-Merchant-Secret") to
        // also exercise header-name case-insensitivity.
        headers.insert("x-merchant-secret", "correct-horse-battery-staple".parse().unwrap());

        let event = gateway.verify_webhook(&headers, body).expect("must verify");
        match event {
            WebhookEvent::Captured { gateway_order_ref, gateway_payment_ref } => {
                assert_eq!(gateway_order_ref, "ord-1");
                assert_eq!(gateway_payment_ref, "txn-1");
            }
            _ => panic!("expected Captured"),
        }
    }

    #[test]
    fn verify_webhook_header_mode_rejects_the_wrong_header_value() {
        let gateway = test_gateway_header_mode();
        let body = br#"{"status":"CAPTURED","orderReference":"ord-1","transactionReference":"txn-1"}"#;
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-merchant-secret", "wrong-value".parse().unwrap());

        assert!(gateway.verify_webhook(&headers, body).is_err());
    }

    #[test]
    fn verify_webhook_header_mode_rejects_a_missing_header() {
        let gateway = test_gateway_header_mode();
        let body = br#"{"status":"CAPTURED","orderReference":"ord-1","transactionReference":"txn-1"}"#;
        let headers = reqwest::header::HeaderMap::new();

        assert!(gateway.verify_webhook(&headers, body).is_err());
    }

    /// Proves the two modes don't silently fall through to each other: a
    /// gateway configured for header mode must reject a request carrying a
    /// *correct* HMAC signature but no static header at all, rather than
    /// falling back to accepting the HMAC as an alternative proof.
    #[test]
    fn verify_webhook_header_mode_rejects_a_correct_hmac_signature_with_no_static_header() {
        let gateway = test_gateway_header_mode();
        let body = br#"{"status":"CAPTURED","orderReference":"ord-1","transactionReference":"txn-1"}"#;
        // Correctly signed against the same "test-secret" the HMAC path
        // would use — but this gateway is in header mode, so x-ni-signature
        // must be irrelevant.
        let sig = sign("test-secret", body);
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-ni-signature", sig.parse().unwrap());

        assert!(
            gateway.verify_webhook(&headers, body).is_err(),
            "a correct HMAC signature must not verify a header-mode-configured gateway \
             when the static header is absent"
        );
    }
}
