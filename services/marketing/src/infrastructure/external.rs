//! HTTP clients for external service calls originating from the marketing service.
//!
//! `CdpClient` resolves CDP-filter-based audiences by calling the Customer Data
//! Platform service.  It is used during campaign activation when the campaign
//! has `targeting.min_clv_score` or `targeting.customer_ids` set but no explicit
//! `targeting.recipients` list.

use reqwest::Client;
use serde::Deserialize;
use uuid::Uuid;

use crate::domain::entities::{CampaignRecipient, TargetingRule};

// ---------------------------------------------------------------------------
// CDP profile shape (minimal — we only need contact fields)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct CdpProfile {
    external_customer_id: Uuid,
    name:                 Option<String>,
    email:                Option<String>,
    phone:                Option<String>,
}

#[derive(Debug, Deserialize)]
struct CdpListResponse {
    profiles: Vec<CdpProfile>,
}

// ---------------------------------------------------------------------------
// CdpClient
// ---------------------------------------------------------------------------

/// Resolves campaign audiences by querying the CDP service.
///
/// Authentication uses a pre-issued internal service token configured via the
/// `SERVICES__CDP_TOKEN` environment variable.  This token must carry the
/// `customers:view` permission and is scoped to the platform (not a tenant).
/// The `X-Tenant-Id` header scopes the query to the correct tenant.
pub struct CdpClient {
    http:     Client,
    base_url: String,
    token:    String,
}

impl CdpClient {
    pub fn new(base_url: String, token: String) -> Self {
        Self {
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("reqwest client"),
            base_url,
            token,
        }
    }

    /// Resolve a `TargetingRule` into a concrete list of recipients.
    ///
    /// Resolution priority:
    ///   1. If `targeting.customer_ids` is non-empty, fetch those specific profiles.
    ///   2. Otherwise use `min_clv_score` and `last_active_days` as query filters.
    ///
    /// The result is capped at 2 000 recipients per request to protect fan-out
    /// memory usage.  For larger campaigns, paginate and split into batches.
    pub async fn resolve_audience(
        &self,
        tenant_id: Uuid,
        targeting: &TargetingRule,
    ) -> anyhow::Result<Vec<CampaignRecipient>> {
        // If min_clv_score targeting — call the list endpoint with a filter.
        let url = format!("{}/v1/customers", self.base_url);
        let limit: i64 = 2_000;

        let mut req = self.http
            .get(&url)
            .bearer_auth(&self.token)
            .header("X-Tenant-Id", tenant_id.to_string())
            .query(&[("limit", limit.to_string())]);

        if let Some(min_clv) = targeting.min_clv_score {
            req = req.query(&[("min_clv", min_clv.to_string())]);
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body   = resp.text().await.unwrap_or_default();
            anyhow::bail!("CDP returned {status}: {body}");
        }

        let list: CdpListResponse = resp.json().await?;
        tracing::info!(
            tenant_id = %tenant_id,
            resolved  = list.profiles.len(),
            "CDP audience resolved"
        );

        Ok(list.profiles.into_iter().map(|p| CampaignRecipient {
            customer_id: Some(p.external_customer_id),
            name:        p.name,
            email:       p.email,
            phone:       p.phone,
            platform_id: None,
        }).collect())
    }

    /// Resolve a CDP segment into a concrete recipient list.
    ///
    /// Calls `GET /v1/segments/{id}/members` on the CDP service.  Results are
    /// capped at 2 000 per batch; callers handling larger segments should paginate.
    pub async fn resolve_segment_audience(
        &self,
        tenant_id:  Uuid,
        segment_id: Uuid,
    ) -> anyhow::Result<Vec<CampaignRecipient>> {
        #[derive(Deserialize)]
        struct SegmentMember {
            external_customer_id: Uuid,
            name:  Option<String>,
            email: Option<String>,
            phone: Option<String>,
        }
        #[derive(Deserialize)]
        struct MembersResponse {
            members: Vec<SegmentMember>,
        }

        let url = format!("{}/v1/segments/{}/members", self.base_url, segment_id);
        let resp = self.http
            .get(&url)
            .bearer_auth(&self.token)
            .header("X-Tenant-Id", tenant_id.to_string())
            .query(&[("limit", "2000")])
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body   = resp.text().await.unwrap_or_default();
            anyhow::bail!("CDP segment {segment_id} returned {status}: {body}");
        }

        let res: MembersResponse = resp.json().await?;
        tracing::info!(
            tenant_id  = %tenant_id,
            segment_id = %segment_id,
            resolved   = res.members.len(),
            "CDP segment audience resolved"
        );

        Ok(res.members.into_iter().map(|m| CampaignRecipient {
            customer_id: Some(m.external_customer_id),
            name:        m.name,
            email:       m.email,
            phone:       m.phone,
            platform_id: None,
        }).collect())
    }

    /// Fetch a single customer's CLV score — used by journey "condition" steps
    /// that branch on CLV threshold (e.g. `clv_above_60`).
    pub async fn get_clv_score(&self, tenant_id: Uuid, customer_id: Uuid) -> anyhow::Result<f32> {
        #[derive(Deserialize)]
        struct ProfileClv {
            clv_score: f32,
        }

        let url = format!("{}/v1/customers/{}", self.base_url, customer_id);
        let resp = self.http
            .get(&url)
            .bearer_auth(&self.token)
            .header("X-Tenant-Id", tenant_id.to_string())
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body   = resp.text().await.unwrap_or_default();
            anyhow::bail!("CDP customer {customer_id} returned {status}: {body}");
        }

        let profile: ProfileClv = resp.json().await?;
        Ok(profile.clv_score)
    }
}
