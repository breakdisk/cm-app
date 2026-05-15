use reqwest::Client;
use serde::Serialize;
use std::time::Duration;

// ── Outbound webhook payloads ─────────────────────────────────────────────────

/// Sent to the carrier's registered `api_endpoint` when a shipment is
/// allocated to them. Allows 3PL systems to begin processing immediately
/// without polling the LogisticOS API.
#[derive(Debug, Serialize)]
pub struct CarrierShipmentNotification {
    pub event:        &'static str,    // "shipment.allocated"
    pub shipment_id:  String,
    pub awb:          String,
    pub service_type: String,
    pub zone:         String,
    pub promised_by:  String,          // ISO-8601
    pub pickup_address: Option<String>,
    pub dropoff_address: Option<String>,
    pub weight_kg:    Option<f32>,
}

/// Sent when a shipment is cancelled after it was already assigned to the carrier.
#[derive(Debug, Serialize)]
pub struct CarrierCancellationNotification {
    pub event:       &'static str,    // "shipment.cancelled"
    pub shipment_id: String,
    pub awb:         String,
    pub reason:      String,
}

// ── Client ────────────────────────────────────────────────────────────────────

/// HTTP client for pushing events to 3PL carrier webhook endpoints.
///
/// The carrier's `api_endpoint` is a URL registered in its profile
/// (`PUT /v1/carriers/:id` → `api_endpoint` field). Calls are best-effort —
/// a failed push is logged and retried once; persistent failures do not block
/// the platform.
pub struct ExternalCarrierClient {
    http:           Client,
    /// If present, added as `X-LogisticOS-Secret` header on every request
    /// so carriers can verify the call originated from LogisticOS.
    outbound_secret: Option<String>,
}

impl ExternalCarrierClient {
    pub fn new(outbound_secret: Option<String>) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent("LogisticOS-CarrierWebhook/1.0")
            .build()
            .expect("Failed to build external carrier HTTP client");
        Self { http, outbound_secret }
    }

    /// POST the payload to `endpoint`. Returns `Ok(())` on HTTP 2xx;
    /// returns `Err` on network failure or non-2xx response (caller decides
    /// whether to retry or discard).
    pub async fn post_event<T: Serialize>(
        &self,
        endpoint: &str,
        payload: &T,
    ) -> anyhow::Result<()> {
        let mut builder = self.http.post(endpoint).json(payload);
        if let Some(ref secret) = self.outbound_secret {
            builder = builder.header("X-LogisticOS-Secret", secret.as_str());
        }
        let resp = builder.send().await?;
        if resp.status().is_success() {
            Ok(())
        } else {
            anyhow::bail!(
                "Carrier webhook returned non-2xx: {} {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            )
        }
    }

    /// Notify the carrier of a new shipment allocation (best-effort, one retry).
    pub async fn notify_shipment_allocated(
        &self,
        endpoint: &str,
        notif: &CarrierShipmentNotification,
    ) {
        if let Err(e) = self.post_event(endpoint, notif).await {
            tracing::warn!(endpoint, error = %e, "First attempt to notify carrier of allocation failed — retrying");
            if let Err(e2) = self.post_event(endpoint, notif).await {
                tracing::error!(endpoint, error = %e2, shipment_id = notif.shipment_id, "Carrier allocation webhook failed after retry");
            }
        }
    }

    /// Notify the carrier of a cancellation (best-effort, one retry).
    pub async fn notify_shipment_cancelled(
        &self,
        endpoint: &str,
        notif: &CarrierCancellationNotification,
    ) {
        if let Err(e) = self.post_event(endpoint, notif).await {
            tracing::warn!(endpoint, error = %e, "First attempt to notify carrier of cancellation failed — retrying");
            if let Err(e2) = self.post_event(endpoint, notif).await {
                tracing::error!(endpoint, error = %e2, shipment_id = notif.shipment_id, "Carrier cancellation webhook failed after retry");
            }
        }
    }
}
