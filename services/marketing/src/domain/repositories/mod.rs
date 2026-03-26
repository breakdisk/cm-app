use async_trait::async_trait;
use logisticos_types::TenantId;
use crate::domain::entities::{Campaign, CampaignId, CampaignStatus};

#[async_trait]
pub trait CampaignRepository: Send + Sync {
    async fn find_by_id(&self, id: &CampaignId) -> anyhow::Result<Option<Campaign>>;
    async fn list(&self, tenant_id: &TenantId, limit: i64, offset: i64) -> anyhow::Result<Vec<Campaign>>;
    async fn list_by_status(&self, tenant_id: &TenantId, status: &CampaignStatus) -> anyhow::Result<Vec<Campaign>>;
    async fn save(&self, campaign: &Campaign) -> anyhow::Result<()>;
}
