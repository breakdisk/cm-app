use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use logisticos_types::TenantId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CampaignId(Uuid);

impl CampaignId {
    pub fn new() -> Self { Self(Uuid::new_v4()) }
    pub fn from_uuid(id: Uuid) -> Self { Self(id) }
    pub fn inner(&self) -> Uuid { self.0 }
}

impl Default for CampaignId {
    fn default() -> Self { Self::new() }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    WhatsApp,
    Sms,
    Email,
    Push,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CampaignStatus {
    Draft,
    Scheduled,
    Sending,
    Completed,
    Cancelled,
    Failed,
}

/// Targeting rule: recipients are customers matching these CDP criteria.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetingRule {
    /// Minimum CLV score (0-100). None = no minimum.
    pub min_clv_score:       Option<f32>,
    /// Maximum days since last shipment.
    pub last_active_days:    Option<u32>,
    /// Specific customer_ids (if set, bypasses other rules).
    pub customer_ids:        Vec<Uuid>,
    /// Estimated recipient count (filled at campaign creation time via CDP query).
    pub estimated_reach:     u64,
}

impl Default for TargetingRule {
    fn default() -> Self {
        Self {
            min_clv_score:   None,
            last_active_days: None,
            customer_ids:    Vec::new(),
            estimated_reach: 0,
        }
    }
}

/// Per-channel message template references.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageTemplate {
    pub template_id:  String,     // references engagement service template registry
    pub subject:      Option<String>, // email only
    pub variables:    serde_json::Value, // key-value pairs passed to template engine
}

/// A campaign: a targeted message sent to a segment of customers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Campaign {
    pub id:              CampaignId,
    pub tenant_id:       TenantId,
    pub name:            String,
    pub description:     Option<String>,

    pub channel:         Channel,
    pub template:        MessageTemplate,
    pub targeting:       TargetingRule,

    pub status:          CampaignStatus,
    pub scheduled_at:    Option<DateTime<Utc>>,
    pub sent_at:         Option<DateTime<Utc>>,
    pub completed_at:    Option<DateTime<Utc>>,

    // Send metrics (updated as notifications are dispatched)
    pub total_sent:      u64,
    pub total_delivered: u64,
    pub total_failed:    u64,

    pub created_by:      Uuid,   // user_id
    pub created_at:      DateTime<Utc>,
    pub updated_at:      DateTime<Utc>,
}

impl Campaign {
    pub fn new(
        tenant_id: TenantId,
        name: String,
        description: Option<String>,
        channel: Channel,
        template: MessageTemplate,
        targeting: TargetingRule,
        created_by: Uuid,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: CampaignId::new(),
            tenant_id,
            name,
            description,
            channel,
            template,
            targeting,
            status: CampaignStatus::Draft,
            scheduled_at: None,
            sent_at: None,
            completed_at: None,
            total_sent: 0,
            total_delivered: 0,
            total_failed: 0,
            created_by,
            created_at: now,
            updated_at: now,
        }
    }

    /// Schedule the campaign for a future send time.
    pub fn schedule(&mut self, at: DateTime<Utc>) -> anyhow::Result<()> {
        if self.status != CampaignStatus::Draft {
            anyhow::bail!("Only Draft campaigns can be scheduled");
        }
        if at <= Utc::now() {
            anyhow::bail!("Scheduled time must be in the future");
        }
        self.scheduled_at = Some(at);
        self.status = CampaignStatus::Scheduled;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Activate immediate send.
    pub fn activate(&mut self) -> anyhow::Result<()> {
        if !matches!(self.status, CampaignStatus::Draft | CampaignStatus::Scheduled) {
            anyhow::bail!("Cannot activate campaign with status {:?}", self.status);
        }
        self.status = CampaignStatus::Sending;
        self.sent_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Mark the campaign as completed after all sends have been dispatched.
    pub fn complete(&mut self, sent: u64, delivered: u64, failed: u64) {
        self.total_sent = sent;
        self.total_delivered = delivered;
        self.total_failed = failed;
        self.status = CampaignStatus::Completed;
        self.completed_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    pub fn cancel(&mut self) -> anyhow::Result<()> {
        if self.status == CampaignStatus::Sending {
            anyhow::bail!("Cannot cancel a campaign that is already sending");
        }
        if self.status == CampaignStatus::Completed {
            anyhow::bail!("Campaign already completed");
        }
        self.status = CampaignStatus::Cancelled;
        self.updated_at = Utc::now();
        Ok(())
    }
}
