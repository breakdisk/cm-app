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
    /// Serialises as "whatsapp". The alias "whats_app" handles any records
    /// written before this fix (serde snake_case was splitting on the capital A).
    #[serde(rename = "whatsapp", alias = "whats_app")]
    WhatsApp,
    Sms,
    Email,
    Push,
    // Social / CRM integration channels — dispatched via LogChannelAdapter stub
    // until platform-specific API connectors are wired per-tenant.
    Messenger,
    Telegram,
    #[serde(rename = "x")]
    X,
    Viber,
    #[serde(rename = "wechat")]
    WeChat,
    Line,
    Slack,
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

/// A single recipient with full contact details embedded in the campaign payload.
/// Used for explicit list-based targeting — bypasses CDP segment resolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CampaignRecipient {
    /// CDP customer profile ID, if known.
    pub customer_id: Option<Uuid>,
    /// Phone number in E.164 format (e.g. "+63912345678"). Used for WhatsApp/SMS.
    pub phone:       Option<String>,
    /// Email address. Used for the email channel.
    pub email:       Option<String>,
    /// Display name used in template variables ({{customer_name}}).
    pub name:        Option<String>,
    /// Platform-specific user/chat identifier for social channels
    /// (Facebook PSID, Telegram chat_id, Slack user_id, Viber URI, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform_id: Option<String>,
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
    /// Explicit recipient list with embedded contact details.
    /// When non-empty, fan-out uses this list directly — no CDP lookup required.
    #[serde(default)]
    pub recipients:          Vec<CampaignRecipient>,
    /// Estimated recipient count (filled at campaign creation time via CDP query).
    pub estimated_reach:     u64,
    /// CDP segment UUID. When set and `recipients` is empty, the marketing
    /// service resolves the audience from the segment at activation time.
    /// Stored in the JSONB `targeting` column — no migration required.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segment_id:          Option<Uuid>,
}

impl Default for TargetingRule {
    fn default() -> Self {
        Self {
            min_clv_score:    None,
            last_active_days: None,
            customer_ids:     Vec::new(),
            recipients:       Vec::new(),
            estimated_reach:  0,
            segment_id:       None,
        }
    }
}

/// Daily message-volume aggregation returned by the weekly-stats endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct WeeklyStat {
    /// ISO 8601 date string, e.g. "2026-05-13".
    pub day:     String,
    pub channel: String,
    pub count:   i64,
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

// ── A/B Testing ──────────────────────────────────────────────────────────────

/// A single variant in an A/B test (e.g. different template or subject line).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbVariant {
    pub name:        String,   // "A" | "B" | "C"
    pub template_id: String,
    pub weight_pct:  u8,       // 0..100; variants in a test should sum to 100
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbTest {
    pub id:             Uuid,
    pub tenant_id:      Uuid,
    pub campaign_id:    Uuid,
    pub name:           String,
    pub variants:       Vec<AbVariant>,
    pub winner_variant: Option<String>,
    pub started_at:     DateTime<Utc>,
    pub concluded_at:   Option<DateTime<Utc>>,
}

/// Per-variant send performance aggregated from `marketing.send_log`.
#[derive(Debug, Clone, Serialize)]
pub struct AbVariantStats {
    pub variant:   String,
    pub sent:      i64,
    pub delivered: i64,
    pub opened:    i64,
    pub clicked:   i64,
}

// ── Journey Builder ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum JourneyStatus { Draft, Active, Paused, Archived }

/// One step in a journey.
/// `step_type` is "send_campaign" | "wait" | "condition".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JourneyStep {
    pub id:                    Uuid,
    pub journey_id:            Uuid,
    pub step_order:            i32,
    pub step_type:             String,
    pub campaign_id:           Option<Uuid>,
    pub wait_days:             Option<i32>,
    pub condition_type:        Option<String>,
    pub condition_campaign_id: Option<Uuid>,
    pub yes_next_order:        Option<i32>,
    pub no_next_order:         Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Journey {
    pub id:          Uuid,
    pub tenant_id:   Uuid,
    pub name:        String,
    pub description: Option<String>,
    /// JSON object: `{ "type": "manual_enroll" | "campaign_opened" | "segment_entered", ... }`
    pub trigger:     serde_json::Value,
    pub status:      JourneyStatus,
    pub steps:       Vec<JourneyStep>,
    pub created_at:  DateTime<Utc>,
    pub updated_at:  DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JourneyEnrollment {
    pub id:                 Uuid,
    pub journey_id:         Uuid,
    pub tenant_id:          Uuid,
    pub customer_id:        Uuid,
    pub current_step_order: Option<i32>,
    pub status:             String,
    pub next_action_at:     Option<DateTime<Utc>>,
    pub enrolled_at:        DateTime<Utc>,
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
