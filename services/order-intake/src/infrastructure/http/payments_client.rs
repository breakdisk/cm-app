//! HTTP client for the payments service's mesh-internal payment-intent endpoint.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub struct PaymentsClient {
    base_url: String,
    http:     reqwest::Client,
}

impl PaymentsClient {
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
    purpose:        String,
    reference_type: String,
    reference_id:   Uuid,
    amount_cents:   i64,
    currency:       String,
    return_url:     String,
}

#[derive(Deserialize)]
pub struct CreatedIntent {
    pub intent_id:    Uuid,
    pub checkout_url: String,
}

impl PaymentsClient {
    pub async fn create_shipping_fee_intent(
        &self,
        tenant_id:    Uuid,
        shipment_id:  Uuid,
        amount_cents: i64,
        currency:     &str,
        return_url:   &str,
    ) -> anyhow::Result<CreatedIntent> {
        self.create_intent(tenant_id, "shipping_fee", "shipment", shipment_id, amount_cents, currency, return_url).await
    }

    /// A whole-home move's survey addendum, approved by the customer: its own
    /// payment, so the shipment's captured-payment handling never sees it.
    pub async fn create_home_addendum_intent(
        &self,
        tenant_id:    Uuid,
        addendum_id:  Uuid,
        amount_cents: i64,
        currency:     &str,
        return_url:   &str,
    ) -> anyhow::Result<CreatedIntent> {
        self.create_intent(tenant_id, "home_addendum", "home_addendum", addendum_id, amount_cents, currency, return_url).await
    }

    #[allow(clippy::too_many_arguments)]
    async fn create_intent(
        &self,
        tenant_id:      Uuid,
        purpose:        &str,
        reference_type: &str,
        reference_id:   Uuid,
        amount_cents:   i64,
        currency:       &str,
        return_url:     &str,
    ) -> anyhow::Result<CreatedIntent> {
        let url = format!("{}/v1/internal/payments/intents", self.base_url.trim_end_matches('/'));
        let resp = self.http
            .post(&url)
            .json(&CreateIntentRequest {
                tenant_id,
                purpose: purpose.into(),
                reference_type: reference_type.into(),
                reference_id,
                amount_cents,
                currency: currency.into(),
                return_url: return_url.into(),
            })
            .send()
            .await?;

        if let Err(e) = resp.error_for_status_ref() {
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("payments create_intent failed: {e} — body: {body_text}");
        }

        let resp = resp.json::<CreatedIntent>().await?;
        Ok(resp)
    }
}
