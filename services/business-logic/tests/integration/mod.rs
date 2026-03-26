// Integration tests for the Business Logic / Rules Engine.
//
// Strategy: tests target the RuleRepository and evaluation logic directly
// (no HTTP router for this service — the API is Kafka-driven).
// A MockActionExecutor captures calls so we can assert side-effects.
// Tests are async via tokio.

use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use async_trait::async_trait;
use uuid::Uuid;

use logisticos_business_logic::{
    application::services::{
        ActionExecutor, RuleRepository, build_context, execute_actions,
    },
    domain::entities::rule::{
        AutomationRule, RuleAction, RuleCondition, RuleContext, RuleTrigger,
    },
};

// ─────────────────────────────────────────────────────────────────────────────
// Mock executor — records all side-effect calls
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Default, Clone)]
struct MockExecutor {
    notifications: Arc<Mutex<Vec<(Uuid, Uuid, String, String)>>>,  // (tenant, customer, channel, template)
    reschedules:   Arc<Mutex<Vec<(Uuid, u32)>>>,                   // (shipment_id, delay_hours)
    alerts:        Arc<Mutex<Vec<(Uuid, String, String)>>>,        // (tenant, message, priority)
    events:        Arc<Mutex<Vec<(String, String)>>>,              // (topic, key)
}

#[async_trait]
impl ActionExecutor for MockExecutor {
    async fn notify_customer(
        &self, tenant_id: Uuid, customer_id: Uuid,
        channel: &str, template_id: &str, _ctx: &RuleContext,
    ) -> anyhow::Result<()> {
        self.notifications.lock().unwrap()
            .push((tenant_id, customer_id, channel.into(), template_id.into()));
        Ok(())
    }

    async fn reschedule_delivery(&self, shipment_id: Uuid, delay_hours: u32) -> anyhow::Result<()> {
        self.reschedules.lock().unwrap().push((shipment_id, delay_hours));
        Ok(())
    }

    async fn alert_dispatcher(&self, tenant_id: Uuid, message: &str, priority: &str) -> anyhow::Result<()> {
        self.alerts.lock().unwrap()
            .push((tenant_id, message.into(), priority.into()));
        Ok(())
    }

    async fn emit_event(&self, topic: &str, key: &str, _payload: &[u8]) -> anyhow::Result<()> {
        self.events.lock().unwrap().push((topic.into(), key.into()));
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn tenant_id() -> Uuid { Uuid::new_v4() }
fn shipment_id() -> Uuid { Uuid::new_v4() }
fn customer_id() -> Uuid { Uuid::new_v4() }

fn make_rule(
    tenant_id: Uuid,
    trigger: RuleTrigger,
    conditions: Vec<RuleCondition>,
    actions: Vec<RuleAction>,
    priority: u32,
) -> AutomationRule {
    AutomationRule {
        id: Uuid::new_v4(),
        tenant_id,
        name: "Test Rule".into(),
        description: "Test".into(),
        is_active: true,
        trigger,
        conditions,
        actions,
        priority,
        created_at: chrono::Utc::now(),
    }
}

fn simple_ctx(tenant_id: Uuid) -> RuleContext {
    RuleContext {
        tenant_id,
        event_type: "driver.delivery.failed".into(),
        shipment_id: Some(shipment_id()),
        customer_id: Some(customer_id()),
        merchant_id: None,
        driver_id: None,
        service_type: Some("standard".into()),
        zone: Some("NCR".into()),
        attempt_count: Some(1),
        shipment_value_cents: Some(50000),
        current_hour: 14,
        current_day: "Tuesday".into(),
        metadata: serde_json::json!({}),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// RuleRepository: basic operations
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn rule_repo_empty_initially() {
    let repo = RuleRepository::new(vec![]);
    let tid = tenant_id();
    let rules = repo.rules_for_topic(tid, "logisticos.driver.delivery.failed").await;
    assert!(rules.is_empty());
}

#[tokio::test]
async fn rule_repo_returns_matching_rule() {
    let tid = tenant_id();
    let rule = make_rule(tid, RuleTrigger::DeliveryFailed, vec![], vec![], 10);
    let repo = RuleRepository::new(vec![rule.clone()]);
    let rules = repo.rules_for_topic(tid, "logisticos.driver.delivery.failed").await;
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].id, rule.id);
}

#[tokio::test]
async fn rule_repo_excludes_inactive_rules() {
    let tid = tenant_id();
    let mut rule = make_rule(tid, RuleTrigger::DeliveryFailed, vec![], vec![], 10);
    rule.is_active = false;
    let repo = RuleRepository::new(vec![rule]);
    let rules = repo.rules_for_topic(tid, "logisticos.driver.delivery.failed").await;
    assert!(rules.is_empty());
}

#[tokio::test]
async fn rule_repo_sorts_by_priority_asc() {
    let tid = tenant_id();
    let r1 = make_rule(tid, RuleTrigger::DeliveryFailed, vec![], vec![], 20);
    let r2 = make_rule(tid, RuleTrigger::DeliveryFailed, vec![], vec![], 5);
    let r3 = make_rule(tid, RuleTrigger::DeliveryFailed, vec![], vec![], 15);
    let repo = RuleRepository::new(vec![r1.clone(), r2.clone(), r3.clone()]);
    let rules = repo.rules_for_topic(tid, "logisticos.driver.delivery.failed").await;
    assert_eq!(rules.len(), 3);
    assert_eq!(rules[0].id, r2.id); // priority=5 first
    assert_eq!(rules[1].id, r3.id); // priority=15
    assert_eq!(rules[2].id, r1.id); // priority=20
}

#[tokio::test]
async fn rule_repo_excludes_different_tenant_rules() {
    let t1 = tenant_id(); let t2 = tenant_id();
    let rule_t1 = make_rule(t1, RuleTrigger::DeliveryFailed, vec![], vec![], 10);
    let rule_t2 = make_rule(t2, RuleTrigger::DeliveryFailed, vec![], vec![], 10);
    let repo = RuleRepository::new(vec![rule_t1.clone(), rule_t2]);
    // Querying for t1 should only return t1's rule
    let rules = repo.rules_for_topic(t1, "logisticos.driver.delivery.failed").await;
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].id, rule_t1.id);
}

#[tokio::test]
async fn rule_repo_platform_rules_match_all_tenants() {
    let t1 = tenant_id(); let t2 = tenant_id();
    // Platform rule: tenant_id = Uuid::nil()
    let platform_rule = make_rule(Uuid::nil(), RuleTrigger::DeliveryFailed, vec![], vec![], 1);
    let repo = RuleRepository::new(vec![platform_rule.clone()]);
    // Both t1 and t2 see the platform rule
    let rules_t1 = repo.rules_for_topic(t1, "logisticos.driver.delivery.failed").await;
    let rules_t2 = repo.rules_for_topic(t2, "logisticos.driver.delivery.failed").await;
    assert_eq!(rules_t1.len(), 1);
    assert_eq!(rules_t2.len(), 1);
}

#[tokio::test]
async fn rule_repo_reload_replaces_rules() {
    let tid = tenant_id();
    let r1 = make_rule(tid, RuleTrigger::DeliveryFailed, vec![], vec![], 10);
    let repo = RuleRepository::new(vec![r1]);
    assert_eq!(repo.rules_for_topic(tid, "logisticos.driver.delivery.failed").await.len(), 1);

    // Reload with new rules
    let r2 = make_rule(tid, RuleTrigger::DeliveryFailed, vec![], vec![], 5);
    let r3 = make_rule(tid, RuleTrigger::DeliveryFailed, vec![], vec![], 15);
    repo.reload(vec![r2, r3]).await;
    assert_eq!(repo.rules_for_topic(tid, "logisticos.driver.delivery.failed").await.len(), 2);
}

// ─────────────────────────────────────────────────────────────────────────────
// Trigger matching
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn delivery_completed_matches_correct_topic() {
    let tid = tenant_id();
    let rule = make_rule(tid, RuleTrigger::DeliveryCompleted, vec![], vec![], 10);
    let repo = RuleRepository::new(vec![rule.clone()]);
    // Wrong topic
    assert!(repo.rules_for_topic(tid, "logisticos.driver.delivery.failed").await.is_empty());
    // Correct topic
    assert_eq!(repo.rules_for_topic(tid, "logisticos.driver.delivery.completed").await.len(), 1);
}

#[tokio::test]
async fn shipment_created_matches_correct_topic() {
    let tid = tenant_id();
    let rule = make_rule(tid, RuleTrigger::ShipmentCreated, vec![], vec![], 10);
    let repo = RuleRepository::new(vec![rule]);
    let rules = repo.rules_for_topic(tid, "logisticos.order.shipment.created").await;
    assert_eq!(rules.len(), 1);
}

#[tokio::test]
async fn delivery_attempted_matches_correct_topic() {
    let tid = tenant_id();
    let rule = make_rule(tid, RuleTrigger::DeliveryAttempted { attempts: 2 }, vec![], vec![], 10);
    let repo = RuleRepository::new(vec![rule]);
    let rules = repo.rules_for_topic(tid, "logisticos.driver.delivery.attempted").await;
    assert_eq!(rules.len(), 1);
}

// ─────────────────────────────────────────────────────────────────────────────
// Condition evaluation
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn conditions_met_true_when_no_conditions() {
    let tid = tenant_id();
    let rule = make_rule(tid, RuleTrigger::DeliveryFailed, vec![], vec![], 10);
    let ctx = simple_ctx(tid);
    assert!(rule.conditions_met(&ctx));
}

#[tokio::test]
async fn condition_service_type_passes() {
    let tid = tenant_id();
    let rule = make_rule(tid, RuleTrigger::DeliveryFailed,
        vec![RuleCondition::ServiceType { equals: "standard".into() }], vec![], 10);
    let ctx = simple_ctx(tid); // service_type = standard
    assert!(rule.conditions_met(&ctx));
}

#[tokio::test]
async fn condition_service_type_fails_mismatch() {
    let tid = tenant_id();
    let rule = make_rule(tid, RuleTrigger::DeliveryFailed,
        vec![RuleCondition::ServiceType { equals: "same_day".into() }], vec![], 10);
    let ctx = simple_ctx(tid); // service_type = standard
    assert!(!rule.conditions_met(&ctx));
}

#[tokio::test]
async fn condition_zone_passes_when_in_list() {
    let tid = tenant_id();
    let rule = make_rule(tid, RuleTrigger::DeliveryFailed,
        vec![RuleCondition::Zone { in_zones: vec!["NCR".into(), "CEBU".into()] }], vec![], 10);
    let ctx = simple_ctx(tid); // zone = NCR
    assert!(rule.conditions_met(&ctx));
}

#[tokio::test]
async fn condition_zone_fails_when_not_in_list() {
    let tid = tenant_id();
    let rule = make_rule(tid, RuleTrigger::DeliveryFailed,
        vec![RuleCondition::Zone { in_zones: vec!["DAVAO".into()] }], vec![], 10);
    let ctx = simple_ctx(tid); // zone = NCR
    assert!(!rule.conditions_met(&ctx));
}

#[tokio::test]
async fn condition_attempt_count_lte_passes() {
    let tid = tenant_id();
    let rule = make_rule(tid, RuleTrigger::DeliveryFailed,
        vec![RuleCondition::AttemptCount { lte: 3 }], vec![], 10);
    let mut ctx = simple_ctx(tid);
    ctx.attempt_count = Some(2);
    assert!(rule.conditions_met(&ctx));
}

#[tokio::test]
async fn condition_attempt_count_lte_fails_when_exceeded() {
    let tid = tenant_id();
    let rule = make_rule(tid, RuleTrigger::DeliveryFailed,
        vec![RuleCondition::AttemptCount { lte: 3 }], vec![], 10);
    let mut ctx = simple_ctx(tid);
    ctx.attempt_count = Some(4);
    assert!(!rule.conditions_met(&ctx));
}

#[tokio::test]
async fn condition_attempt_count_lte_exact_boundary_passes() {
    let tid = tenant_id();
    let rule = make_rule(tid, RuleTrigger::DeliveryFailed,
        vec![RuleCondition::AttemptCount { lte: 3 }], vec![], 10);
    let mut ctx = simple_ctx(tid);
    ctx.attempt_count = Some(3); // exactly lte=3
    assert!(rule.conditions_met(&ctx));
}

#[tokio::test]
async fn condition_shipment_value_passes() {
    let tid = tenant_id();
    let rule = make_rule(tid, RuleTrigger::DeliveryFailed,
        vec![RuleCondition::ShipmentValue { greater_than: 10000 }], vec![], 10);
    let mut ctx = simple_ctx(tid);
    ctx.shipment_value_cents = Some(50000);
    assert!(rule.conditions_met(&ctx));
}

#[tokio::test]
async fn condition_shipment_value_fails() {
    let tid = tenant_id();
    let rule = make_rule(tid, RuleTrigger::DeliveryFailed,
        vec![RuleCondition::ShipmentValue { greater_than: 100000 }], vec![], 10);
    let mut ctx = simple_ctx(tid);
    ctx.shipment_value_cents = Some(50000);
    assert!(!rule.conditions_met(&ctx));
}

#[tokio::test]
async fn multiple_conditions_all_must_pass() {
    let tid = tenant_id();
    let rule = make_rule(tid, RuleTrigger::DeliveryFailed,
        vec![
            RuleCondition::ServiceType { equals: "standard".into() },
            RuleCondition::Zone { in_zones: vec!["NCR".into()] },
            RuleCondition::AttemptCount { lte: 3 },
        ], vec![], 10);
    let ctx = simple_ctx(tid);
    assert!(rule.conditions_met(&ctx));

    // Fail one condition — zone mismatch
    let mut ctx2 = ctx.clone();
    ctx2.zone = Some("DAVAO".into());
    assert!(!rule.conditions_met(&ctx2));
}

// ─────────────────────────────────────────────────────────────────────────────
// Action execution
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn execute_notify_customer_action() {
    let tid = tenant_id();
    let cid = customer_id();
    let executor = MockExecutor::default();
    let rule = make_rule(tid, RuleTrigger::DeliveryFailed, vec![],
        vec![RuleAction::NotifyCustomer {
            channel: "whatsapp".into(), template_id: "tmpl_delivery_failed".into()
        }], 10);
    let mut ctx = simple_ctx(tid);
    ctx.customer_id = Some(cid);

    execute_actions(&rule, &ctx, &executor).await.unwrap();

    let notifs = executor.notifications.lock().unwrap();
    assert_eq!(notifs.len(), 1);
    assert_eq!(notifs[0].1, cid);
    assert_eq!(notifs[0].2, "whatsapp");
    assert_eq!(notifs[0].3, "tmpl_delivery_failed");
}

#[tokio::test]
async fn execute_reschedule_delivery_action() {
    let tid = tenant_id();
    let sid = shipment_id();
    let executor = MockExecutor::default();
    let rule = make_rule(tid, RuleTrigger::DeliveryFailed, vec![],
        vec![RuleAction::RescheduleDelivery { delay_hours: 24 }], 10);
    let mut ctx = simple_ctx(tid);
    ctx.shipment_id = Some(sid);

    execute_actions(&rule, &ctx, &executor).await.unwrap();

    let reschedules = executor.reschedules.lock().unwrap();
    assert_eq!(reschedules.len(), 1);
    assert_eq!(reschedules[0].0, sid);
    assert_eq!(reschedules[0].1, 24);
}

#[tokio::test]
async fn execute_alert_dispatcher_action() {
    let tid = tenant_id();
    let executor = MockExecutor::default();
    let rule = make_rule(tid, RuleTrigger::DeliveryFailed, vec![],
        vec![RuleAction::AlertDispatcher {
            message: "Delivery failed".into(), priority: "high".into()
        }], 10);
    let ctx = simple_ctx(tid);

    execute_actions(&rule, &ctx, &executor).await.unwrap();

    let alerts = executor.alerts.lock().unwrap();
    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0].2, "high");
}

#[tokio::test]
async fn execute_escalate_to_support_action() {
    let tid = tenant_id();
    let executor = MockExecutor::default();
    let rule = make_rule(tid, RuleTrigger::DeliveryFailed, vec![],
        vec![RuleAction::EscalateToSupport { tier: 2 }], 10);
    let ctx = simple_ctx(tid);

    execute_actions(&rule, &ctx, &executor).await.unwrap();

    // EscalateToSupport delegates to alert_dispatcher with priority="high"
    let alerts = executor.alerts.lock().unwrap();
    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0].2, "high");
}

#[tokio::test]
async fn execute_run_ai_dispatch_action() {
    let tid = tenant_id();
    let executor = MockExecutor::default();
    let rule = make_rule(tid, RuleTrigger::DeliveryFailed, vec![],
        vec![RuleAction::RunAiDispatch], 10);
    let ctx = simple_ctx(tid);

    execute_actions(&rule, &ctx, &executor).await.unwrap();

    let events = executor.events.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, "logisticos.ai.dispatch.requested");
}

#[tokio::test]
async fn execute_multiple_actions_in_sequence() {
    let tid = tenant_id();
    let executor = MockExecutor::default();
    let rule = make_rule(tid, RuleTrigger::DeliveryFailed, vec![],
        vec![
            RuleAction::AlertDispatcher { message: "Alert".into(), priority: "medium".into() },
            RuleAction::RescheduleDelivery { delay_hours: 4 },
        ], 10);
    let ctx = simple_ctx(tid);

    execute_actions(&rule, &ctx, &executor).await.unwrap();

    assert_eq!(executor.alerts.lock().unwrap().len(), 1);
    assert_eq!(executor.reschedules.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn execute_actions_no_notifications_when_no_customer() {
    let tid = tenant_id();
    let executor = MockExecutor::default();
    let rule = make_rule(tid, RuleTrigger::DeliveryFailed, vec![],
        vec![RuleAction::NotifyCustomer {
            channel: "sms".into(), template_id: "t1".into()
        }], 10);
    // ctx has no customer_id
    let mut ctx = simple_ctx(tid);
    ctx.customer_id = None;

    execute_actions(&rule, &ctx, &executor).await.unwrap();

    // Should not fire without a customer_id
    assert!(executor.notifications.lock().unwrap().is_empty());
}

// ─────────────────────────────────────────────────────────────────────────────
// build_context
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn build_context_extracts_fields() {
    let tid = Uuid::new_v4();
    let sid = Uuid::new_v4();
    let cid = Uuid::new_v4();
    let payload = serde_json::json!({
        "shipment_id": sid.to_string(),
        "customer_id": cid.to_string(),
        "service_type": "same_day",
        "zone": "CEBU",
        "attempt_number": 2,
        "cod_amount": 150000
    });
    let ctx = build_context(tid, "logisticos.driver.delivery.failed", &payload);
    assert_eq!(ctx.tenant_id, tid);
    assert_eq!(ctx.shipment_id, Some(sid));
    assert_eq!(ctx.customer_id, Some(cid));
    assert_eq!(ctx.service_type, Some("same_day".into()));
    assert_eq!(ctx.zone, Some("CEBU".into()));
    assert_eq!(ctx.attempt_count, Some(2));
    assert_eq!(ctx.shipment_value_cents, Some(150000));
}

#[tokio::test]
async fn build_context_handles_missing_optional_fields() {
    let tid = Uuid::new_v4();
    let ctx = build_context(tid, "logisticos.order.shipment.created", &serde_json::json!({}));
    assert_eq!(ctx.tenant_id, tid);
    assert!(ctx.shipment_id.is_none());
    assert!(ctx.customer_id.is_none());
    assert!(ctx.service_type.is_none());
    assert!(ctx.zone.is_none());
    assert!(ctx.attempt_count.is_none());
    assert!(ctx.shipment_value_cents.is_none());
}

#[tokio::test]
async fn build_context_event_type_derived_from_topic() {
    let tid = Uuid::new_v4();
    let ctx = build_context(tid, "logisticos.driver.delivery.failed", &serde_json::json!({}));
    assert_eq!(ctx.event_type, "driver.delivery.failed");
}

// ─────────────────────────────────────────────────────────────────────────────
// End-to-end: repo → filter → conditions → execute
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn end_to_end_delivery_failed_fires_notification() {
    let tid = tenant_id();
    let cid = customer_id();
    let executor = MockExecutor::default();

    let rule = make_rule(tid, RuleTrigger::DeliveryFailed,
        vec![RuleCondition::AttemptCount { lte: 3 }],
        vec![RuleAction::NotifyCustomer {
            channel: "whatsapp".into(), template_id: "failed_tmpl".into()
        }], 10);
    let repo = RuleRepository::new(vec![rule]);

    let payload = serde_json::json!({
        "shipment_id": Uuid::new_v4().to_string(),
        "customer_id": cid.to_string(),
        "attempt_number": 1
    });
    let ctx = build_context(tid, "logisticos.driver.delivery.failed", &payload);

    let rules = repo.rules_for_topic(tid, "logisticos.driver.delivery.failed").await;
    assert_eq!(rules.len(), 1);

    for rule in &rules {
        if rule.conditions_met(&ctx) {
            execute_actions(rule, &ctx, &executor).await.unwrap();
        }
    }

    assert_eq!(executor.notifications.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn end_to_end_condition_failure_prevents_execution() {
    let tid = tenant_id();
    let executor = MockExecutor::default();

    // Rule fires only when attempt <= 1
    let rule = make_rule(tid, RuleTrigger::DeliveryFailed,
        vec![RuleCondition::AttemptCount { lte: 1 }],
        vec![RuleAction::AlertDispatcher { message: "First fail".into(), priority: "low".into() }],
        10);
    let repo = RuleRepository::new(vec![rule]);

    // Event has attempt_number=3 — condition fails
    let payload = serde_json::json!({ "attempt_number": 3 });
    let ctx = build_context(tid, "logisticos.driver.delivery.failed", &payload);
    let rules = repo.rules_for_topic(tid, "logisticos.driver.delivery.failed").await;

    for rule in &rules {
        if rule.conditions_met(&ctx) {
            execute_actions(rule, &ctx, &executor).await.unwrap();
        }
    }

    assert!(executor.alerts.lock().unwrap().is_empty());
}

#[tokio::test]
async fn end_to_end_multiple_rules_fire_in_priority_order() {
    let tid = tenant_id();
    let executor = MockExecutor::default();

    let r1 = make_rule(tid, RuleTrigger::DeliveryFailed, vec![],
        vec![RuleAction::AlertDispatcher { message: "Priority 20".into(), priority: "low".into() }], 20);
    let r2 = make_rule(tid, RuleTrigger::DeliveryFailed, vec![],
        vec![RuleAction::AlertDispatcher { message: "Priority 5".into(), priority: "high".into() }], 5);

    let repo = RuleRepository::new(vec![r1, r2]);
    let ctx = simple_ctx(tid);
    let rules = repo.rules_for_topic(tid, "logisticos.driver.delivery.failed").await;

    assert_eq!(rules[0].priority, 5);  // lower priority number = executed first
    assert_eq!(rules[1].priority, 20);

    for rule in &rules {
        if rule.conditions_met(&ctx) {
            execute_actions(rule, &ctx, &executor).await.unwrap();
        }
    }
    assert_eq!(executor.alerts.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn end_to_end_inactive_rules_not_fired() {
    let tid = tenant_id();
    let executor = MockExecutor::default();
    let mut rule = make_rule(tid, RuleTrigger::DeliveryFailed, vec![],
        vec![RuleAction::AlertDispatcher { message: "Test".into(), priority: "low".into() }], 10);
    rule.is_active = false;

    let repo = RuleRepository::new(vec![rule]);
    let ctx = simple_ctx(tid);
    let rules = repo.rules_for_topic(tid, "logisticos.driver.delivery.failed").await;
    // Empty — inactive rules excluded at repo level
    assert!(rules.is_empty());
    for rule in &rules {
        execute_actions(rule, &ctx, &executor).await.unwrap();
    }
    assert!(executor.alerts.lock().unwrap().is_empty());
}
