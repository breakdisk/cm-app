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
//!
//! ## Webhook payload shape and encryption (confirmed against NI's published docs)
//!
//! Two further mismatches, found by reading
//! <https://docs.ngenius-payments.com/reference/consuming-web-hooks> and
//! <https://docs.ngenius-payments.com/reference/webhook-encryption-decryption-guide>
//! rather than by running against a live sandbox:
//!
//! 1. **Shape.** NI sends a *nested* body (`order.reference`,
//!    `order._embedded.payment[0].state`, ...), not the flat
//!    `{status, orderReference, transactionReference}` shape originally
//!    assumed. The exact field carrying the payment/transaction reference
//!    isn't confirmed, so parsing is tolerant: deserialize to
//!    `serde_json::Value` and try several field paths with fallbacks (see
//!    `parse_webhook_body`), rather than a single rigid `struct`. The old
//!    flat shape is kept as a fallback at every extraction point so the
//!    pre-existing tests stay meaningful.
//! 2. **Encryption.** If the merchant sets an "Encryption Key" in NI's
//!    portal, the body is AES-256-CBC encrypted (PKCS7 padding, base64,
//!    16-byte IV prepended to the ciphertext) — decrypted here, after
//!    header/HMAC verification and before JSON parsing, when
//!    `NetworkInternationalConfig::webhook_encryption_key` is set.
//!
//! Every webhook logs its top-level JSON keys at INFO (keys only, never
//! values) so the first real call — sandbox or production — confirms the
//! true shape from our own logs instead of needing another round of
//! doc-reading.
//!
//! ## Authorize-then-capture, with void
//!
//! `create_session` can now place an `AUTH` hold instead of an immediate
//! `SALE` (`PaymentAction`), and this adapter gained `capture` (confirmed
//! against NI's docs) and `void` (**not** confirmed — see that method's own
//! doc comment). The `CAPTURED`/`AUTHORISED` webhook states used to be
//! conflated into one `WebhookEvent::Captured` outcome — correct only while
//! this integration was SALE-only. They are now parsed as two distinct
//! `WebhookEvent` variants; see `parse_webhook_body`.

use aes::Aes256;
use async_trait::async_trait;
use base64::Engine as _;
use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::config::NetworkInternationalConfig;
use crate::domain::repositories::payment_gateway::{
    CreateSessionRequest, GatewaySession, PaymentAction, PaymentGateway, WebhookEvent,
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
        // "AUTH" places a hold without taking the money; "SALE" (the
        // original, still-default behavior) captures immediately. Confirmed
        // against NI's docs — this is the same order-creation endpoint
        // `create_session` always used, just with a different `action`.
        let action = match req.action {
            PaymentAction::Sale => "SALE",
            PaymentAction::Authorize => "AUTH",
        };
        let body = CreateOrderRequest {
            action,
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

        // Decryption (if configured) happens after verification and before
        // parsing — verification covers the wire body exactly as NI sent
        // it (encrypted or not), and JSON parsing must never see ciphertext.
        let decrypted;
        let body_for_parsing: &[u8] = match self.cfg.webhook_encryption_key() {
            Some(key) => {
                decrypted = decrypt_webhook_body(key, raw_body)?;
                &decrypted
            }
            None => raw_body,
        };

        parse_webhook_body(body_for_parsing)
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

    /// Confirmed against NI's docs:
    /// `POST {base}/transactions/outlets/{outletRef}/orders/{orderRef}/payments/{paymentRef}/captures`.
    /// The capture response's own shape (what field carries the capture
    /// reference) is NOT confirmed against a live sandbox — tried the same
    /// way `extract_payment_reference` tries several field names for the
    /// webhook body, falling back to the payment reference itself (already
    /// known to be correct and stable) rather than failing the whole
    /// capture over a reference this caller doesn't strictly need: nothing
    /// in this codebase persists a separate "capture reference" column (see
    /// `PaymentGateway::capture`'s doc comment) — it's returned for logging only.
    async fn capture(
        &self,
        gateway_order_ref: &str,
        gateway_payment_ref: &str,
        amount_cents: i64,
    ) -> anyhow::Result<String> {
        let url = format!(
            "{}/transactions/outlets/{}/orders/{}/payments/{}/captures",
            self.cfg.base_url.trim_end_matches('/'),
            self.cfg.outlet_ref,
            gateway_order_ref,
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
            anyhow::bail!("NI capture failed: {e} — body: {body_text}");
        }
        let body_text = resp.text().await.unwrap_or_default();
        let capture_ref = serde_json::from_str::<Value>(&body_text)
            .ok()
            .and_then(|v| {
                v.get("reference")
                    .and_then(Value::as_str)
                    .or_else(|| v.get("_id").and_then(Value::as_str))
                    .map(str::to_string)
            })
            .unwrap_or_else(|| gateway_payment_ref.to_string());
        Ok(capture_ref)
    }

    /// Releases an authorization hold that was NEVER captured — the
    /// no-courier path in OmniDeliv's prepaid checkout.
    ///
    /// **This endpoint is unverified.** NI's docs confirm a "void a
    /// capture" operation:
    /// `DELETE {base}/transactions/outlets/{outletRef}/orders/{orderRef}/payments/{paymentRef}/captures/{captureRef}`
    /// — but that requires a `captureRef`, which only exists once a capture
    /// has actually happened. It cannot be the right call for releasing a
    /// hold that was never captured in the first place (there is no
    /// `captureRef` to put in the URL), and no separate "reverse an
    /// authorization" endpoint could be confirmed against NI's published
    /// docs during this pass.
    ///
    /// Best reading implemented here: `DELETE` against the *payment*
    /// resource itself (one path segment shorter than the confirmed
    /// void-a-capture URL — no `/captures/{captureRef}` suffix, since no
    /// capture exists), extrapolating from the confirmed pattern that
    /// `DELETE` on a resource in this API family means "cancel/release it".
    /// This is a guess, not a confirmed contract, and MUST be verified
    /// against a live NI sandbox (`scripts/ni-sandbox-verify.py` per the
    /// project's standing note on this integration) before this path is
    /// ever exercised with real money. If the endpoint is wrong, this call
    /// fails loudly (404 or similar, captured in the error body below) —
    /// it does not fail silently — and the caller
    /// (`PaymentIntentService::void_intent`) treats any failure here as
    /// requiring investigation, not routine background noise: an auth we
    /// failed to release is money still ring-fenced on a customer's card.
    /// Separately, per NI's docs, a void/cancel is only guaranteed to be
    /// possible on the SAME DAY as the original transaction — after that,
    /// only the issuing bank's own (unspecified) hold-expiry window
    /// releases it.
    async fn void(&self, gateway_order_ref: &str, gateway_payment_ref: &str) -> anyhow::Result<()> {
        let url = format!(
            "{}/transactions/outlets/{}/orders/{}/payments/{}",
            self.cfg.base_url.trim_end_matches('/'),
            self.cfg.outlet_ref,
            gateway_order_ref,
            gateway_payment_ref,
        );
        let resp = self.http.delete(&url).bearer_auth(&self.cfg.api_key).send().await?;
        if let Err(e) = resp.error_for_status_ref() {
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "NI void (authorization reversal — UNVERIFIED endpoint, see \
                 NetworkInternationalGateway::void's doc comment) failed: {e} — body: {body_text}"
            );
        }
        Ok(())
    }
}

type Aes256CbcDec = cbc::Decryptor<Aes256>;

/// Decrypts a webhook body per NI's webhook-encryption-decryption-guide:
/// the wire body is base64, decoding to a 16-byte IV prepended to
/// AES-256-CBC ciphertext (PKCS5/PKCS7 padding). `key` must already be
/// validated as exactly 32 bytes — `Config::load` enforces that at boot
/// (`NetworkInternationalConfig::webhook_encryption_key`), so a length
/// mismatch here would mean that guard was bypassed (e.g. constructed
/// directly in a test), not a normal runtime condition.
///
/// Returns `Err` — never silently falls through to treating the ciphertext
/// as plaintext JSON — if the body isn't valid base64, is too short to
/// contain an IV, or fails to decrypt (wrong key, or the body was not
/// actually encrypted despite a key being configured).
fn decrypt_webhook_body(key: &str, raw_body: &[u8]) -> anyhow::Result<Vec<u8>> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(raw_body)
        .map_err(|_| {
            anyhow::anyhow!(
                "webhook body encryption is configured (WEBHOOK_ENCRYPTION_KEY) but the \
                 body is not valid base64"
            )
        })?;

    if decoded.len() < 16 {
        anyhow::bail!("webhook body is too short to contain the 16-byte IV NI prepends");
    }
    let (iv, ciphertext) = decoded.split_at(16);

    let cipher = Aes256CbcDec::new_from_slices(key.as_bytes(), iv)
        .map_err(|e| anyhow::anyhow!("invalid AES-256 key/IV while decrypting webhook body: {e}"))?;

    cipher.decrypt_padded_vec_mut::<Pkcs7>(ciphertext).map_err(|_| {
        anyhow::anyhow!(
            "webhook body encryption is configured (WEBHOOK_ENCRYPTION_KEY) but decryption \
             failed — wrong key, or the body was not actually encrypted"
        )
    })
}

/// Parses a (decrypted, if applicable) webhook body into a `WebhookEvent`.
///
/// NI's real payload is nested (`order.reference`,
/// `order._embedded.payment[0].state`, ...) per
/// <https://docs.ngenius-payments.com/reference/consuming-web-hooks>, not
/// the flat `{status, orderReference, transactionReference}` shape
/// originally assumed. The exact field carrying the payment/transaction
/// reference isn't confirmed against a live sandbox, so this parses
/// tolerantly — `serde_json::Value` plus fallback field paths — instead of
/// a single rigid `struct` that would fail outright on a shape mismatch.
/// The old flat shape is kept as the last fallback at every extraction
/// point, so the pre-existing tests (and any NI account still on that
/// shape) keep working.
fn parse_webhook_body(body: &[u8]) -> anyhow::Result<WebhookEvent> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|e| anyhow::anyhow!("failed to parse webhook body as JSON: {e}"))?;

    // Keys only, never values — the body carries payment data.
    if let Some(keys) = value.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()) {
        tracing::info!(?keys, "verify_webhook: received payload top-level keys");
    }

    let order_reference = extract_order_reference(&value).ok_or_else(|| {
        anyhow::anyhow!(
            "webhook payload has no resolvable order reference (checked order.reference, \
             top-level orderReference, top-level reference)"
        )
    })?;

    let state = extract_state(&value).unwrap_or_default();
    // Previously `matches!(state, "CAPTURED" | "AUTHORISED") -> WebhookEvent::Captured`
    // for BOTH states — correct back when this integration was SALE-only
    // (`AUTHORISED` was, at the time, just an intermediate state NI's docs
    // mention en route to `CAPTURED`, treated as "close enough to captured").
    // Now that `PaymentAction::Authorize` is a real, intentional outcome
    // (NI genuinely stops at `AUTHORISED` until a separate `capture` call is
    // made), that conflation is wrong: recording an authorization-only hold
    // as `Captured` would tell the rest of the system money was taken when
    // it was only ring-fenced — the opposite of what `authorize`/`capture`/
    // `void` exist to make possible. `CAPTURED` and `AUTHORISED` are now
    // kept as two distinct outcomes.
    let is_captured = state.eq_ignore_ascii_case("CAPTURED");
    let is_authorized = state.eq_ignore_ascii_case("AUTHORISED");

    if !is_captured && !is_authorized {
        return Ok(WebhookEvent::Failed { gateway_order_ref: order_reference });
    }

    // A resolvable payment/transaction reference is not optional for a
    // Captured OR Authorized event: it's what `refund`/`capture`/`void`
    // (above) key on, so recording either outcome for a payment we could
    // never look back up to act on would be strictly worse than rejecting
    // the webhook and letting NI retry (or ops investigate) once the true
    // field name is confirmed.
    let payment_reference = extract_payment_reference(&value).ok_or_else(|| {
        anyhow::anyhow!(
            "webhook payload state {state:?} looks like a successful capture or \
             authorization but no transaction/payment reference could be resolved (checked \
             order._embedded.payment[0].reference, ...[0]._id, top-level \
             transactionReference) — refusing to record a payment that could never be \
             captured, voided, or refunded"
        )
    })?;

    if is_captured {
        Ok(WebhookEvent::Captured { gateway_order_ref: order_reference, gateway_payment_ref: payment_reference })
    } else {
        Ok(WebhookEvent::Authorized { gateway_order_ref: order_reference, gateway_payment_ref: payment_reference })
    }
}

/// First element of `order._embedded.payment[]`, if present.
fn first_payment(value: &Value) -> Option<&Value> {
    value.get("order")?.get("_embedded")?.get("payment")?.as_array()?.first()
}

fn extract_order_reference(value: &Value) -> Option<String> {
    value
        .get("order")
        .and_then(|o| o.get("reference"))
        .and_then(Value::as_str)
        .or_else(|| value.get("orderReference").and_then(Value::as_str))
        .or_else(|| value.get("reference").and_then(Value::as_str))
        .map(str::to_string)
}

fn extract_payment_reference(value: &Value) -> Option<String> {
    first_payment(value)
        .and_then(|p| p.get("reference").and_then(Value::as_str).or_else(|| p.get("_id").and_then(Value::as_str)))
        .or_else(|| value.get("transactionReference").and_then(Value::as_str))
        .map(str::to_string)
}

fn extract_state(value: &Value) -> Option<String> {
    first_payment(value)
        .and_then(|p| p.get("state"))
        .and_then(Value::as_str)
        .or_else(|| value.get("status").and_then(Value::as_str))
        .map(str::to_string)
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
            webhook_encryption_key: None,
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
            webhook_encryption_key: None,
        })
    }

    /// HMAC-signature verification mode + AES-256-CBC body encryption, both
    /// configured — exercises the decrypt-after-verify path in
    /// `verify_webhook`. Key is NI's own 32-character example from
    /// webhook-encryption-decryption-guide.
    const TEST_ENCRYPTION_KEY: &str = "f9K@82nNc%P!r4QwLxTzA#10UvM&b6Xe";

    fn test_gateway_with_encryption() -> NetworkInternationalGateway {
        NetworkInternationalGateway::new(NetworkInternationalConfig {
            base_url: "https://example.invalid".into(),
            api_key: "test-key".into(),
            webhook_secret: Some("test-secret".into()),
            outlet_ref: "outlet-1".into(),
            webhook_header_key: None,
            webhook_header_value: None,
            webhook_encryption_key: Some(TEST_ENCRYPTION_KEY.into()),
        })
    }

    fn sign(secret: &str, body: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes())
    }

    /// Encrypts `plaintext` exactly as NI's webhook-encryption-decryption-guide
    /// describes: AES-256-CBC (PKCS7 padding), 16-byte IV prepended to the
    /// ciphertext, whole thing base64-encoded — the inverse of
    /// `decrypt_webhook_body`. `iv` is caller-supplied (not random) so tests
    /// are deterministic.
    fn encrypt_for_test(key: &str, iv: [u8; 16], plaintext: &[u8]) -> Vec<u8> {
        use cbc::cipher::BlockEncryptMut;
        type Aes256CbcEnc = cbc::Encryptor<Aes256>;

        let ciphertext = Aes256CbcEnc::new_from_slices(key.as_bytes(), &iv)
            .unwrap()
            .encrypt_padded_vec_mut::<Pkcs7>(plaintext);

        let mut framed = iv.to_vec();
        framed.extend_from_slice(&ciphertext);
        base64::engine::general_purpose::STANDARD.encode(framed).into_bytes()
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

    // --- Payload shape (mismatch A) ---------------------------------------
    //
    // "Old flat shape still parses" (back-compat) is covered by the four
    // pre-existing tests above, unchanged: `extract_order_reference` /
    // `extract_payment_reference` / `extract_state` all fall back to the
    // old flat field names (`orderReference`, `transactionReference`,
    // `status`) when the nested `order.*` shape isn't present, so those
    // tests still pass exactly as written.

    /// NI's actual documented nested shape (consuming-web-hooks):
    /// `order.reference`, `order._embedded.payment[0].state`/`.reference`.
    /// No flat fields present at all.
    #[test]
    fn verify_webhook_parses_ni_s_real_nested_payload_shape() {
        let gateway = test_gateway();
        let body = br#"{
            "eventId": "evt-1",
            "eventName": "transaction.captured",
            "outletId": "outlet-1",
            "order": {
                "action": "SALE",
                "reference": "ord-nested-1",
                "amount": { "currencyCode": "AED", "value": 2200 },
                "_embedded": {
                    "payment": [
                        { "state": "CAPTURED", "reference": "pay-nested-1" }
                    ]
                }
            }
        }"#;
        let sig = sign("test-secret", body);
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-ni-signature", sig.parse().unwrap());

        let event = gateway.verify_webhook(&headers, body).expect("must verify and parse");
        match event {
            WebhookEvent::Captured { gateway_order_ref, gateway_payment_ref } => {
                assert_eq!(gateway_order_ref, "ord-nested-1");
                assert_eq!(gateway_payment_ref, "pay-nested-1");
            }
            _ => panic!("expected Captured"),
        }
    }

    /// Same nested shape, but the payment reference is only present as
    /// `_id` — the other field name the docs suggest is plausible.
    #[test]
    fn verify_webhook_parses_nested_payload_payment_id_fallback() {
        let gateway = test_gateway();
        let body = br#"{
            "order": {
                "reference": "ord-nested-2",
                "_embedded": { "payment": [ { "state": "CAPTURED", "_id": "pay-nested-2" } ] }
            }
        }"#;
        let sig = sign("test-secret", body);
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-ni-signature", sig.parse().unwrap());

        let event = gateway.verify_webhook(&headers, body).expect("must verify and parse");
        match event {
            WebhookEvent::Captured { gateway_order_ref, gateway_payment_ref } => {
                assert_eq!(gateway_order_ref, "ord-nested-2");
                assert_eq!(gateway_payment_ref, "pay-nested-2");
            }
            _ => panic!("expected Captured"),
        }
    }

    // --- CAPTURED vs AUTHORISED (highest-risk fix: the old code conflated
    // these into the same WebhookEvent::Captured variant) ------------------

    /// THE regression test for the split: `AUTHORISED` must map to its own
    /// distinct `WebhookEvent::Authorized`, never to `Captured`. Before this
    /// fix, an `Authorize`-action order that only ever reached `AUTHORISED`
    /// (funds merely ring-fenced, never taken) would have been recorded as
    /// a full capture — the exact opposite of what `PaymentAction::Authorize`
    /// exists to make possible.
    #[test]
    fn verify_webhook_maps_authorised_state_to_a_distinct_authorized_event_not_captured() {
        let gateway = test_gateway();
        let body = br#"{
            "order": {
                "reference": "ord-auth-1",
                "_embedded": { "payment": [ { "state": "AUTHORISED", "reference": "pay-auth-1" } ] }
            }
        }"#;
        let sig = sign("test-secret", body);
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-ni-signature", sig.parse().unwrap());

        let event = gateway.verify_webhook(&headers, body).expect("must verify and parse");
        match event {
            WebhookEvent::Authorized { gateway_order_ref, gateway_payment_ref } => {
                assert_eq!(gateway_order_ref, "ord-auth-1");
                assert_eq!(gateway_payment_ref, "pay-auth-1");
            }
            other => panic!("expected Authorized, not {other:?} — AUTHORISED must never be recorded as a capture"),
        }
    }

    /// The flat (pre-nested-shape) `AUTHORISED` payload must resolve the
    /// same way — the split applies regardless of which payload shape.
    #[test]
    fn verify_webhook_maps_flat_authorised_status_to_authorized() {
        let gateway = test_gateway();
        let body = br#"{"status":"AUTHORISED","orderReference":"ord-auth-2","transactionReference":"txn-auth-2"}"#;
        let sig = sign("test-secret", body);
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-ni-signature", sig.parse().unwrap());

        match gateway.verify_webhook(&headers, body).expect("must verify") {
            WebhookEvent::Authorized { gateway_order_ref, gateway_payment_ref } => {
                assert_eq!(gateway_order_ref, "ord-auth-2");
                assert_eq!(gateway_payment_ref, "txn-auth-2");
            }
            other => panic!("expected Authorized, got {other:?}"),
        }
    }

    /// A nested payload whose payment state is a decline/unknown value ->
    /// Failed, never Captured — and this must succeed (not error) even
    /// though no payment reference is present at all, since
    /// `WebhookEvent::Failed` doesn't carry one.
    #[test]
    fn verify_webhook_maps_nested_declined_state_to_failed() {
        let gateway = test_gateway();
        let body = br#"{
            "order": {
                "reference": "ord-nested-3",
                "_embedded": { "payment": [ { "state": "DECLINED" } ] }
            }
        }"#;
        let sig = sign("test-secret", body);
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-ni-signature", sig.parse().unwrap());

        match gateway.verify_webhook(&headers, body).expect("must verify and parse") {
            WebhookEvent::Failed { gateway_order_ref } => assert_eq!(gateway_order_ref, "ord-nested-3"),
            _ => panic!("expected Failed"),
        }
    }

    /// A nested payload that looks captured but genuinely has no resolvable
    /// payment reference anywhere -- must be rejected (`Err`), never
    /// silently recorded as a Captured payment with a fabricated or empty
    /// reference that `refund` could never look up later.
    #[test]
    fn verify_webhook_rejects_a_captured_payload_with_no_resolvable_payment_reference() {
        let gateway = test_gateway();
        let body = br#"{
            "order": {
                "reference": "ord-nested-4",
                "_embedded": { "payment": [ { "state": "CAPTURED" } ] }
            }
        }"#;
        let sig = sign("test-secret", body);
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-ni-signature", sig.parse().unwrap());

        let err = gateway.verify_webhook(&headers, body).expect_err(
            "a Captured-looking event with no resolvable payment reference must be rejected",
        );
        assert!(
            err.to_string().contains("payment reference"),
            "error should explain the missing reference, got: {err}"
        );
    }

    // --- Body encryption (mismatch B) ---------------------------------------

    /// Encrypt a known JSON body with AES-256-CBC + prepended IV + base64
    /// (exactly as NI's webhook-encryption-decryption-guide describes),
    /// HMAC-sign the resulting *still-encrypted* wire body the way NI
    /// would, and feed it through `verify_webhook` with the encryption key
    /// configured: verification must pass over the encrypted bytes,
    /// decryption must recover the plaintext, and parsing must yield the
    /// right event.
    #[test]
    fn verify_webhook_decrypts_an_encrypted_body_before_parsing() {
        let gateway = test_gateway_with_encryption();
        let plaintext =
            br#"{"status":"CAPTURED","orderReference":"ord-enc-1","transactionReference":"txn-enc-1"}"#;
        let iv = [7u8; 16];
        let wire_body = encrypt_for_test(TEST_ENCRYPTION_KEY, iv, plaintext);

        // HMAC covers the wire body exactly as NI would send it -- the
        // still-encrypted bytes -- never the plaintext.
        let sig = sign("test-secret", &wire_body);
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-ni-signature", sig.parse().unwrap());

        let event = gateway
            .verify_webhook(&headers, &wire_body)
            .expect("must verify the encrypted body, decrypt it, and parse the result");
        match event {
            WebhookEvent::Captured { gateway_order_ref, gateway_payment_ref } => {
                assert_eq!(gateway_order_ref, "ord-enc-1");
                assert_eq!(gateway_payment_ref, "txn-enc-1");
            }
            _ => panic!("expected Captured"),
        }
    }

    /// Encryption key configured but the body is plaintext JSON (not
    /// actually encrypted) -> rejected with a clear error. Must never fall
    /// through to parsing the "ciphertext" as JSON.
    #[test]
    fn verify_webhook_rejects_a_plaintext_body_when_encryption_is_configured() {
        let gateway = test_gateway_with_encryption();
        let body = br#"{"status":"CAPTURED","orderReference":"ord-1","transactionReference":"txn-1"}"#;
        let sig = sign("test-secret", body);
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-ni-signature", sig.parse().unwrap());

        let err = gateway.verify_webhook(&headers, body).expect_err(
            "a plaintext body must be rejected, not silently accepted, when an encryption \
             key is configured",
        );
        assert!(
            err.to_string().contains("base64"),
            "error should explain the decode/decryption failure, got: {err}"
        );
    }
}
