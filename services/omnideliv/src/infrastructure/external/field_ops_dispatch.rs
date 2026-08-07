//! Offers a placed order to field-ops couriers.
//!
//! OmniDeliv is a product tier and field-ops is the platform tier that owns
//! couriers (ADR-0015), so this is a product calling a platform service —
//! the allowed direction. The `CourierDispatch` trait it implements belongs to
//! OmniDeliv, so the dependency points inward and field-ops knows nothing about
//! this caller.

use async_trait::async_trait;
use serde::Deserialize;
use uuid::Uuid;

use crate::application::services::CourierDispatch;

pub struct FieldOpsDispatch {
    http:         reqwest::Client,
    base_url:     String,
    /// Service-to-service token. field-ops authenticates every operational
    /// route, so an unauthenticated call is a 401 that looks exactly like
    /// "no couriers" unless it is distinguished — see the status check below.
    service_token: String,
}

impl FieldOpsDispatch {
    pub fn new(base_url: String, service_token: String) -> Self {
        Self { http: reqwest::Client::new(), base_url, service_token }
    }
}

#[derive(Debug, Deserialize)]
struct OfferResponse {
    assignment_ids: Vec<Uuid>,
}

#[async_trait]
impl CourierDispatch for FieldOpsDispatch {
    async fn offer(
        &self,
        _tenant_id: Uuid,
        order_id: Uuid,
        lat: f64,
        lng: f64,
    ) -> anyhow::Result<Vec<Uuid>> {
        // Tenant is not sent: field-ops reads it from the token, and a
        // caller-supplied tenant would let one tenant offer work to another's
        // couriers. The order id travels as `external_ref` — field-ops stores
        // it opaquely and never interprets it, which is what keeps the platform
        // tier product-agnostic.
        let res = self
            .http
            .post(format!("{}/v1/field-ops/assignments/offer", self.base_url))
            .bearer_auth(&self.service_token)
            .json(&serde_json::json!({
                "product":      "omnideliv",
                "external_ref": order_id,
                "lat":          lat,
                "lng":          lng,
            }))
            .send()
            .await?;

        // An auth or routing failure must not read as "no couriers available".
        // Both end in an empty list at the call site, but only one of them is a
        // reason to tell the customer to try again later — the other is an
        // outage that needs to page someone.
        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            anyhow::bail!("field-ops offer failed: {status} {body}");
        }

        Ok(res.json::<OfferResponse>().await?.assignment_ids)
    }
}
