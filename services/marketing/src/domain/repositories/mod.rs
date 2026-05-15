use async_trait::async_trait;
use logisticos_types::TenantId;
use crate::domain::entities::{Campaign, CampaignId, CampaignStatus, WeeklyStat};

#[async_trait]
pub trait CampaignRepository: Send + Sync {
    async fn find_by_id(&self, id: &CampaignId) -> anyhow::Result<Option<Campaign>>;
    async fn list(&self, tenant_id: &TenantId, limit: i64, offset: i64) -> anyhow::Result<Vec<Campaign>>;
    async fn list_by_status(&self, tenant_id: &TenantId, status: &CampaignStatus) -> anyhow::Result<Vec<Campaign>>;
    async fn save(&self, campaign: &Campaign) -> anyhow::Result<()>;
    /// Aggregate total_sent per day per channel over the last 7 days.
    async fn weekly_stats(&self, tenant_id: &TenantId) -> anyhow::Result<Vec<WeeklyStat>>;
    /// Return up to `limit` scheduled campaigns whose `scheduled_at <= NOW()`.
    async fn find_campaigns_due(&self, limit: i64) -> anyhow::Result<Vec<Campaign>>;
}
