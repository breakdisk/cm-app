//! HTTP client for `services/payments`' mesh-internal payment-intent routes —
//! `POST /v1/internal/payments/intents[/:id/capture|/:id/void]`.
//!
//! Mirrors `services/order-intake/src/infrastructure/http/payments_client.rs`
//! in shape (30s timeout, `error_for_status_ref` + read the body on failure)
//! but is its own type rather than a shared crate: OmniDeliv always opens an
//! `action: "authorize"` intent (never `"sale"`, order-intake's only mode
//! today) and is the only caller in the platform that needs `capture`/`void`
//! at all.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::application::services::order_payments::{AuthorizedIntent, OrderPayments};

pub struct OmniPaymentsClient {
    base_url: String,
    http:     reqwest::Client,
}

impl OmniPaymentsClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("payments HTTP client");
        Self { base_url: base_url.into(), http }
    }
}

#[derive(Serialize)]
struct CreateIntentRequest {
    tenant_id:      Uuid,
    purpose:        &'static str,
    reference_type: &'static str,
    reference_id:   Uuid,
    amount_cents:   i64,
    currency:       String,
    return_url:     String,
    action:         &'static str,
}

#[derive(Deserialize)]
struct CreateIntentResponse {
    intent_id:    Uuid,
    checkout_url: String,
}

#[async_trait::async_trait]
impl OrderPayments for OmniPaymentsClient {
    async fn authorize(
        &self,
        tenant_id: Uuid,
        order_id: Uuid,
        amount_cents: i64,
        currency: &str,
        return_url: &str,
    ) -> anyhow::Result<AuthorizedIntent> {
        let url = format!("{}/v1/internal/payments/intents", self.base_url.trim_end_matches('/'));
        let resp = self.http
            .post(&url)
            .json(&CreateIntentRequest {
                tenant_id,
                // Lets `services/payments`' own `payment.intent.*` consumers
                // (and any future one on OmniDeliv's own topics) tell an
                // OmniDeliv order apart from an order-intake shipment sharing
                // the same Kafka topics.
                purpose: "omnideliv_order",
                reference_type: "order",
                reference_id: order_id,
                amount_cents,
                currency: currency.to_string(),
                return_url: return_url.to_string(),
                // Ring-fence only. `capture`/`void` below resolve it later —
                // see `OrderPayments`'s doc comment for when each fires.
                action: "authorize",
            })
            .send()
            .await?;

        if let Err(e) = resp.error_for_status_ref() {
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("payments authorize failed: {e} — body: {body_text}");
        }

        let resp = resp.json::<CreateIntentResponse>().await?;
        Ok(AuthorizedIntent { intent_id: resp.intent_id, checkout_url: resp.checkout_url })
    }

    async fn capture(&self, intent_id: Uuid, amount_cents: Option<i64>) -> anyhow::Result<()> {
        let url = format!(
            "{}/v1/internal/payments/intents/{intent_id}/capture",
            self.base_url.trim_end_matches('/'),
        );
        // A bodyless POST still means "take it all", so the delivery path's
        // request is byte-identical to what it sent before partial capture
        // existed.
        let req = self.http.post(&url);
        let req = match amount_cents {
            Some(n) => req.json(&serde_json::json!({ "amount_cents": n })),
            None => req,
        };
        let resp = req.send().await?;

        if let Err(e) = resp.error_for_status_ref() {
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("payments capture failed: {e} — body: {body_text}");
        }
        Ok(())
    }

    async fn void(&self, intent_id: Uuid) -> anyhow::Result<()> {
        let url = format!(
            "{}/v1/internal/payments/intents/{intent_id}/void",
            self.base_url.trim_end_matches('/'),
        );
        let resp = self.http.post(&url).send().await?;

        if let Err(e) = resp.error_for_status_ref() {
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("payments void failed: {e} — body: {body_text}");
        }
        Ok(())
    }
}
