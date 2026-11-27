use serde_json::Value;
use uuid::Uuid;

use crate::domain::entities::rule::{
    AutomationRule, RuleAction, RuleContext, RuleTrigger,
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

    /// Trigger a pre-built campaign from the marketing service for a single customer.
    /// The campaign's channel + template are loaded by the marketing service; the
    /// customer's contact details are resolved from the CDP.
    async fn trigger_campaign(
        &self,
        rule_id:     Uuid,
        rule_name:   &str,
        campaign_id: Uuid,
        ctx:         &RuleContext,
    ) -> anyhow::Result<()>;

    /// Assign (or reassign) a driver to a shipment via the dispatch service's
    /// heuristic quick-dispatch — no explicit driver_id, dispatch picks the best
    /// available one.
    async fn assign_new_driver(&self, tenant_id: Uuid, shipment_id: Uuid) -> anyhow::Result<()>;

    /// Update a shipment's status directly (order-intake).
    async fn update_shipment_status(&self, tenant_id: Uuid, shipment_id: Uuid, status: &str) -> anyhow::Result<()>;

    /// Generic outbound webhook call — user-configured URL/method on the rule.
    async fn webhook_call(&self, url: &str, method: &str, ctx: &RuleContext) -> anyhow::Result<()>;
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
    /// `data` is the event's business payload (the envelope's `data`, already unwrapped);
    /// threshold triggers inspect it to decide whether the event is a crossing.
    pub async fn rules_for_topic(&self, tenant_id: Uuid, topic: &str, data: &Value) -> Vec<AutomationRule> {
        let _event_type = topic_to_event_type(topic);
        let rules = self.rules.read().await;
        let mut matching: Vec<AutomationRule> = rules
            .iter()
            .filter(|r| {
                r.is_active
                    && (r.tenant_id == Uuid::nil() || r.tenant_id == tenant_id)
                    && trigger_matches(&r.trigger, topic, data)
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

/// Determines if a rule trigger matches an incoming Kafka event.
///
/// Most triggers match on the topic alone. The threshold triggers
/// (`HubCapacityExceeded`, `PaymentOverdue`, `SlaBreach`) also inspect `data`
/// and fire only on a *crossing*, so a hub that stays above its threshold does
/// not re-fire on every load change. A missing or ill-typed field is a non-match.
pub fn trigger_matches(trigger: &RuleTrigger, topic: &str, data: &Value) -> bool {
    use logisticos_events::topics;

    let int = |key: &str| data.get(key).and_then(Value::as_i64);

    match trigger {
        RuleTrigger::HubCapacityExceeded { threshold_pct } if topic == topics::HUB_LOAD_CHANGED => {
            let (Some(capacity), Some(previous), Some(current)) =
                (int("capacity"), int("previous_load"), int("current_load"))
            else {
                return false;
            };
            // Integer math, widened to i64: no float rounding at the boundary.
            let threshold = i64::from(*threshold_pct) * capacity;
            capacity > 0 && previous * 100 < threshold && current * 100 >= threshold
        }
        RuleTrigger::PaymentOverdue { days } if topic == topics::INVOICE_OVERDUE => {
            let (Some(previous), Some(current)) = (int("previous_days_overdue"), int("days_overdue"))
            else {
                return false;
            };
            let days = i64::from(*days);
            previous < days && current >= days
        }
        RuleTrigger::SlaBreach { sla_type } if topic == topics::SHIPMENT_SLA_BREACHED => data
            .get("sla_type")
            .and_then(Value::as_str)
            .is_some_and(|t| t.eq_ignore_ascii_case(sla_type)),
        _ => matches!(
            (trigger, topic),
            (RuleTrigger::DeliveryFailed,        "logisticos.driver.delivery.failed")
            | (RuleTrigger::DeliveryCompleted,   "logisticos.driver.delivery.completed")
            | (RuleTrigger::ShipmentCreated,     "logisticos.order.shipment.created")
            | (RuleTrigger::ShipmentRescheduled, "logisticos.order.shipment.rescheduled")
            | (RuleTrigger::PickupCompleted,     "logisticos.driver.pickup.completed")
            | (RuleTrigger::CodCollected,        "logisticos.payments.cod.collected")
            | (RuleTrigger::PaymentReceived,     "logisticos.payments.payment.received")
            | (RuleTrigger::DeliveryAttempted { .. }, "logisticos.driver.delivery.attempted")
            | (RuleTrigger::CampaignTriggered { .. }, "logisticos.marketing.campaign.triggered")
        ),
    }
}

fn topic_to_event_type(topic: &str) -> String {
    topic.split('.').skip(1).collect::<Vec<_>>().join(".")
}

/// Build a RuleContext from a Kafka message payload.
/// `customer_segment_ids` is left empty here; callers should call
/// `enrich_segments()` asynchronously before invoking `conditions_met()`.
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
        shipment_value_cents: payload["cod_amount_cents"].as_i64().or_else(|| payload["cod_amount"].as_i64()),
        current_hour: chrono::Local::now().hour() as u8,
        current_day: chrono::Local::now().format("%A").to_string(),
        metadata: payload.clone(),
        customer_segment_ids: Vec::new(),
    }
}

/// Async segment resolver — implemented by bootstrap with an HTTP call to CDP.
#[async_trait::async_trait]
pub trait SegmentChecker: Send + Sync {
    /// Return segment IDs the customer belongs to within the tenant.
    async fn segment_ids_for_customer(
        &self,
        tenant_id: Uuid,
        customer_id: Uuid,
    ) -> anyhow::Result<Vec<Uuid>>;
}

/// Populate `ctx.customer_segment_ids` if any rule in the list uses `CustomerSegment`.
/// Call once per event before `conditions_met()`.
pub async fn enrich_segments(
    ctx: &mut RuleContext,
    rules: &[AutomationRule],
    checker: &dyn SegmentChecker,
) {
    use crate::domain::entities::rule::RuleCondition;

    let needs_segments = rules.iter().any(|r| {
        r.conditions.iter().any(|c| matches!(c, RuleCondition::CustomerSegment { .. }))
    });

    if !needs_segments { return; }

    let Some(customer_id) = ctx.customer_id else { return; };

    match checker.segment_ids_for_customer(ctx.tenant_id, customer_id).await {
        Ok(ids) => { ctx.customer_segment_ids = ids; }
        Err(e)  => {
            tracing::warn!(
                customer_id = %customer_id,
                err = %e,
                "Failed to resolve segment IDs — CustomerSegment conditions will evaluate to false"
            );
        }
    }
}

use chrono::Timelike;

/// An action that could not run for this event, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedAction {
    pub action: String,
    pub reason: String,
}

/// What `execute_actions` actually did: actions that ran, and actions that
/// were skipped because the event lacked what they need or no integration exists.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecutionReport {
    pub ran: Vec<String>,
    pub skipped: Vec<SkippedAction>,
}

impl ExecutionReport {
    /// "skipped" when nothing ran and something was skipped; otherwise "success".
    pub fn outcome(&self) -> &'static str {
        if self.ran.is_empty() && !self.skipped.is_empty() { "skipped" } else { "success" }
    }

    /// Entries for the execution log: ran actions, then `skipped: <action> (<reason>)`.
    pub fn logged_actions(&self) -> Vec<String> {
        self.ran
            .iter()
            .cloned()
            .chain(self.skipped.iter().map(|s| format!("skipped: {} ({})", s.action, s.reason)))
            .collect()
    }
}

/// Execute a rule's actions against the event context.
pub async fn execute_actions(
    rule: &AutomationRule,
    ctx: &RuleContext,
    executor: &dyn ActionExecutor,
) -> anyhow::Result<ExecutionReport> {
    let mut report = ExecutionReport::default();
    for action in &rule.actions {
        let label = format!("{action:?}");
        // Some(reason) when the action cannot run for this event.
        let skip_reason: Option<&str> = match action {
            RuleAction::RescheduleDelivery { delay_hours } => match ctx.shipment_id {
                Some(shipment_id) => {
                    executor.reschedule_delivery(shipment_id, *delay_hours).await?;
                    None
                }
                None => Some("event has no shipment_id"),
            },
            RuleAction::NotifyCustomer { channel, template_id } => {
                match (ctx.customer_id, ctx.shipment_id) {
                    (Some(customer_id), Some(_)) => {
                        executor.notify_customer(
                            ctx.tenant_id,
                            customer_id,
                            channel,
                            template_id,
                            ctx,
                        ).await?;
                        None
                    }
                    _ => Some("event has no customer_id and shipment_id"),
                }
            }
            RuleAction::AlertDispatcher { message, priority } => {
                executor.alert_dispatcher(ctx.tenant_id, message, priority).await?;
                None
            }
            RuleAction::LogAuditEvent { event_type } => {
                tracing::info!(
                    rule_id = %rule.id,
                    tenant_id = %ctx.tenant_id,
                    event_type = %event_type,
                    shipment_id = ?ctx.shipment_id,
                    "Rule action: audit event"
                );
                None
            }
            RuleAction::RunAiDispatch => {
                // Signal to the ai-layer that autonomous dispatch is needed.
                let payload = serde_json::to_vec(&serde_json::json!({
                    "shipment_id": ctx.shipment_id,
                    "tenant_id":   ctx.tenant_id,
                    "trigger":     "rules_engine",
                }))?;
                executor.emit_event("logisticos.ai.dispatch.requested", "", &payload).await?;
                None
            }
            RuleAction::EscalateToSupport { tier } => {
                executor.alert_dispatcher(
                    ctx.tenant_id,
                    &format!("Escalation tier {} required for shipment {:?}", tier, ctx.shipment_id),
                    "high",
                ).await?;
                None
            }
            RuleAction::TriggerCampaign { campaign_id } => {
                executor.trigger_campaign(
                    rule.id,
                    &rule.name,
                    *campaign_id,
                    ctx,
                ).await?;
                None
            }
            RuleAction::AssignNewDriver => match ctx.shipment_id {
                Some(shipment_id) => {
                    executor.assign_new_driver(ctx.tenant_id, shipment_id).await?;
                    None
                }
                None => Some("event has no shipment_id"),
            },
            RuleAction::UpdateShipmentStatus { status } => match ctx.shipment_id {
                Some(shipment_id) => {
                    executor.update_shipment_status(ctx.tenant_id, shipment_id, status).await?;
                    None
                }
                None => Some("event has no shipment_id"),
            },
            RuleAction::WebhookCall { url, method } => {
                executor.webhook_call(url, method, ctx).await?;
                None
            }
            RuleAction::NotifyMerchant { .. } | RuleAction::ApplyDiscount { .. } | RuleAction::CreateRefund { .. } => {
                Some("no downstream integration yet")
            }
        };

        match skip_reason {
            None => report.ran.push(label),
            Some(reason) => {
                tracing::warn!(rule_id = %rule.id, action = %label, reason, "Rule action skipped");
                report.skipped.push(SkippedAction { action: label, reason: reason.to_owned() });
            }
        }
    }
    Ok(report)
}


#[cfg(test)]
mod tests {
    use super::*;
    use logisticos_events::topics;
    use serde_json::json;

    fn hub(threshold_pct: u8, data: Value) -> bool {
        trigger_matches(&RuleTrigger::HubCapacityExceeded { threshold_pct }, topics::HUB_LOAD_CHANGED, &data)
    }

    #[test]
    fn hub_capacity_upward_crossing_only() {
        // 79 -> 80 of 100 at an 80% threshold: crossing.
        assert!(hub(80, json!({"capacity": 100, "previous_load": 79, "current_load": 80})));
        // Below the threshold.
        assert!(!hub(80, json!({"capacity": 100, "previous_load": 50, "current_load": 79})));
        // Already above: no re-fire.
        assert!(!hub(80, json!({"capacity": 100, "previous_load": 85, "current_load": 90})));
        // Downward crossing is not an exceed.
        assert!(!hub(80, json!({"capacity": 100, "previous_load": 90, "current_load": 70})));
        // Zero capacity never matches.
        assert!(!hub(80, json!({"capacity": 0, "previous_load": 0, "current_load": 5})));
    }

    #[test]
    fn hub_capacity_missing_or_ill_typed_field_is_false() {
        assert!(!hub(80, json!({"capacity": 100, "previous_load": 79})));
        assert!(!hub(80, json!({"capacity": "100", "previous_load": 79, "current_load": 80})));
        assert!(!hub(80, json!({})));
    }

    #[test]
    fn hub_capacity_wrong_topic_is_false() {
        let t = RuleTrigger::HubCapacityExceeded { threshold_pct: 80 };
        let d = json!({"capacity": 100, "previous_load": 79, "current_load": 80});
        assert!(!trigger_matches(&t, topics::INVOICE_OVERDUE, &d));
    }

    fn overdue(days: u32, data: Value) -> bool {
        trigger_matches(&RuleTrigger::PaymentOverdue { days }, topics::INVOICE_OVERDUE, &data)
    }

    #[test]
    fn payment_overdue_crossing_only() {
        assert!(overdue(7, json!({"previous_days_overdue": 6, "days_overdue": 7})));
        assert!(!overdue(7, json!({"previous_days_overdue": 5, "days_overdue": 6})));
        assert!(!overdue(7, json!({"previous_days_overdue": 7, "days_overdue": 8})));
        assert!(!overdue(7, json!({"days_overdue": 7})));
        assert!(!overdue(7, json!({"previous_days_overdue": "6", "days_overdue": 7})));
    }

    fn sla(kind: &str, data: Value) -> bool {
        trigger_matches(&RuleTrigger::SlaBreach { sla_type: kind.into() }, topics::SHIPMENT_SLA_BREACHED, &data)
    }

    #[test]
    fn sla_breach_matches_type_case_insensitively() {
        assert!(sla("delivery", json!({"sla_type": "delivery"})));
        assert!(sla("Delivery", json!({"sla_type": "DELIVERY"})));
        assert!(!sla("delivery", json!({"sla_type": "pickup"})));
        assert!(!sla("delivery", json!({})));
        assert!(!sla("delivery", json!({"sla_type": 3})));
    }

    #[tokio::test]
    async fn existing_arm_matches_envelope_data_and_context_reads_data() {
        let tenant = Uuid::new_v4();
        let shipment = Uuid::new_v4();
        let rule = AutomationRule {
            id: Uuid::new_v4(),
            tenant_id: tenant,
            name: "failed".into(),
            description: String::new(),
            is_active: true,
            priority: 1,
            trigger: RuleTrigger::DeliveryFailed,
            conditions: vec![],
            actions: vec![],
            created_at: chrono::Utc::now(),
        };
        let repo = RuleRepository::new(vec![rule]);
        let envelope = json!({
            "tenant_id": tenant, "source": "driver-ops",
            "data": {"shipment_id": shipment, "driver_id": Uuid::new_v4()}
        });
        let (t, data) = crate::bootstrap::decode_event(None, &envelope).expect("decodes");
        let hits = repo.rules_for_topic(t, topics::DELIVERY_FAILED, &data).await;
        assert_eq!(hits.len(), 1);
        let ctx = build_context(t, topics::DELIVERY_FAILED, &data);
        assert_eq!(ctx.shipment_id, Some(shipment));
        assert!(ctx.driver_id.is_some());
    }

    fn rule_with(actions: Vec<RuleAction>) -> AutomationRule {
        AutomationRule {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            name: "t".into(),
            description: String::new(),
            is_active: true,
            priority: 1,
            trigger: RuleTrigger::PaymentOverdue { days: 7 },
            conditions: vec![],
            actions,
            created_at: chrono::Utc::now(),
        }
    }

    #[derive(Default)]
    struct NoopExecutor {
        alerts: std::sync::atomic::AtomicUsize,
    }

    #[async_trait::async_trait]
    impl ActionExecutor for NoopExecutor {
        async fn notify_customer(&self, _: Uuid, _: Uuid, _: &str, _: &str, _: &RuleContext) -> anyhow::Result<()> { Ok(()) }
        async fn reschedule_delivery(&self, _: Uuid, _: u32) -> anyhow::Result<()> { Ok(()) }
        async fn alert_dispatcher(&self, _: Uuid, _: &str, _: &str) -> anyhow::Result<()> {
            self.alerts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
        async fn emit_event(&self, _: &str, _: &str, _: &[u8]) -> anyhow::Result<()> { Ok(()) }
        async fn trigger_campaign(&self, _: Uuid, _: &str, _: Uuid, _: &RuleContext) -> anyhow::Result<()> { Ok(()) }
        async fn assign_new_driver(&self, _: Uuid, _: Uuid) -> anyhow::Result<()> { Ok(()) }
        async fn update_shipment_status(&self, _: Uuid, _: Uuid, _: &str) -> anyhow::Result<()> { Ok(()) }
        async fn webhook_call(&self, _: &str, _: &str, _: &RuleContext) -> anyhow::Result<()> { Ok(()) }
    }

    fn overdue_ctx() -> RuleContext {
        build_context(
            Uuid::new_v4(),
            topics::INVOICE_OVERDUE,
            &json!({"merchant_id": Uuid::new_v4(), "previous_days_overdue": 6, "days_overdue": 7}),
        )
    }

    #[tokio::test]
    async fn notify_customer_without_ids_is_skipped_with_reason() {
        let rule = rule_with(vec![RuleAction::NotifyCustomer { channel: "sms".into(), template_id: "t".into() }]);
        let report = execute_actions(&rule, &overdue_ctx(), &NoopExecutor::default()).await.expect("runs");
        assert!(report.ran.is_empty());
        assert_eq!(report.skipped.len(), 1);
        assert!(report.skipped[0].reason.contains("customer_id"));
        assert!(report.logged_actions()[0].starts_with("skipped: "));
    }

    #[tokio::test]
    async fn all_skipped_execution_is_not_success() {
        let rule = rule_with(vec![
            RuleAction::NotifyMerchant { template_id: "t".into() },
            RuleAction::NotifyCustomer { channel: "sms".into(), template_id: "t".into() },
        ]);
        let report = execute_actions(&rule, &overdue_ctx(), &NoopExecutor::default()).await.expect("runs");
        assert_eq!(report.skipped.len(), 2);
        assert_eq!(report.outcome(), "skipped");
    }

    #[tokio::test]
    async fn mixed_ran_and_skipped_is_success_and_lists_both() {
        let rule = rule_with(vec![
            RuleAction::AlertDispatcher { message: "m".into(), priority: "high".into() },
            RuleAction::NotifyMerchant { template_id: "t".into() },
        ]);
        let exec = NoopExecutor::default();
        let report = execute_actions(&rule, &overdue_ctx(), &exec).await.expect("runs");
        assert_eq!(exec.alerts.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(report.outcome(), "success");
        let logged = report.logged_actions();
        assert_eq!(logged.len(), 2);
        assert!(logged[0].starts_with("AlertDispatcher"));
        assert!(logged[1].starts_with("skipped: NotifyMerchant"));
    }

    #[test]
    fn build_context_reads_cod_amount_cents_and_falls_back() {
        let t = Uuid::new_v4();
        let ctx = build_context(t, topics::DELIVERY_FAILED, &json!({"cod_amount_cents": 12_500}));
        assert_eq!(ctx.shipment_value_cents, Some(12_500));
        let ctx = build_context(t, topics::DELIVERY_FAILED, &json!({"cod_amount": 900}));
        assert_eq!(ctx.shipment_value_cents, Some(900));
        let ctx = build_context(t, topics::DELIVERY_FAILED, &json!({"cod_amount_cents": 1, "cod_amount": 2}));
        assert_eq!(ctx.shipment_value_cents, Some(1));
    }
}
