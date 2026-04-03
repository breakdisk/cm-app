use std::sync::Arc;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use logisticos_errors::{AppError, AppResult};
use logisticos_events::{topics, Event};
use logisticos_types::TenantId;

use crate::domain::{
    entities::{Campaign, CampaignId, Channel, MessageTemplate, TargetingRule},
    repositories::CampaignRepository,
};

/// Minimal Kafka publisher interface — keeps the service testable.
#[async_trait::async_trait]
pub trait EventPublisher: Send + Sync {
    async fn publish(&self, topic: &str, key: &str, payload: &[u8]) -> anyhow::Result<()>;
}

#[derive(Debug, Deserialize)]
pub struct CreateCampaignCommand {
    pub name:         String,
    pub description:  Option<String>,
    pub channel:      Channel,
    pub template:     MessageTemplate,
    pub targeting:    TargetingRule,
}

#[derive(Debug, Deserialize)]
pub struct ScheduleCampaignCommand {
    pub scheduled_at: DateTime<Utc>,
}

pub struct CampaignService {
    repo:      Arc<dyn CampaignRepository>,
    publisher: Arc<dyn EventPublisher>,
}

impl CampaignService {
    pub fn new(repo: Arc<dyn CampaignRepository>, publisher: Arc<dyn EventPublisher>) -> Self {
        Self { repo, publisher }
    }

    pub async fn create(
        &self,
        tenant_id: &TenantId,
        created_by: Uuid,
        cmd: CreateCampaignCommand,
    ) -> AppResult<Campaign> {
        let campaign = Campaign::new(
            tenant_id.clone(),
            cmd.name,
            cmd.description,
            cmd.channel,
            cmd.template,
            cmd.targeting,
            created_by,
        );
        self.repo.save(&campaign).await.map_err(AppError::internal)?;
        Ok(campaign)
    }

    pub async fn get(&self, id: Uuid) -> AppResult<Campaign> {
        self.repo
            .find_by_id(&CampaignId::from_uuid(id))
            .await
            .map_err(AppError::internal)?
            .ok_or_else(|| AppError::NotFound { resource: "Campaign", id: id.to_string() })
    }

    pub async fn list(&self, tenant_id: &TenantId, limit: i64, offset: i64) -> AppResult<Vec<Campaign>> {
        self.repo
            .list(tenant_id, limit.clamp(1, 100), offset.max(0))
            .await
            .map_err(AppError::internal)
    }

    pub async fn schedule(&self, id: Uuid, cmd: ScheduleCampaignCommand) -> AppResult<Campaign> {
        let mut campaign = self.get(id).await?;
        campaign.schedule(cmd.scheduled_at)
            .map_err(|e| AppError::BusinessRule(e.to_string()))?;
        self.repo.save(&campaign).await.map_err(AppError::internal)?;
        Ok(campaign)
    }

    /// Activate campaign — queues notification sends via Kafka → engagement service.
    pub async fn activate(&self, id: Uuid) -> AppResult<Campaign> {
        let mut campaign = self.get(id).await?;
        campaign.activate().map_err(|e| AppError::BusinessRule(e.to_string()))?;
        self.repo.save(&campaign).await.map_err(AppError::internal)?;

        // Publish CAMPAIGN_TRIGGERED event so the engagement service starts sending.
        let payload = serde_json::json!({
            "campaign_id":  campaign.id.inner(),
            "tenant_id":    campaign.tenant_id.inner(),
            "channel":      campaign.channel,
            "template_id":  campaign.template.template_id,
            "variables":    campaign.template.variables,
            "targeting":    campaign.targeting,
        });
        self.publisher
            .publish(
                topics::CAMPAIGN_TRIGGERED,
                &campaign.id.inner().to_string(),
                serde_json::to_vec(&payload).unwrap_or_default().as_slice(),
            )
            .await
            .map_err(AppError::internal)?;

        tracing::info!(
            campaign_id = %campaign.id.inner(),
            reach = campaign.targeting.estimated_reach,
            channel = ?campaign.channel,
            "Campaign activated"
        );

        Ok(campaign)
    }

    pub async fn cancel(&self, id: Uuid) -> AppResult<Campaign> {
        let mut campaign = self.get(id).await?;
        campaign.cancel().map_err(|e| AppError::BusinessRule(e.to_string()))?;
        self.repo.save(&campaign).await.map_err(AppError::internal)?;
        Ok(campaign)
    }
}
