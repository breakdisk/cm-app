use uuid::Uuid;

/// HTTP client for fetching POP + POD evidence from the pod service.
///
/// Calls the internal `/v1/internal/pop-evidence/:shipment_id` endpoint which
/// requires no JWT — it is protected by Istio mTLS in production and by the
/// private Docker network in staging/dev.
///
/// All errors are logged and surfaced as `None` — evidence is optional data
/// for enriching tracking responses; a pod-service outage must not break tracking.
#[derive(Clone)]
pub struct PodClient {
    inner:    reqwest::Client,
    base_url: String,
}

impl PodClient {
    pub fn new(base_url: String) -> anyhow::Result<Self> {
        let inner = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .map_err(|e| anyhow::anyhow!("failed to build PodClient HTTP client: {e}"))?;
        Ok(Self { inner, base_url })
    }

    /// Fetch combined POP + POD evidence for a shipment.
    ///
    /// Returns `None` on any network or parse error — callers treat absent
    /// evidence as optional and return the tracking record without it.
    pub async fn get_evidence(&self, shipment_id: Uuid) -> Option<serde_json::Value> {
        let url = format!(
            "{}/v1/internal/pop-evidence/{}",
            self.base_url.trim_end_matches('/'),
            shipment_id,
        );

        let resp = self.inner
            .get(&url)
            .send()
            .await
            .map_err(|e| tracing::warn!(
                error = %e,
                %shipment_id,
                "PodClient: network error fetching evidence"
            ))
            .ok()?;

        if !resp.status().is_success() {
            tracing::warn!(
                status = resp.status().as_u16(),
                %shipment_id,
                "PodClient: pod service returned non-2xx for evidence"
            );
            return None;
        }

        resp.json::<serde_json::Value>()
            .await
            .map_err(|e| tracing::warn!(
                error = %e,
                %shipment_id,
                "PodClient: failed to deserialise evidence response"
            ))
            .ok()
    }
}
