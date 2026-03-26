//! Rules Engine — core domain logic for LogisticOS automation.
//!
//! Implements an event-condition-action (ECA) pattern:
//!   WHEN event OCCURS
//!   IF conditions ARE MET
//!   THEN execute actions

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationRule {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub description: String,
    pub is_active: bool,
    pub trigger: RuleTrigger,
    pub conditions: Vec<RuleCondition>,
    pub actions: Vec<RuleAction>,
    pub priority: u32,         // Lower number = higher priority
    pub created_at: DateTime<Utc>,
}

// ── Triggers ─────────────────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuleTrigger {
    DeliveryFailed,
    DeliveryCompleted,
    DeliveryAttempted { attempts: u32 },
    ShipmentCreated,
    ShipmentDelayed { minutes_late: u32 },
    CustomerInactive { days: u32 },
    DriverIdleExceeded { minutes: u32 },
    HubCapacityExceeded { threshold_pct: u8 },
    PaymentOverdue { days: u32 },
    SlaBreach { sla_type: String },
    CampaignTriggered { campaign_id: Uuid },
}

// ── Conditions ───────────────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuleCondition {
    ServiceType { equals: String },
    ShipmentValue { greater_than: i64 },
    Zone { in_zones: Vec<String> },
    CustomerSegment { segment_id: Uuid },
    TimeOfDay { hour_from: u8, hour_to: u8 },
    DayOfWeek { days: Vec<String> },
    AttemptCount { lte: u32 },
    MerchantTier { tiers: Vec<String> },
    WeatherCondition { condition: String },
    Custom { expression: String },  // DSL expression for complex rules
}

// ── Actions ──────────────────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuleAction {
    RescheduleDelivery { delay_hours: u32 },
    NotifyCustomer { channel: String, template_id: String },
    NotifyMerchant { template_id: String },
    AlertDispatcher { message: String, priority: String },
    AssignNewDriver,
    UpdateShipmentStatus { status: String },
    ApplyDiscount { percentage: f32 },
    TriggerCampaign { campaign_id: Uuid },
    CreateRefund { reason: String },
    EscalateToSupport { tier: u8 },
    RunAiDispatch,
    WebhookCall { url: String, method: String },
    LogAuditEvent { event_type: String },
}

impl AutomationRule {
    /// Evaluates whether the rule's conditions pass for a given event context.
    /// Returns true if all conditions evaluate to true (AND logic).
    /// Use OR by creating multiple rules with the same trigger.
    pub fn conditions_met(&self, ctx: &RuleContext) -> bool {
        self.conditions.iter().all(|cond| cond.evaluate(ctx))
    }
}

/// Context object passed to the rules engine when an event fires.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleContext {
    pub tenant_id: Uuid,
    pub event_type: String,
    pub shipment_id: Option<Uuid>,
    pub customer_id: Option<Uuid>,
    pub merchant_id: Option<Uuid>,
    pub driver_id: Option<Uuid>,
    pub service_type: Option<String>,
    pub zone: Option<String>,
    pub attempt_count: Option<u32>,
    pub shipment_value_cents: Option<i64>,
    pub current_hour: u8,
    pub current_day: String,
    pub metadata: serde_json::Value,
}

impl RuleCondition {
    pub fn evaluate(&self, ctx: &RuleContext) -> bool {
        match self {
            RuleCondition::ServiceType { equals } => {
                ctx.service_type.as_deref() == Some(equals.as_str())
            }
            RuleCondition::ShipmentValue { greater_than } => {
                ctx.shipment_value_cents.map_or(false, |v| v > *greater_than)
            }
            RuleCondition::Zone { in_zones } => {
                ctx.zone.as_ref().map_or(false, |z| in_zones.contains(z))
            }
            RuleCondition::TimeOfDay { hour_from, hour_to } => {
                ctx.current_hour >= *hour_from && ctx.current_hour <= *hour_to
            }
            RuleCondition::DayOfWeek { days } => {
                days.contains(&ctx.current_day)
            }
            RuleCondition::AttemptCount { lte } => {
                ctx.attempt_count.map_or(false, |a| a <= *lte)
            }
            // Custom DSL expressions require an interpreter — evaluated externally
            RuleCondition::Custom { .. } => true,
            _ => true,
        }
    }
}

// ── Example Built-in Rule: Failed Delivery Automation ────────
// IF delivery_failed AND attempt_count <= 3
// THEN reschedule_delivery(24h) AND notify_customer AND alert_dispatcher
pub fn failed_delivery_rule(tenant_id: Uuid) -> AutomationRule {
    AutomationRule {
        id: Uuid::new_v4(),
        tenant_id,
        name: "Auto-Reschedule on Failed Delivery".into(),
        description: "Automatically reschedule and notify when delivery fails".into(),
        is_active: true,
        trigger: RuleTrigger::DeliveryFailed,
        conditions: vec![
            RuleCondition::AttemptCount { lte: 3 },
        ],
        actions: vec![
            RuleAction::RescheduleDelivery { delay_hours: 24 },
            RuleAction::NotifyCustomer {
                channel: "whatsapp".into(),
                template_id: "delivery_failed_reschedule".into(),
            },
            RuleAction::AlertDispatcher {
                message: "Delivery failed — auto-rescheduled".into(),
                priority: "normal".into(),
            },
            RuleAction::LogAuditEvent { event_type: "delivery.failed.auto_rescheduled".into() },
        ],
        priority: 10,
        created_at: Utc::now(),
    }
}
