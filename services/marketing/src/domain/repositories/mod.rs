use async_trait::async_trait;
use uuid::Uuid;
use logisticos_types::TenantId;
use crate::domain::entities::{Campaign, CampaignId, CampaignRecipient, CampaignStatus, WeeklyStat};

#[async_trait]
pub trait CampaignRepository: Send + Sync {
    async fn find_by_id(&self, id: &CampaignId) -> anyhow::Result<Option<Campaign>>;
    async fn list(&self, tenant_id: &TenantId, limit: i64, offset: i64) -> anyhow::Result<Vec<Campaign>>;
    async fn list_by_status(&self, tenant_id: &TenantId, status: &CampaignStatus) -> anyhow::Result<Vec<Campaign>>;
    async fn save(&self, campaign: &Campaign) -> anyhow::Result<()>;
    /// Overwrite the `targeting.recipients` array for a campaign without touching
    /// any other fields.  Used by the CDP audience-resolution path so that a
    /// subsequent `save` with the full Campaign struct is not required.
    async fn patch_recipients(&self, id: &CampaignId, recipients: Vec<CampaignRecipient>) -> anyhow::Result<()>;
    /// Aggregate total_sent per day per channel over the last 7 days.
    async fn weekly_stats(&self, tenant_id: &TenantId) -> anyhow::Result<Vec<WeeklyStat>>;
    /// Return up to `limit` scheduled campaigns whose `scheduled_at <= NOW()`.
    async fn find_campaigns_due(&self, limit: i64) -> anyhow::Result<Vec<Campaign>>;
}

/// The one A/B-test read `activate` depends on, behind a trait so a lookup
/// failure can be exercised without a live Postgres.
#[async_trait]
pub trait AbTestLookup: Send + Sync {
    /// `Ok(None)` means the campaign has no A/B test; `Err` means we could not tell.
    async fn find_by_campaign(&self, campaign_id: Uuid) -> anyhow::Result<Option<crate::domain::entities::AbTest>>;
}
