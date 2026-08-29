//! Telling identity what tier a tenant is entitled to.
//!
//! The tier lives in another service's database, so a subscription payment can
//! be captured here and the entitlement fail to arrive there. That gap is why
//! `subscriptions.tier_synced_at` exists: this client is allowed to fail, and
//! the sweep retries from that column until it succeeds. A fire-and-forget
//! event would have the same failure mode with no record of it.
//!
//! Authentication is the pre-existing `X-Internal-Secret` header identity
//! already uses for `/v1/internal/auth/exchange-firebase`, not a JWT. That is
//! deliberate: the tenant-facing route for this is `PUT /v1/tenants/:id/tier`,
//! which requires `tenants:manage` — a permission no role grants, because it
//! also rewrites the platform-wide pricing matrix. Minting a token that clears
//! that check would be recreating the free-self-upgrade this whole design
//! exists to avoid. There is no principal here; there is a system acting on a
//! payment it already verified.

use uuid::Uuid;

/// Grants a tenant a subscription tier.
#[async_trait::async_trait]
pub trait TenantTierSync: Send + Sync {
    async fn set_tier(&self, tenant_id: Uuid, tier: &str) -> anyhow::Result<()>;
}

pub struct IdentityClient {
    base_url: String,
    secret:   String,
    http:     reqwest::Client,
}

impl IdentityClient {
    pub fn new(base_url: impl Into<String>, secret: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("identity HTTP client");
        Self { base_url: base_url.into(), secret: secret.into(), http }
    }
}

#[async_trait::async_trait]
impl TenantTierSync for IdentityClient {
    async fn set_tier(&self, tenant_id: Uuid, tier: &str) -> anyhow::Result<()> {
        let url = format!(
            "{}/v1/internal/tenants/{tenant_id}/tier",
            self.base_url.trim_end_matches('/'),
        );
        let resp = self.http
            .put(&url)
            .header("X-Internal-Secret", &self.secret)
            .json(&serde_json::json!({ "tier": tier }))
            .send()
            .await?;

        if let Err(e) = resp.error_for_status_ref() {
            // The body carries identity's reason. Without reading it, a
            // permanently-failing sync is indistinguishable from a transient
            // one in the logs, and the sweep retries it forever.
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("identity tier sync failed: {e} — body: {body}");
        }
        Ok(())
    }
}
