/// Rules Engine execution service.
///
/// On each domain event:
///   1. Load active rules for the tenant + topic from the repository
///   2. For each rule (ordered by priority), evaluate conditions against event payload
///   3. If conditions pass, execute the action
///   4. Log the rule fire to the audit log
use std::sync::Arc;
use chrono::Utc;
use rdkafka::message::BorrowedMessage;
use rdkafka::Message;
use serde_json::Value;
use uuid::Uuid;

use crate::domain::entities::rule::{
    AutomationRule, RuleAction, RuleCondition, RuleContext, RuleTrigger,
};

/// External services the rules engine can invoke.
#[async_trait::async_trait]
pub trait ActionExecutor: Send + Sync {
    /// Notify customer via engagement service.
    async fn notify_customer(
        &self,
        tenant_id: Uuid,
        customer_id: Uuid,
        channel: &str,
        template_id: &str,
        ctx: &RuleContext,
    ) -> anyhow::Result<()>;

    /// Re-schedule a shipment delivery.
    async fn reschedule_delivery(&self, shipment_id: Uuid, delay_hours: u32) -> anyhow::Result<()>;

    /// Escalate to support/operations team.
    async fn alert_dispatcher(&self, tenant_id: Uuid, message: &str, priority: &str) -> anyhow::Result<()>;

    /// Publish a custom Kafka event.
    async fn emit_event(&self, topic: &str, key: &str, payload: &[u8]) -> anyhow::Result<()>;
}

/// In-memory rule store — production loads from PostgreSQL.
/// Loaded at startup and refreshed on rule change events.
pub struct RuleRepository {
    /// Platform-level rules (tenant_id = None) + tenant-specific rules.
    /// Protected by a read-write lock for hot-reload without restart.
    rules: tokio::sync::RwLock<Vec<AutomationRule>>,
}

impl RuleRepository {
    pub fn new(initial_rules: Vec<AutomationRule>) -> Self {
        Self { rules: tokio::sync::RwLock::new(initial_rules) }
    }

    /// Get active rules matching a Kafka topic, ordered by priority (lower = higher priority).
    pub async fn rules_for_topic(&self, tenant_id: Uuid, topic: &str) -> Vec<AutomationRule> {
        let event_type = topic_to_event_type(topic);
        let rules = self.rules.read().await;
        let mut matching: Vec<AutomationRule> = rules
            .iter()
            .filter(|r| {
                r.is_active
                    && (r.tenant_id == Uuid::nil() || r.tenant_id == tenant_id)
                    && trigger_matches_topic(&r.trigger, topic)
            })
            .cloned()
            .collect();
        matching.sort_by_key(|r| r.priority);
        matching
    }

    pub async fn reload(&self, rules: Vec<AutomationRule>) {
        let mut store = self.rules.write().await;
        *store = rules;
        tracing::info!(count = store.len(), "Rules engine reloaded");
    }
}

/// Determines if a rule trigger matches an incoming Kafka topic.
fn trigger_matches_topic(trigger: &RuleTrigger, topic: &str) -> bool {
    matches!(
        (trigger, topic),
        (RuleTrigger::DeliveryFailed, "logisticos.driver.delivery.failed")
        | (RuleTrigger::DeliveryCompleted, "logisticos.driver.delivery.completed")
        | (RuleTrigger::ShipmentCreated, "logisticos.order.shipment.created")
        | (RuleTrigger::DeliveryAttempted { .. }, "logisticos.driver.delivery.attempted")
        | (RuleTrigger::CampaignTriggered { .. }, "logisticos.marketing.campaign.triggered")
    )
}

fn topic_to_event_type(topic: &str) -> String {
    topic.split('.').skip(1).collect::<Vec<_>>().join(".")
}

/// Build a RuleContext from a Kafka message payload.
pub fn build_context(tenant_id: Uuid, topic: &str, payload: &Value) -> RuleContext {
    RuleContext {
        tenant_id,
        event_type: topic_to_event_type(topic),
        shipment_id:  payload["shipment_id"].as_str().and_then(|s| s.parse().ok()),
        customer_id:  payload["customer_id"].as_str().and_then(|s| s.parse().ok()),
        merchant_id:  payload["merchant_id"].as_str().and_then(|s| s.parse().ok()),
        driver_id:    payload["driver_id"].as_str().and_then(|s| s.parse().ok()),
        service_type: payload["service_type"].as_str().map(str::to_owned),
        zone:         payload["zone"].as_str().map(str::to_owned),
        attempt_count: payload["attempt_number"].as_u64().map(|n| n as u32),
        shipment_value_cents: payload["cod_amount"].as_i64(),
        current_hour: chrono::Local::now().hour() as u8,
        current_day: chrono::Local::now().format("%A").to_string(),
        metadata: payload.clone(),
    }
}

use chrono::Timelike;

/// Execute a rule's actions against the event context.
pub async fn execute_actions(
    rule: &AutomationRule,
    ctx: &RuleContext,
    executor: &dyn ActionExecutor,
) -> anyhow::Result<()> {
    for action in &rule.actions {
        match action {
            RuleAction::RescheduleDelivery { delay_hours } => {
                if let Some(shipment_id) = ctx.shipment_id {
                    executor.reschedule_delivery(shipment_id, *delay_hours).await?;
                }
            }
            RuleAction::NotifyCustomer { channel, template_id } => {
                if let (Some(customer_id), Some(_)) = (ctx.customer_id, ctx.shipment_id) {
                    executor.notify_customer(
                        ctx.tenant_id,
                        customer_id,
                        channel,
                        template_id,
                        ctx,
                    ).await?;
                }
            }
            RuleAction::AlertDispatcher { message, priority } => {
                executor.alert_dispatcher(ctx.tenant_id, message, priority).await?;
            }
            RuleAction::LogAuditEvent { event_type } => {
                tracing::info!(
                    rule_id = %rule.id,
                    tenant_id = %ctx.tenant_id,
                    event_type = %event_type,
                    shipment_id = ?ctx.shipment_id,
                    "Rule action: audit event"
                );
            }
            RuleAction::RunAiDispatch => {
                // Signal to the ai-layer that autonomous dispatch is needed.
                let payload = serde_json::to_vec(&serde_json::json!({
                    "shipment_id": ctx.shipment_id,
                    "tenant_id":   ctx.tenant_id,
                    "trigger":     "rules_engine",
                }))?;
                executor.emit_event("logisticos.ai.dispatch.requested", "", &payload).await?;
            }
            RuleAction::EscalateToSupport { tier } => {
                executor.alert_dispatcher(
                    ctx.tenant_id,
                    &format!("Escalation tier {} required for shipment {:?}", tier, ctx.shipment_id),
                    "high",
                ).await?;
            }
            _ => {
                tracing::debug!(action = ?action, "Rule action not yet implemented — logged only");
            }
        }
    }
    Ok(())
}
