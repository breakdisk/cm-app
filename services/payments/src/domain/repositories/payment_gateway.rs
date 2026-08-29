//! `PaymentGateway` — the port every payment gateway adapter implements.
//! `services/payments` calls this; it never talks to a specific gateway's SDK
//! outside an `infrastructure/external/*` module.

use async_trait::async_trait;

pub struct CreateSessionRequest<'a> {
    pub amount_cents: i64,
    pub currency: &'a str,
    /// Our own `payment_intents.id` — passed through as the gateway's
    /// merchant-supplied order reference so the webhook can be matched back
    /// to a row without a database round trip keyed on anything gateway-issued.
    pub intent_id: uuid::Uuid,
    /// Where the gateway's hosted page redirects the customer's browser/WebView
    /// after payment. This is a UX signal only — never trusted as proof of payment.
    pub return_url: &'a str,
}

pub struct GatewaySession {
    pub checkout_url: String,
    pub gateway_order_ref: String,
}

/// The result of successfully verifying an inbound webhook payload.
#[derive(Debug)]
pub enum WebhookEvent {
    Captured { gateway_order_ref: String, gateway_payment_ref: String },
    Failed { gateway_order_ref: String },
}

#[async_trait]
pub trait PaymentGateway: Send + Sync {
    async fn create_session(&self, req: CreateSessionRequest<'_>) -> anyhow::Result<GatewaySession>;

    /// Verifies the webhook's authenticity (signature check) and parses it.
    /// Returns `Err` for a payload that fails signature verification — the
    /// caller must never act on an unverified webhook.
    fn verify_webhook(&self, headers: &reqwest::header::HeaderMap, raw_body: &[u8]) -> anyhow::Result<WebhookEvent>;

    async fn refund(&self, gateway_payment_ref: &str, amount_cents: i64) -> anyhow::Result<()>;
}
