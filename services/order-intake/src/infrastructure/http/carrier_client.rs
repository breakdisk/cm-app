//! HTTP client for carrier's mesh-internal itemised-quote endpoint.
//!
//! No JWT is attached: carrier mounts `/v1/internal/quote-breakdown` outside its
//! auth layer and Istio mTLS asserts the caller. This exists so a consumer can be
//! priced off a carrier rate card without holding `marketplace:book` — a
//! permission libs/auth has a test asserting the `customer` role must not have.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub struct CarrierClient {
    base_url: String,
    http: reqwest::Client,
}

impl CarrierClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("carrier HTTP client");
        Self { base_url: base_url.into(), http }
    }
}

#[derive(Serialize)]
struct QuoteBreakdownRequest {
    tenant_id: Uuid,
    distance_km: f32,
    billable_kg: f32,
}

/// Mirrors carrier's `QuoteBreakdown`. The three rows sum to `total_cents` on the
/// far side; `get_quote` re-checks that rather than trusting it.
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct Breakdown {
    pub base_cents: i64,
    pub distance_cents: i64,
    pub weight_cents: i64,
    pub total_cents: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CarrierQuoteBreakdown {
    pub listing_id: Uuid,
    pub carrier_id: Uuid,
    pub size_class: String,
    pub vehicle_label: String,
    pub breakdown: Breakdown,
    pub per_km_cents: i64,
    pub per_kg_cents: i64,
}

impl CarrierClient {
    pub async fn quote_breakdown(
        &self,
        tenant_id: Uuid,
        distance_km: f32,
        billable_kg: f32,
    ) -> Result<CarrierQuoteBreakdown, String> {
        let url = format!(
            "{}/v1/internal/quote-breakdown",
            self.base_url.trim_end_matches('/')
        );
        let resp = self
            .http
            .post(&url)
            .json(&QuoteBreakdownRequest { tenant_id, distance_km, billable_kg })
            .send()
            .await
            .map_err(|e| format!("carrier quote request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("carrier quote returned {status}: {body}"));
        }

        resp.json::<CarrierQuoteBreakdown>()
            .await
            .map_err(|e| format!("carrier quote response did not parse: {e}"))
    }
}
