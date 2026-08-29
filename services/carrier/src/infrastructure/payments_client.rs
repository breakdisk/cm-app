//! HTTP client for `services/payments`' mesh-internal payment-intent routes —
//! `POST /v1/internal/payments/intents[/:id/capture|/:id/void]`.
//!
//! Mirrors `services/omnideliv/src/infrastructure/external/payments_client.rs`
//! in shape (30s timeout, `error_for_status_ref` then read the body on failure)
//! rather than sharing a crate with it: the two differ in `purpose`, and a
//! shared client would need a knob for the one field whose whole job is telling
//! the two apart on a shared Kafka topic.
//!
//! A marketplace booking always opens an `action: "authorize"` intent, never a
//! `"sale"`. A booking sits `Pending` until a carrier answers, so a sale at
//! request time would charge a merchant for a truck that gets rejected.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::application::services::{AuthorizedIntent, BookingPayments};

pub struct CarrierPaymentsClient {
    base_url: String,
    http:     reqwest::Client,
}

impl CarrierPaymentsClient {
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
impl BookingPayments for CarrierPaymentsClient {
    async fn authorize(
        &self,
        tenant_id: Uuid,
        booking_id: Uuid,
        amount_cents: i64,
        currency: &str,
        return_url: &str,
    ) -> anyhow::Result<AuthorizedIntent> {
        let url = format!("{}/v1/internal/payments/intents", self.base_url.trim_end_matches('/'));
        let resp = self.http
            .post(&url)
            .json(&CreateIntentRequest {
                tenant_id,
                // What tells a marketplace booking apart from an OmniDeliv
                // order and an order-intake shipping fee, all three of which
                // share `payment.intent.*`. Every consumer filters on it.
                purpose: super::booking_payment_consumer::MARKETPLACE_BOOKING_PURPOSE,
                reference_type: "marketplace_booking",
                reference_id: booking_id,
                amount_cents,
                currency: currency.to_string(),
                return_url: return_url.to_string(),
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

    async fn capture(&self, intent_id: Uuid) -> anyhow::Result<()> {
        self.post_no_body(&format!("intents/{intent_id}/capture")).await
    }

    async fn void(&self, intent_id: Uuid) -> anyhow::Result<()> {
        self.post_no_body(&format!("intents/{intent_id}/void")).await
    }
}

impl CarrierPaymentsClient {
    async fn post_no_body(&self, suffix: &str) -> anyhow::Result<()> {
        let url = format!(
            "{}/v1/internal/payments/{suffix}",
            self.base_url.trim_end_matches('/'),
        );
        let resp = self.http.post(&url).send().await?;
        if let Err(e) = resp.error_for_status_ref() {
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("payments {suffix} failed: {e} — body: {body_text}");
        }
        Ok(())
    }
}
