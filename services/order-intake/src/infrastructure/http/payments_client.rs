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
        let url = format!("{}/v1/internal/payments/intents", self.base_url.trim_end_matches('/'));
        let resp = self.http
            .post(&url)
            .json(&CreateIntentRequest {
                tenant_id,
                purpose: "shipping_fee".into(),
                reference_type: "shipment".into(),
                reference_id: shipment_id,
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
