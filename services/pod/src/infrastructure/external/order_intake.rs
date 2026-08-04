//! HTTP client for order-intake's internal shipment endpoints.
//!
//! Implements one driven port:
//! - `ShipmentBillingContextSource` — `GET /v1/internal/shipments/:id/billing`
//!
//! No JWT is attached: the `/v1/internal/*` routes are mTLS-only by design
//! (Istio enforces that only in-mesh services can reach them), matching how
//! payments' `OrderIntakeClient` calls the same endpoint.

use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::repositories::{ShipmentBillingContext, ShipmentBillingContextSource};

/// Wall-clock ceiling for the lookup. A driver standing at the merchant's
/// counter must not wait on a slow internal hop — `initiate_pickup` falls back
/// to the client-supplied values when this budget is exceeded.
const LOOKUP_TIMEOUT_SECS: u64 = 5;

pub struct OrderIntakeClient {
    base_url: String,
    http:     reqwest::Client,
}

impl OrderIntakeClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            http:     reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(LOOKUP_TIMEOUT_SECS))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }
}

/// Subset of `GET /v1/internal/shipments/:id/billing` that POP cares about.
/// The endpoint also returns the computed fee breakdown, which is ignored here.
#[derive(Debug, serde::Deserialize)]
struct BillingContextResponse {
    #[serde(default)]
    service_code:         Option<String>,
    #[serde(default)]
    declared_value_cents: Option<i64>,
}

#[async_trait]
impl ShipmentBillingContextSource for OrderIntakeClient {
    async fn fetch(&self, shipment_id: Uuid) -> anyhow::Result<ShipmentBillingContext> {
        let url = format!(
            "{}/v1/internal/shipments/{}/billing",
            self.base_url.trim_end_matches('/'),
            shipment_id,
        );

        let resp = self.http
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .json::<BillingContextResponse>()
            .await?;

        // An order-intake old enough to predate these fields would deserialise
        // both as None. Treat a missing service_code as an error rather than
        // silently classifying a Balikbayan parcel as "standard".
        let service_code = resp.service_code.ok_or_else(|| {
            anyhow::anyhow!(
                "order-intake returned no service_code for shipment {shipment_id} \
                 (deployed version predates the POP billing-context fields?)"
            )
        })?;

        Ok(ShipmentBillingContext {
            service_code,
            declared_value_cents: resp.declared_value_cents,
        })
    }
}
