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
    ShipmentRescheduled,
    ShipmentDelayed { minutes_late: u32 },
    PickupCompleted,
    CodCollected,
    PaymentReceived,
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
    /// Pre-resolved segment IDs for the customer — populated asynchronously before
    /// `conditions_met()` is called so the synchronous `evaluate()` can check membership.
    pub customer_segment_ids: Vec<Uuid>,
}

impl RuleCondition {
    pub fn evaluate(&self, ctx: &RuleContext) -> bool {
        match self {
            RuleCondition::ServiceType { equals } => {
                ctx.service_type.as_deref() == Some(equals.as_str())
            }
            RuleCondition::ShipmentValue { greater_than } => {
                ctx.shipment_value_cents.is_some_and(|v| v > *greater_than)
            }
            RuleCondition::Zone { in_zones } => {
                ctx.zone.as_ref().is_some_and(|z| in_zones.contains(z))
            }
            RuleCondition::TimeOfDay { hour_from, hour_to } => {
                ctx.current_hour >= *hour_from && ctx.current_hour <= *hour_to
            }
            RuleCondition::DayOfWeek { days } => {
                days.contains(&ctx.current_day)
            }
            RuleCondition::AttemptCount { lte } => {
                ctx.attempt_count.is_some_and(|a| a <= *lte)
            }
            RuleCondition::CustomerSegment { segment_id } => {
                ctx.customer_segment_ids.contains(segment_id)
            }
            RuleCondition::Custom { expression } => evaluate_custom_expression(expression, ctx),
            _ => true,
        }
    }
}

/// Evaluates a DSL expression against a RuleContext using evalexpr.
///
/// Supported variables (map directly to RuleContext fields):
///   service_type (string), zone (string), current_hour (int), current_day (string),
///   attempt_count (int), shipment_value_cents (int), has_customer (bool),
///   has_shipment (bool), has_driver (bool), has_merchant (bool).
///
/// Supported operators: ==, !=, >, >=, <, <=, &&, ||, !
///
/// Returns false (fail-safe) on parse/type errors — bad expressions must not
/// halt rule evaluation for other conditions.
fn evaluate_custom_expression(expression: &str, ctx: &RuleContext) -> bool {
    use evalexpr::{ContextWithMutableVariables, HashMapContext, Value, eval_boolean_with_context};

    let mut evalctx = HashMapContext::new();

    let set = |c: &mut HashMapContext, k: &str, v: Value| {
        c.set_value(k.into(), v).ok();
    };

    set(&mut evalctx, "service_type",
        Value::String(ctx.service_type.clone().unwrap_or_default()));
    set(&mut evalctx, "zone",
        Value::String(ctx.zone.clone().unwrap_or_default()));
    set(&mut evalctx, "current_day",
        Value::String(ctx.current_day.clone()));
    set(&mut evalctx, "event_type",
        Value::String(ctx.event_type.clone()));
    set(&mut evalctx, "current_hour",
        Value::Int(ctx.current_hour as i64));
    set(&mut evalctx, "attempt_count",
        Value::Int(ctx.attempt_count.unwrap_or(0) as i64));
    set(&mut evalctx, "shipment_value_cents",
        Value::Int(ctx.shipment_value_cents.unwrap_or(0)));
    set(&mut evalctx, "has_customer",
        Value::Boolean(ctx.customer_id.is_some()));
    set(&mut evalctx, "has_shipment",
        Value::Boolean(ctx.shipment_id.is_some()));
    set(&mut evalctx, "has_driver",
        Value::Boolean(ctx.driver_id.is_some()));
    set(&mut evalctx, "has_merchant",
        Value::Boolean(ctx.merchant_id.is_some()));

    // Populate any additional scalar fields from the raw metadata JSON so
    // operators can reference custom payload keys not modelled in RuleContext.
    if let Some(obj) = ctx.metadata.as_object() {
        for (k, v) in obj {
            let eval_val = match v {
                serde_json::Value::Bool(b)   => Some(Value::Boolean(*b)),
                serde_json::Value::Number(n) => n.as_i64().map(Value::Int)
                    .or_else(|| n.as_f64().map(Value::Float)),
                serde_json::Value::String(s) => Some(Value::String(s.clone())),
                _ => None,
            };
            if let Some(val) = eval_val {
                set(&mut evalctx, k, val);
            }
        }
    }

    match eval_boolean_with_context(expression, &evalctx) {
        Ok(result) => result,
        Err(e) => {
            tracing::warn!(
                expression = %expression,
                err = %e,
                "Custom DSL condition evaluation failed — treating as false"
            );
            false
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
