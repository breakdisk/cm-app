//! `PaymentGateway` — the port every payment gateway adapter implements.
//! `services/payments` calls this; it never talks to a specific gateway's SDK
//! outside an `infrastructure/external/*` module.

use async_trait::async_trait;

/// Whether a hosted-checkout session takes the customer's money immediately
/// (`Sale`, the original and still-default behavior) or only places an
/// authorization hold pending a later, separate `PaymentGateway::capture`
/// (or `::void`) call (`Authorize`). This is OmniDeliv's prepaid-checkout
/// foundation: ring-fence funds when the order is placed, capture only once
/// a courier actually accepts it, void if none does — so the customer is
/// never charged for an order nobody fulfilled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentAction {
    Sale,
    Authorize,
}

impl Default for PaymentAction {
    /// Every caller that existed before this feature — and every caller
    /// that doesn't explicitly ask for `Authorize` — keeps getting the
    /// original immediate-capture behavior.
    fn default() -> Self {
        Self::Sale
    }
}

impl PaymentAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Sale      => "sale",
            Self::Authorize => "authorize",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "sale"      => Some(Self::Sale),
            "authorize" => Some(Self::Authorize),
            _           => None,
        }
    }
}

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
    /// `Sale` (immediate capture) or `Authorize` (hold only) — see
    /// `PaymentAction`.
    pub action: PaymentAction,
}

pub struct GatewaySession {
    pub checkout_url: String,
    pub gateway_order_ref: String,
}

/// The result of successfully verifying an inbound webhook payload.
#[derive(Debug)]
pub enum WebhookEvent {
    Captured { gateway_order_ref: String, gateway_payment_ref: String },
    /// NI reports the order's payment state as `AUTHORISED` — a hold was
    /// placed, but no money has moved yet. Was previously conflated with
    /// `Captured` (both mapped to the same variant) — that conflation is
    /// exactly what `PaymentAction::Authorize` needs *not* to have, or an
    /// authorization would be recorded as an actual capture. See
    /// `network_international.rs::parse_webhook_body`.
    Authorized { gateway_order_ref: String, gateway_payment_ref: String },
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

    /// Captures funds previously ring-fenced by an `Authorize` session.
    /// Returns the gateway's own capture reference (logged for
    /// observability — `PaymentIntent` has no column to persist it in,
    /// since `refund()` keys on the underlying payment reference, not the
    /// capture, and that reference is unchanged by capture — see
    /// `PaymentIntent::capture_authorized`).
    async fn capture(
        &self,
        gateway_order_ref: &str,
        gateway_payment_ref: &str,
        amount_cents: i64,
    ) -> anyhow::Result<String>;

    /// Releases an authorization hold that was never captured. See
    /// `network_international.rs`'s doc comment on its `void` implementation
    /// for an explicit callout: the wire-level endpoint for this specific
    /// operation (reversing an *authorization*, as opposed to voiding an
    /// already-made *capture*) is NOT confirmed against NI's docs — treat a
    /// failure here as requiring investigation, not routine retry noise.
    async fn void(&self, gateway_order_ref: &str, gateway_payment_ref: &str) -> anyhow::Result<()>;
}
