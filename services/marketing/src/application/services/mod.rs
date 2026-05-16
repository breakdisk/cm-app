use std::sync::Arc;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use logisticos_errors::{AppError, AppResult};
use logisticos_events::{topics, Event};
use logisticos_types::TenantId;

use crate::domain::{
    entities::{Campaign, CampaignId, CampaignRecipient, Channel, MessageTemplate, TargetingRule, WeeklyStat},
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

    pub async fn list_by_status(
        &self,
        tenant_id: &TenantId,
        status:    &crate::domain::entities::CampaignStatus,
    ) -> AppResult<Vec<Campaign>> {
        self.repo
            .list_by_status(tenant_id, status)
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

    /// Overwrite the explicit recipient list on a campaign.
    ///
    /// Called by the CDP audience-resolution path (both HTTP and MCP) after
    /// `CdpClient::resolve_audience` returns a populated list.  The campaign
    /// must still be in Draft or Scheduled state; activating it is a separate
    /// call so that the caller retains full control over the sequence.
    pub async fn patch_recipients(
        &self,
        id:         Uuid,
        recipients: Vec<CampaignRecipient>,
    ) -> AppResult<()> {
        let campaign = self.get(id).await?;
        if !matches!(
            campaign.status,
            crate::domain::entities::CampaignStatus::Draft
                | crate::domain::entities::CampaignStatus::Scheduled
        ) {
            return Err(AppError::BusinessRule(format!(
                "Cannot patch recipients on a campaign with status {:?}",
                campaign.status
            )));
        }
        let count = recipients.len();
        self.repo
            .patch_recipients(&campaign.id, recipients)
            .await
            .map_err(AppError::internal)?;
        tracing::debug!(campaign_id = %id, recipient_count = count, "Campaign recipients patched from CDP");
        Ok(())
    }

    /// Activate campaign — queues notification sends via Kafka → engagement service.
    pub async fn activate(&self, id: Uuid) -> AppResult<Campaign> {
        let mut campaign = self.get(id).await?;
        campaign.activate().map_err(|e| AppError::BusinessRule(e.to_string()))?;
        self.repo.save(&campaign).await.map_err(AppError::internal)?;

        // Publish CAMPAIGN_TRIGGERED event so the engagement service starts sending.
        // Embed recipients with full contact details so the engagement consumer can
        // fan-out without a CDP lookup.  Also include `name` and `created_by` so the
        // engagement service can upsert an engagement.campaigns row before fan-out.
        // Embed recipients at the top level so the engagement consumer can read
        // `data["recipients"]` without unwrapping a nested targeting object.
        // `targeting` is omitted from the payload — recipients is the canonical
        // fan-out list after CDP resolution has already run.
        let payload = serde_json::json!({
            "campaign_id":  campaign.id.inner(),
            "tenant_id":    campaign.tenant_id.inner(),
            "name":         campaign.name,
            "created_by":   campaign.created_by,
            "channel":      campaign.channel,
            "template_id":  campaign.template.template_id,
            "subject":      campaign.template.subject,
            "variables":    campaign.template.variables,
            "recipients":   campaign.targeting.recipients,
        });
        let payload_bytes = serde_json::to_vec(&payload)
            .map_err(|e| AppError::internal(anyhow::anyhow!("Failed to serialize CAMPAIGN_TRIGGERED: {}", e)))?;
        self.publisher
            .publish(
                topics::CAMPAIGN_TRIGGERED,
                &campaign.id.inner().to_string(),
                &payload_bytes,
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

    /// Mark a campaign as completed after the engagement service finishes fan-out.
    /// Called when a `CAMPAIGN_COMPLETED` Kafka event is received.
    pub async fn complete(
        &self,
        id:              Uuid,
        total_sent:      u64,
        total_delivered: u64,
        total_failed:    u64,
    ) -> AppResult<Campaign> {
        let mut campaign = self.get(id).await?;
        campaign.complete(total_sent, total_delivered, total_failed);
        self.repo.save(&campaign).await.map_err(AppError::internal)?;
        Ok(campaign)
    }

    /// Return daily send totals per channel for the last 7 days.
    /// Powers the campaign volume chart in the Merchant Portal.
    pub async fn weekly_stats(&self, tenant_id: &TenantId) -> AppResult<Vec<WeeklyStat>> {
        self.repo.weekly_stats(tenant_id).await.map_err(AppError::internal)
    }

    /// Activate all scheduled campaigns whose `scheduled_at` has elapsed.
    /// Called every 60 s by the background poller in bootstrap.
    pub async fn activate_due_campaigns(&self) -> anyhow::Result<usize> {
        let due = self.repo.find_campaigns_due(50).await?;
        let count = due.len();
        for campaign in due {
            let id = campaign.id.inner();
            if let Err(e) = self.activate(id).await {
                tracing::warn!(campaign_id = %id, err = %e, "Failed to auto-activate due campaign");
            } else {
                tracing::info!(campaign_id = %id, "Scheduled campaign auto-activated");
            }
        }
        Ok(count)
    }
}
