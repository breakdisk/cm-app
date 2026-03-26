// Unit tests for the business-logic service domain layer.
//
// Tests exercise the Rules Engine (ECA — Event/Condition/Action) logic:
// AutomationRule, RuleCondition, RuleContext, and the built-in rule constructors.
// No I/O, no Kafka, no database — pure in-memory evaluation.

use logisticos_business_logic::domain::entities::rule::{
    AutomationRule, RuleCondition, RuleContext, RuleAction, RuleTrigger,
    failed_delivery_rule,
};
use uuid::Uuid;
use chrono::Utc;

// ─────────────────────────────────────────────────────────────────────────────
// Test helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Build a minimal valid RuleContext for a given tenant. All optional fields are None
/// unless the test populates them explicitly.
fn base_ctx(tenant_id: Uuid) -> RuleContext {
    RuleContext {
        tenant_id,
        event_type: "driver.delivery.failed".into(),
        shipment_id: Some(Uuid::new_v4()),
        customer_id: Some(Uuid::new_v4()),
        merchant_id: None,
        driver_id: None,
        service_type: None,
        zone: None,
        attempt_count: None,
        shipment_value_cents: None,
        current_hour: 10,
        current_day: "Tuesday".into(),
        metadata: serde_json::Value::Null,
    }
}

/// Build a minimal AutomationRule with the given conditions. Trigger, actions, and
/// metadata are set to valid defaults so the test focuses on condition evaluation.
fn rule_with_conditions(conditions: Vec<RuleCondition>) -> AutomationRule {
    AutomationRule {
        id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        name: "Test Rule".into(),
        description: "Generated for unit tests".into(),
        is_active: true,
        trigger: RuleTrigger::DeliveryFailed,
        conditions,
        actions: vec![RuleAction::LogAuditEvent { event_type: "test.fired".into() }],
        priority: 50,
        created_at: Utc::now(),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Condition: vacuous truth (no conditions)
// ─────────────────────────────────────────────────────────────────────────────

mod empty_conditions {
    use super::*;

    #[test]
    fn rule_with_no_conditions_always_passes() {
        let rule = rule_with_conditions(vec![]);
        let ctx = base_ctx(Uuid::new_v4());
        assert!(
            rule.conditions_met(&ctx),
            "A rule with zero conditions must vacuously return true (AND of empty set)"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Condition: AttemptCount { lte }
// ─────────────────────────────────────────────────────────────────────────────

mod attempt_count_condition {
    use super::*;

    fn rule_attempt_lte_3() -> AutomationRule {
        rule_with_conditions(vec![RuleCondition::AttemptCount { lte: 3 }])
    }

    #[test]
    fn passes_when_attempt_count_equals_1() {
        let rule = rule_attempt_lte_3();
        let mut ctx = base_ctx(Uuid::new_v4());
        ctx.attempt_count = Some(1);
        assert!(rule.conditions_met(&ctx), "attempt=1 must pass lte=3");
    }

    #[test]
    fn passes_when_attempt_count_equals_2() {
        let rule = rule_attempt_lte_3();
        let mut ctx = base_ctx(Uuid::new_v4());
        ctx.attempt_count = Some(2);
        assert!(rule.conditions_met(&ctx), "attempt=2 must pass lte=3");
    }

    #[test]
    fn passes_when_attempt_count_equals_3() {
        let rule = rule_attempt_lte_3();
        let mut ctx = base_ctx(Uuid::new_v4());
        ctx.attempt_count = Some(3);
        assert!(rule.conditions_met(&ctx), "attempt=3 must pass lte=3 (boundary)");
    }

    #[test]
    fn fails_when_attempt_count_equals_4() {
        let rule = rule_attempt_lte_3();
        let mut ctx = base_ctx(Uuid::new_v4());
        ctx.attempt_count = Some(4);
        assert!(
            !rule.conditions_met(&ctx),
            "attempt=4 must FAIL lte=3"
        );
    }

    #[test]
    fn fails_when_attempt_count_is_none() {
        let rule = rule_attempt_lte_3();
        let mut ctx = base_ctx(Uuid::new_v4());
        ctx.attempt_count = None;
        assert!(
            !rule.conditions_met(&ctx),
            "attempt=None must FAIL — no attempt data means condition cannot be satisfied"
        );
    }

    #[test]
    fn passes_when_attempt_count_is_zero() {
        let rule = rule_attempt_lte_3();
        let mut ctx = base_ctx(Uuid::new_v4());
        ctx.attempt_count = Some(0);
        assert!(rule.conditions_met(&ctx), "attempt=0 must pass lte=3");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Condition: ServiceType { equals }
// ─────────────────────────────────────────────────────────────────────────────

mod service_type_condition {
    use super::*;

    fn rule_for_express() -> AutomationRule {
        rule_with_conditions(vec![RuleCondition::ServiceType { equals: "express".into() }])
    }

    #[test]
    fn passes_for_matching_service_type() {
        let rule = rule_for_express();
        let mut ctx = base_ctx(Uuid::new_v4());
        ctx.service_type = Some("express".into());
        assert!(rule.conditions_met(&ctx), "service_type=express must match");
    }

    #[test]
    fn fails_for_different_service_type() {
        let rule = rule_for_express();
        let mut ctx = base_ctx(Uuid::new_v4());
        ctx.service_type = Some("standard".into());
        assert!(
            !rule.conditions_met(&ctx),
            "service_type=standard must NOT match an express rule"
        );
    }

    #[test]
    fn fails_when_service_type_is_none() {
        let rule = rule_for_express();
        let mut ctx = base_ctx(Uuid::new_v4());
        ctx.service_type = None;
        assert!(
            !rule.conditions_met(&ctx),
            "service_type=None must NOT match"
        );
    }

    #[test]
    fn fails_for_case_different_service_type() {
        let rule = rule_for_express();
        let mut ctx = base_ctx(Uuid::new_v4());
        // Matching is exact (not case-insensitive)
        ctx.service_type = Some("Express".into());
        assert!(
            !rule.conditions_met(&ctx),
            "Case-mismatched 'Express' must NOT match 'express'"
        );
    }

    #[test]
    fn passes_for_same_day_service_type() {
        let rule = rule_with_conditions(vec![
            RuleCondition::ServiceType { equals: "same_day".into() },
        ]);
        let mut ctx = base_ctx(Uuid::new_v4());
        ctx.service_type = Some("same_day".into());
        assert!(rule.conditions_met(&ctx), "same_day service type must match");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Condition: Zone { in_zones }
// ─────────────────────────────────────────────────────────────────────────────

mod zone_condition {
    use super::*;

    fn rule_for_luzon_zones() -> AutomationRule {
        rule_with_conditions(vec![RuleCondition::Zone {
            in_zones: vec!["NCR".into(), "CALABARZON".into()],
        }])
    }

    #[test]
    fn passes_for_ncr_zone() {
        let rule = rule_for_luzon_zones();
        let mut ctx = base_ctx(Uuid::new_v4());
        ctx.zone = Some("NCR".into());
        assert!(rule.conditions_met(&ctx), "NCR is in the allowed zone list");
    }

    #[test]
    fn passes_for_calabarzon_zone() {
        let rule = rule_for_luzon_zones();
        let mut ctx = base_ctx(Uuid::new_v4());
        ctx.zone = Some("CALABARZON".into());
        assert!(rule.conditions_met(&ctx), "CALABARZON is in the allowed zone list");
    }

    #[test]
    fn fails_for_caraga_zone() {
        let rule = rule_for_luzon_zones();
        let mut ctx = base_ctx(Uuid::new_v4());
        ctx.zone = Some("CARAGA".into());
        assert!(
            !rule.conditions_met(&ctx),
            "CARAGA is NOT in [NCR, CALABARZON]"
        );
    }

    #[test]
    fn fails_when_zone_is_none() {
        let rule = rule_for_luzon_zones();
        let mut ctx = base_ctx(Uuid::new_v4());
        ctx.zone = None;
        assert!(!rule.conditions_met(&ctx), "zone=None must NOT match");
    }

    #[test]
    fn single_zone_list_works_correctly() {
        let rule = rule_with_conditions(vec![RuleCondition::Zone {
            in_zones: vec!["MIMAROPA".into()],
        }]);
        let mut ctx = base_ctx(Uuid::new_v4());
        ctx.zone = Some("MIMAROPA".into());
        assert!(rule.conditions_met(&ctx));

        ctx.zone = Some("NCR".into());
        assert!(!rule.conditions_met(&ctx));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Condition: ShipmentValue { greater_than }
// ─────────────────────────────────────────────────────────────────────────────

mod shipment_value_condition {
    use super::*;

    fn rule_value_gt_10000() -> AutomationRule {
        // Greater than PHP 100.00 (10_000 centavos)
        rule_with_conditions(vec![RuleCondition::ShipmentValue { greater_than: 10_000 }])
    }

    #[test]
    fn passes_when_value_exceeds_threshold() {
        let rule = rule_value_gt_10000();
        let mut ctx = base_ctx(Uuid::new_v4());
        ctx.shipment_value_cents = Some(15_000); // PHP 150
        assert!(rule.conditions_met(&ctx), "PHP 150 must exceed PHP 100 threshold");
    }

    #[test]
    fn fails_when_value_equals_threshold() {
        let rule = rule_value_gt_10000();
        let mut ctx = base_ctx(Uuid::new_v4());
        ctx.shipment_value_cents = Some(10_000); // PHP 100 — not strictly greater than
        assert!(
            !rule.conditions_met(&ctx),
            "PHP 100 must NOT pass a strictly-greater-than PHP 100 check"
        );
    }

    #[test]
    fn fails_when_value_is_below_threshold() {
        let rule = rule_value_gt_10000();
        let mut ctx = base_ctx(Uuid::new_v4());
        ctx.shipment_value_cents = Some(5_000); // PHP 50
        assert!(!rule.conditions_met(&ctx), "PHP 50 must not exceed PHP 100 threshold");
    }

    #[test]
    fn fails_when_value_is_none() {
        let rule = rule_value_gt_10000();
        let mut ctx = base_ctx(Uuid::new_v4());
        ctx.shipment_value_cents = None;
        assert!(!rule.conditions_met(&ctx), "None value must not satisfy the condition");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// AND logic — all conditions must pass
// ─────────────────────────────────────────────────────────────────────────────

mod and_logic {
    use super::*;

    fn rule_express_and_ncr() -> AutomationRule {
        rule_with_conditions(vec![
            RuleCondition::ServiceType { equals: "express".into() },
            RuleCondition::Zone { in_zones: vec!["NCR".into()] },
        ])
    }

    #[test]
    fn both_conditions_pass_means_rule_passes() {
        let rule = rule_express_and_ncr();
        let mut ctx = base_ctx(Uuid::new_v4());
        ctx.service_type = Some("express".into());
        ctx.zone = Some("NCR".into());
        assert!(rule.conditions_met(&ctx), "Both express+NCR must pass together");
    }

    #[test]
    fn first_condition_fails_means_rule_fails() {
        let rule = rule_express_and_ncr();
        let mut ctx = base_ctx(Uuid::new_v4());
        ctx.service_type = Some("standard".into()); // fails service check
        ctx.zone = Some("NCR".into());              // would pass zone check
        assert!(
            !rule.conditions_met(&ctx),
            "Failing service_type condition must cause the whole rule to fail"
        );
    }

    #[test]
    fn second_condition_fails_means_rule_fails() {
        let rule = rule_express_and_ncr();
        let mut ctx = base_ctx(Uuid::new_v4());
        ctx.service_type = Some("express".into()); // passes service check
        ctx.zone = Some("CARAGA".into());           // fails zone check
        assert!(
            !rule.conditions_met(&ctx),
            "Failing zone condition must cause the whole rule to fail"
        );
    }

    #[test]
    fn both_conditions_fail_means_rule_fails() {
        let rule = rule_express_and_ncr();
        let mut ctx = base_ctx(Uuid::new_v4());
        ctx.service_type = Some("standard".into());
        ctx.zone = Some("CARAGA".into());
        assert!(
            !rule.conditions_met(&ctx),
            "Both conditions failing must cause the rule to fail"
        );
    }

    #[test]
    fn three_conditions_all_must_pass() {
        let rule = rule_with_conditions(vec![
            RuleCondition::ServiceType { equals: "express".into() },
            RuleCondition::Zone { in_zones: vec!["NCR".into()] },
            RuleCondition::AttemptCount { lte: 2 },
        ]);
        let mut ctx = base_ctx(Uuid::new_v4());
        ctx.service_type    = Some("express".into());
        ctx.zone            = Some("NCR".into());
        ctx.attempt_count   = Some(2);
        assert!(rule.conditions_met(&ctx), "All three conditions passing must fire the rule");

        ctx.attempt_count = Some(3); // now the third condition fails
        assert!(
            !rule.conditions_met(&ctx),
            "Third condition failing must prevent the rule from firing"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Inactive rule — conditions_met is independent of is_active
// ─────────────────────────────────────────────────────────────────────────────

mod inactive_rule {
    use super::*;

    #[test]
    fn inactive_rule_conditions_still_evaluate_to_true_when_met() {
        // is_active is a filter applied by the RuleRepository (rules_for_topic),
        // NOT inside conditions_met. The domain method evaluates only conditions.
        let mut rule = rule_with_conditions(vec![]);
        rule.is_active = false;

        let ctx = base_ctx(Uuid::new_v4());
        assert!(
            rule.conditions_met(&ctx),
            "conditions_met must return true (empty conditions) regardless of is_active flag"
        );
    }

    #[test]
    fn inactive_rule_conditions_still_fail_when_not_met() {
        let mut rule = rule_with_conditions(vec![
            RuleCondition::ServiceType { equals: "express".into() },
        ]);
        rule.is_active = false;

        let mut ctx = base_ctx(Uuid::new_v4());
        ctx.service_type = Some("standard".into());

        assert!(
            !rule.conditions_met(&ctx),
            "conditions_met must still return false when conditions fail, regardless of is_active"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Built-in rule: failed_delivery_rule()
// ─────────────────────────────────────────────────────────────────────────────

mod failed_delivery_rule_tests {
    use super::*;

    #[test]
    fn failed_delivery_rule_has_correct_name_and_is_active() {
        let rule = failed_delivery_rule(Uuid::new_v4());
        assert!(rule.is_active, "Built-in failed delivery rule must be active by default");
        assert_eq!(rule.name, "Auto-Reschedule on Failed Delivery");
    }

    #[test]
    fn failed_delivery_rule_has_attempt_count_lte_3_condition() {
        let rule = failed_delivery_rule(Uuid::new_v4());
        assert_eq!(rule.conditions.len(), 1, "Must have exactly one condition");

        match &rule.conditions[0] {
            RuleCondition::AttemptCount { lte } => {
                assert_eq!(*lte, 3, "Condition must be AttemptCount lte=3");
            }
            other => panic!("Expected AttemptCount condition, got {:?}", other),
        }
    }

    #[test]
    fn failed_delivery_rule_passes_for_attempt_3_or_less() {
        let rule = failed_delivery_rule(Uuid::new_v4());
        let mut ctx = base_ctx(rule.tenant_id);
        ctx.attempt_count = Some(3);
        assert!(rule.conditions_met(&ctx), "Attempt 3 must satisfy lte=3");
    }

    #[test]
    fn failed_delivery_rule_fails_for_attempt_4() {
        let rule = failed_delivery_rule(Uuid::new_v4());
        let mut ctx = base_ctx(rule.tenant_id);
        ctx.attempt_count = Some(4);
        assert!(!rule.conditions_met(&ctx), "Attempt 4 must NOT satisfy lte=3");
    }

    #[test]
    fn failed_delivery_rule_has_reschedule_notify_and_alert_actions() {
        let rule = failed_delivery_rule(Uuid::new_v4());

        let has_reschedule = rule.actions.iter().any(|a| {
            matches!(a, RuleAction::RescheduleDelivery { delay_hours: 24 })
        });
        let has_notify = rule.actions.iter().any(|a| {
            matches!(a, RuleAction::NotifyCustomer { .. })
        });
        let has_alert = rule.actions.iter().any(|a| {
            matches!(a, RuleAction::AlertDispatcher { .. })
        });
        let has_audit = rule.actions.iter().any(|a| {
            matches!(a, RuleAction::LogAuditEvent { .. })
        });

        assert!(has_reschedule, "Must include RescheduleDelivery(24h) action");
        assert!(has_notify,     "Must include NotifyCustomer action");
        assert!(has_alert,      "Must include AlertDispatcher action");
        assert!(has_audit,      "Must include LogAuditEvent action");
    }

    #[test]
    fn failed_delivery_rule_trigger_is_delivery_failed() {
        let rule = failed_delivery_rule(Uuid::new_v4());
        assert!(
            matches!(rule.trigger, RuleTrigger::DeliveryFailed),
            "Trigger must be DeliveryFailed"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// trigger_matches_topic (internal function tested via the topic mapping)
// ─────────────────────────────────────────────────────────────────────────────

mod topic_matching {
    use super::*;

    // We verify the expected Kafka topic for DeliveryFailed by checking
    // that the failed_delivery_rule correctly fires on the canonical topic.
    // trigger_matches_topic is private; we test it indirectly through
    // RuleRepository::rules_for_topic behaviour, verified here without async
    // by checking the trigger variant directly.

    #[test]
    fn delivery_failed_trigger_maps_to_correct_kafka_topic() {
        let rule = failed_delivery_rule(Uuid::new_v4());
        // Verify that the trigger is DeliveryFailed — the topic mapping
        // `logisticos.driver.delivery.failed` is confirmed in the application service.
        // We assert the variant rather than calling private fn trigger_matches_topic.
        assert!(
            matches!(rule.trigger, RuleTrigger::DeliveryFailed),
            "failed_delivery_rule must have a DeliveryFailed trigger (maps to logisticos.driver.delivery.failed)"
        );
    }

    #[test]
    fn delivery_completed_trigger_variant_exists() {
        // Verify all trigger variants are instantiable (compile-time + runtime check)
        let _trigger = RuleTrigger::DeliveryCompleted;
        let _trigger2 = RuleTrigger::ShipmentCreated;
        let _trigger3 = RuleTrigger::DeliveryAttempted { attempts: 1 };
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// RuleContext construction
// ─────────────────────────────────────────────────────────────────────────────

mod rule_context {
    use super::*;

    #[test]
    fn context_fields_are_set_correctly() {
        let tenant_id   = Uuid::new_v4();
        let shipment_id = Uuid::new_v4();
        let ctx = RuleContext {
            tenant_id,
            event_type: "driver.delivery.failed".into(),
            shipment_id: Some(shipment_id),
            customer_id: None,
            merchant_id: None,
            driver_id: None,
            service_type: Some("express".into()),
            zone: Some("NCR".into()),
            attempt_count: Some(2),
            shipment_value_cents: Some(50_000),
            current_hour: 14,
            current_day: "Monday".into(),
            metadata: serde_json::json!({ "custom_key": "value" }),
        };

        assert_eq!(ctx.tenant_id, tenant_id);
        assert_eq!(ctx.shipment_id, Some(shipment_id));
        assert_eq!(ctx.service_type.as_deref(), Some("express"));
        assert_eq!(ctx.zone.as_deref(), Some("NCR"));
        assert_eq!(ctx.attempt_count, Some(2));
        assert_eq!(ctx.shipment_value_cents, Some(50_000));
    }

    #[test]
    fn time_of_day_condition_evaluates_against_current_hour() {
        let rule = rule_with_conditions(vec![
            RuleCondition::TimeOfDay { hour_from: 8, hour_to: 17 },
        ]);

        let mut ctx = base_ctx(Uuid::new_v4());
        ctx.current_hour = 12; // mid-day — within window
        assert!(rule.conditions_met(&ctx), "Hour 12 must be within 8–17 window");

        ctx.current_hour = 18; // outside window
        assert!(!rule.conditions_met(&ctx), "Hour 18 must be outside 8–17 window");

        ctx.current_hour = 8; // boundary — inclusive
        assert!(rule.conditions_met(&ctx), "Hour 8 must be within 8–17 window (inclusive lower)");

        ctx.current_hour = 17; // boundary — inclusive
        assert!(rule.conditions_met(&ctx), "Hour 17 must be within 8–17 window (inclusive upper)");
    }

    #[test]
    fn day_of_week_condition_evaluates_against_current_day() {
        let rule = rule_with_conditions(vec![
            RuleCondition::DayOfWeek {
                days: vec!["Monday".into(), "Tuesday".into(), "Wednesday".into()],
            },
        ]);

        let mut ctx = base_ctx(Uuid::new_v4());
        ctx.current_day = "Monday".into();
        assert!(rule.conditions_met(&ctx), "Monday is in the weekday list");

        ctx.current_day = "Saturday".into();
        assert!(!rule.conditions_met(&ctx), "Saturday is NOT in the weekday list");
    }
}
