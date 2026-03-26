// Unit tests for the carrier service domain layer.
//
// Tests exercise Carrier, RateCard, SlaCommitment, and PerformanceGrade
// business rules in isolation. No database, no HTTP, no Kafka — pure domain
// logic exercised directly against entity methods and field values.

use logisticos_carrier::domain::entities::{
    Carrier, CarrierStatus, PerformanceGrade, RateCard, SlaCommitment,
};
use logisticos_types::TenantId;

// ─────────────────────────────────────────────────────────────────────────────
// Test helpers
// ─────────────────────────────────────────────────────────────────────────────

fn make_tenant() -> TenantId {
    TenantId::new()
}

fn make_sla() -> SlaCommitment {
    SlaCommitment {
        on_time_target_pct:  95.0,
        max_delivery_days:   2,
        penalty_per_breach:  500,
    }
}

fn make_carrier(code: &str) -> Carrier {
    Carrier::new(
        make_tenant(),
        "J&T Express".into(),
        code.into(),
        "ops@jnt.ph".into(),
        make_sla(),
    )
}

fn make_rate_card(service_type: &str, base: i64, per_kg: i64, max_kg: f32) -> RateCard {
    RateCard {
        service_type:    service_type.into(),
        base_rate_cents: base,
        per_kg_cents:    per_kg,
        max_weight_kg:   max_kg,
        coverage_zones:  vec!["ZONE-A".into(), "ZONE-B".into()],
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Carrier::new() — defaults
// ─────────────────────────────────────────────────────────────────────────────

mod carrier_new {
    use super::*;

    #[test]
    fn new_carrier_status_is_pending_verification() {
        let c = make_carrier("JNT");
        assert_eq!(
            c.status,
            CarrierStatus::PendingVerification,
            "New carrier must start with status=PendingVerification"
        );
    }

    #[test]
    fn new_carrier_has_empty_rate_cards() {
        let c = make_carrier("JNT");
        assert!(c.rate_cards.is_empty(), "New carrier must have no rate cards");
    }

    #[test]
    fn new_carrier_code_is_uppercased() {
        let c = Carrier::new(
            make_tenant(),
            "Grab Express".into(),
            "grab".into(), // lowercase input
            "ops@grab.ph".into(),
            make_sla(),
        );
        assert_eq!(c.code, "GRAB", "Carrier code must be uppercased on creation");
    }

    #[test]
    fn mixed_case_code_is_uppercased() {
        let c = Carrier::new(
            make_tenant(),
            "LBC Express".into(),
            "Lbc".into(),
            "ops@lbc.ph".into(),
            make_sla(),
        );
        assert_eq!(c.code, "LBC");
    }

    #[test]
    fn already_uppercase_code_stays_uppercase() {
        let c = make_carrier("FLASH");
        assert_eq!(c.code, "FLASH");
    }

    #[test]
    fn new_carrier_has_zero_total_shipments() {
        let c = make_carrier("JNT");
        assert_eq!(c.total_shipments, 0);
    }

    #[test]
    fn new_carrier_has_zero_on_time_count() {
        let c = make_carrier("JNT");
        assert_eq!(c.on_time_count, 0);
    }

    #[test]
    fn new_carrier_has_zero_failed_count() {
        let c = make_carrier("JNT");
        assert_eq!(c.failed_count, 0);
    }

    #[test]
    fn new_carrier_stores_name() {
        let c = Carrier::new(
            make_tenant(),
            "Ninja Van PH".into(),
            "NV".into(),
            "ops@ninja.ph".into(),
            make_sla(),
        );
        assert_eq!(c.name, "Ninja Van PH");
    }

    #[test]
    fn new_carrier_stores_contact_email() {
        let c = Carrier::new(
            make_tenant(),
            "Carrier".into(),
            "CAR".into(),
            "contact@carrier.ph".into(),
            make_sla(),
        );
        assert_eq!(c.contact_email, "contact@carrier.ph");
    }

    #[test]
    fn new_carrier_has_no_api_endpoint() {
        let c = make_carrier("JNT");
        assert!(c.api_endpoint.is_none());
    }

    #[test]
    fn new_carrier_has_no_api_key_hash() {
        let c = make_carrier("JNT");
        assert!(c.api_key_hash.is_none());
    }

    #[test]
    fn new_carrier_has_unique_id() {
        let a = make_carrier("JNT");
        let b = make_carrier("JNT");
        assert_ne!(a.id.inner(), b.id.inner(), "Each carrier must get a unique ID");
    }

    #[test]
    fn new_carrier_stores_sla() {
        let sla = SlaCommitment {
            on_time_target_pct: 97.5,
            max_delivery_days: 1,
            penalty_per_breach: 1000,
        };
        let c = Carrier::new(
            make_tenant(),
            "Fast Carrier".into(),
            "FAST".into(),
            "fast@test.ph".into(),
            sla,
        );
        assert!((c.sla.on_time_target_pct - 97.5).abs() < 0.01);
        assert_eq!(c.sla.max_delivery_days, 1);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Carrier::activate()
// ─────────────────────────────────────────────────────────────────────────────

mod carrier_activate {
    use super::*;

    #[test]
    fn activate_from_pending_verification_sets_status_active() {
        let mut c = make_carrier("JNT");
        assert_eq!(c.status, CarrierStatus::PendingVerification);
        c.activate().expect("activate from PendingVerification must succeed");
        assert_eq!(c.status, CarrierStatus::Active);
    }

    #[test]
    fn activate_from_suspended_sets_status_active() {
        let mut c = make_carrier("JNT");
        c.activate().unwrap();
        c.suspend("test reason");
        c.activate().expect("activate from Suspended must succeed");
        assert_eq!(c.status, CarrierStatus::Active);
    }

    #[test]
    fn activate_returns_ok_result() {
        let mut c = make_carrier("JNT");
        assert!(c.activate().is_ok());
    }

    #[test]
    fn activate_from_deactivated_returns_error() {
        let mut c = make_carrier("JNT");
        // Force status to Deactivated via direct field manipulation
        // (no deactivate() method exists on the entity)
        c.status = CarrierStatus::Deactivated;
        let result = c.activate();
        assert!(result.is_err(), "Cannot reactivate a Deactivated carrier");
    }

    #[test]
    fn activate_from_deactivated_error_message_is_descriptive() {
        let mut c = make_carrier("JNT");
        c.status = CarrierStatus::Deactivated;
        let err = c.activate().unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("deactivat") || msg.contains("cannot"),
            "Error message must describe why reactivation failed, got: {}",
            err
        );
    }

    #[test]
    fn activate_twice_stays_active() {
        let mut c = make_carrier("JNT");
        c.activate().unwrap();
        c.activate().unwrap(); // idempotent
        assert_eq!(c.status, CarrierStatus::Active);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Carrier::suspend()
// ─────────────────────────────────────────────────────────────────────────────

mod carrier_suspend {
    use super::*;

    #[test]
    fn suspend_sets_status_to_suspended() {
        let mut c = make_carrier("JNT");
        c.activate().unwrap();
        c.suspend("compliance failure");
        assert_eq!(c.status, CarrierStatus::Suspended);
    }

    #[test]
    fn suspend_from_pending_sets_suspended() {
        let mut c = make_carrier("JNT");
        c.suspend("pre-activation suspension");
        assert_eq!(c.status, CarrierStatus::Suspended);
    }

    #[test]
    fn cannot_activate_from_deactivated_after_suspend() {
        let mut c = make_carrier("JNT");
        c.status = CarrierStatus::Deactivated;
        // suspend() itself doesn't guard against Deactivated state — it simply sets Suspended
        // The constraint is on activate() from Deactivated
        c.status = CarrierStatus::Deactivated;
        let result = c.activate();
        assert!(result.is_err(), "Cannot activate a Deactivated carrier even after suspension flow");
    }

    #[test]
    fn suspend_then_activate_cycles_correctly() {
        let mut c = make_carrier("LBC");
        c.activate().unwrap();
        assert_eq!(c.status, CarrierStatus::Active);
        c.suspend("temporary hold");
        assert_eq!(c.status, CarrierStatus::Suspended);
        c.activate().unwrap();
        assert_eq!(c.status, CarrierStatus::Active);
    }

    #[test]
    fn suspend_accepts_reason_string() {
        let mut c = make_carrier("GX");
        // The method signature accepts &str; verify no panic
        c.suspend("SLA breach — on-time rate dropped below 60%");
        assert_eq!(c.status, CarrierStatus::Suspended);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Carrier::record_delivery()
// ─────────────────────────────────────────────────────────────────────────────

mod carrier_record_delivery {
    use super::*;

    #[test]
    fn record_on_time_delivery_increments_total_shipments() {
        let mut c = make_carrier("JNT");
        c.record_delivery(true);
        assert_eq!(c.total_shipments, 1);
    }

    #[test]
    fn record_late_delivery_increments_total_shipments() {
        let mut c = make_carrier("JNT");
        c.record_delivery(false);
        assert_eq!(c.total_shipments, 1);
    }

    #[test]
    fn record_on_time_increments_on_time_count() {
        let mut c = make_carrier("JNT");
        c.record_delivery(true);
        assert_eq!(c.on_time_count, 1);
    }

    #[test]
    fn record_late_does_not_increment_on_time_count() {
        let mut c = make_carrier("JNT");
        c.record_delivery(false);
        assert_eq!(c.on_time_count, 0);
    }

    #[test]
    fn record_late_increments_failed_count() {
        let mut c = make_carrier("JNT");
        c.record_delivery(false);
        assert_eq!(c.failed_count, 1);
    }

    #[test]
    fn record_on_time_does_not_increment_failed_count() {
        let mut c = make_carrier("JNT");
        c.record_delivery(true);
        assert_eq!(c.failed_count, 0);
    }

    #[test]
    fn mixed_deliveries_accumulate_correctly() {
        let mut c = make_carrier("JNT");
        // 3 on-time, 2 late
        c.record_delivery(true);
        c.record_delivery(true);
        c.record_delivery(false);
        c.record_delivery(true);
        c.record_delivery(false);
        assert_eq!(c.total_shipments, 5);
        assert_eq!(c.on_time_count, 3);
        assert_eq!(c.failed_count, 2);
    }

    #[test]
    fn performance_grade_updates_after_recording() {
        let mut c = make_carrier("JNT");
        // Record 10 on-time deliveries → 100% rate → Excellent
        for _ in 0..10 {
            c.record_delivery(true);
        }
        assert_eq!(
            c.performance_grade,
            PerformanceGrade::Excellent,
            "100% on-time rate must yield Excellent grade"
        );
    }

    #[test]
    fn performance_grade_drops_to_poor_after_many_late() {
        let mut c = make_carrier("JNT");
        // 40 on-time, 60 late → 40% on-time → Poor
        for _ in 0..40 { c.record_delivery(true); }
        for _ in 0..60 { c.record_delivery(false); }
        assert_eq!(c.performance_grade, PerformanceGrade::Poor);
    }

    #[test]
    fn grade_fair_at_exactly_70_pct() {
        let mut c = make_carrier("JNT");
        // 70 on-time, 30 late → 70.0% → Fair
        for _ in 0..70 { c.record_delivery(true); }
        for _ in 0..30 { c.record_delivery(false); }
        assert_eq!(
            c.performance_grade,
            PerformanceGrade::Fair,
            "70.0% on-time must yield Fair"
        );
    }

    #[test]
    fn grade_good_at_exactly_85_pct() {
        let mut c = make_carrier("JNT");
        // 85 on-time, 15 late → 85.0% → Good
        for _ in 0..85 { c.record_delivery(true); }
        for _ in 0..15 { c.record_delivery(false); }
        assert_eq!(c.performance_grade, PerformanceGrade::Good, "85.0% must yield Good");
    }

    #[test]
    fn grade_excellent_at_exactly_95_pct() {
        let mut c = make_carrier("JNT");
        // 95 on-time, 5 late → 95.0% → Excellent
        for _ in 0..95 { c.record_delivery(true); }
        for _ in 0..5  { c.record_delivery(false); }
        assert_eq!(c.performance_grade, PerformanceGrade::Excellent, "95.0% must yield Excellent");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// PerformanceGrade::from_rate()
// ─────────────────────────────────────────────────────────────────────────────

mod performance_grade_from_rate {
    use super::*;

    #[test]
    fn exactly_95_is_excellent() {
        assert_eq!(PerformanceGrade::from_rate(95.0), PerformanceGrade::Excellent);
    }

    #[test]
    fn above_95_is_excellent() {
        assert_eq!(PerformanceGrade::from_rate(99.0), PerformanceGrade::Excellent);
        assert_eq!(PerformanceGrade::from_rate(100.0), PerformanceGrade::Excellent);
    }

    #[test]
    fn exactly_94_is_good() {
        assert_eq!(PerformanceGrade::from_rate(94.0), PerformanceGrade::Good);
    }

    #[test]
    fn exactly_85_is_good() {
        assert_eq!(PerformanceGrade::from_rate(85.0), PerformanceGrade::Good);
    }

    #[test]
    fn between_85_and_95_exclusive_is_good() {
        assert_eq!(PerformanceGrade::from_rate(90.0), PerformanceGrade::Good);
        assert_eq!(PerformanceGrade::from_rate(85.1), PerformanceGrade::Good);
        assert_eq!(PerformanceGrade::from_rate(94.9), PerformanceGrade::Good);
    }

    #[test]
    fn exactly_84_is_fair() {
        assert_eq!(PerformanceGrade::from_rate(84.0), PerformanceGrade::Fair);
    }

    #[test]
    fn exactly_70_is_fair() {
        assert_eq!(PerformanceGrade::from_rate(70.0), PerformanceGrade::Fair);
    }

    #[test]
    fn between_70_and_85_exclusive_is_fair() {
        assert_eq!(PerformanceGrade::from_rate(75.0), PerformanceGrade::Fair);
        assert_eq!(PerformanceGrade::from_rate(70.1), PerformanceGrade::Fair);
        assert_eq!(PerformanceGrade::from_rate(84.9), PerformanceGrade::Fair);
    }

    #[test]
    fn exactly_69_is_poor() {
        assert_eq!(PerformanceGrade::from_rate(69.0), PerformanceGrade::Poor);
    }

    #[test]
    fn zero_is_poor() {
        assert_eq!(PerformanceGrade::from_rate(0.0), PerformanceGrade::Poor);
    }

    #[test]
    fn just_below_70_is_poor() {
        assert_eq!(PerformanceGrade::from_rate(69.9), PerformanceGrade::Poor);
    }

    #[test]
    fn just_below_85_is_fair() {
        assert_eq!(PerformanceGrade::from_rate(84.99), PerformanceGrade::Fair);
    }

    #[test]
    fn just_below_95_is_good() {
        assert_eq!(PerformanceGrade::from_rate(94.99), PerformanceGrade::Good);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// RateCard — compute cost
// ─────────────────────────────────────────────────────────────────────────────

mod rate_card_computation {
    use super::*;

    #[test]
    fn quote_returns_base_plus_per_kg_times_weight() {
        // base = 5000 centavos, per_kg = 1000 centavos, weight = 2.0 kg
        // expected = 5000 + 1000 * 2.0 = 7000
        let card = make_rate_card("standard", 5000, 1000, 30.0);
        let mut c = make_carrier("JNT");
        c.rate_cards.push(card);
        let quote = c.quote("standard", 2.0).expect("quote must be returned for valid inputs");
        assert_eq!(quote, 7000, "5000 base + 1000*2 kg = 7000 centavos");
    }

    #[test]
    fn quote_at_zero_kg_returns_base_rate_only() {
        let card = make_rate_card("standard", 5000, 1000, 30.0);
        let mut c = make_carrier("JNT");
        c.rate_cards.push(card);
        let quote = c.quote("standard", 0.0).expect("0kg quote must succeed");
        assert_eq!(quote, 5000, "0kg parcel must return only the base rate");
    }

    #[test]
    fn quote_returns_none_when_no_rate_cards() {
        let c = make_carrier("JNT");
        assert!(c.quote("standard", 1.0).is_none(), "No rate cards means no quote");
    }

    #[test]
    fn quote_returns_none_for_unknown_service_type() {
        let card = make_rate_card("standard", 5000, 1000, 30.0);
        let mut c = make_carrier("JNT");
        c.rate_cards.push(card);
        assert!(
            c.quote("express", 1.0).is_none(),
            "Quote for non-existent service type must be None"
        );
    }

    #[test]
    fn quote_returns_none_when_weight_exceeds_max() {
        let card = make_rate_card("standard", 5000, 1000, 5.0); // max 5kg
        let mut c = make_carrier("JNT");
        c.rate_cards.push(card);
        assert!(
            c.quote("standard", 6.0).is_none(),
            "Quote for weight exceeding max_weight_kg must be None"
        );
    }

    #[test]
    fn quote_matches_at_exact_max_weight() {
        let card = make_rate_card("standard", 5000, 1000, 5.0);
        let mut c = make_carrier("JNT");
        c.rate_cards.push(card);
        // 5.0 kg == max_weight_kg → must return a quote
        let quote = c.quote("standard", 5.0);
        assert!(quote.is_some(), "Weight at exactly max_weight_kg must be quoted");
        assert_eq!(quote.unwrap(), 5000 + (1000.0_f32 * 5.0) as i64);
    }

    #[test]
    fn quote_uses_first_matching_rate_card() {
        let card_a = make_rate_card("standard", 5000, 1000, 10.0);
        let card_b = make_rate_card("standard", 3000, 500, 20.0); // cheaper but second
        let mut c = make_carrier("JNT");
        c.rate_cards.push(card_a);
        c.rate_cards.push(card_b);
        // The entity's quote() returns the first matching card
        let quote = c.quote("standard", 1.0).unwrap();
        assert_eq!(quote, 5000 + 1000, "First matching card must be used: 5000 + 1000*1 = 6000");
    }

    #[test]
    fn rate_card_coverage_zones_stored() {
        let card = RateCard {
            service_type:    "same_day".into(),
            base_rate_cents: 20000,
            per_kg_cents:    2000,
            max_weight_kg:   3.0,
            coverage_zones:  vec!["QUEZON-CITY".into(), "MANILA".into()],
        };
        assert_eq!(card.coverage_zones.len(), 2);
        assert!(card.coverage_zones.contains(&"MANILA".to_string()));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SlaCommitment defaults
// ─────────────────────────────────────────────────────────────────────────────

mod sla_commitment_defaults {
    use super::*;

    #[test]
    fn default_on_time_target_is_90_pct() {
        let sla = SlaCommitment::default();
        assert!(
            (sla.on_time_target_pct - 90.0).abs() < 0.01,
            "Default SLA on_time_target_pct must be 90.0, got {}",
            sla.on_time_target_pct
        );
    }

    #[test]
    fn default_max_delivery_days_is_3() {
        let sla = SlaCommitment::default();
        assert_eq!(sla.max_delivery_days, 3, "Default max_delivery_days must be 3");
    }

    #[test]
    fn default_penalty_per_breach_is_zero() {
        let sla = SlaCommitment::default();
        assert_eq!(sla.penalty_per_breach, 0, "Default penalty_per_breach must be 0");
    }

    #[test]
    fn custom_sla_values_are_stored() {
        let sla = SlaCommitment {
            on_time_target_pct: 98.0,
            max_delivery_days:  1,
            penalty_per_breach: 2500,
        };
        assert!((sla.on_time_target_pct - 98.0).abs() < 0.01);
        assert_eq!(sla.max_delivery_days, 1);
        assert_eq!(sla.penalty_per_breach, 2500);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Carrier::on_time_rate()
// ─────────────────────────────────────────────────────────────────────────────

mod carrier_on_time_rate {
    use super::*;

    #[test]
    fn returns_zero_when_no_shipments() {
        let c = make_carrier("JNT");
        assert_eq!(
            c.on_time_rate(), 0.0,
            "on_time_rate must be 0.0 when no shipments have been recorded"
        );
    }

    #[test]
    fn returns_100_when_all_on_time() {
        let mut c = make_carrier("JNT");
        for _ in 0..10 { c.record_delivery(true); }
        assert!(
            (c.on_time_rate() - 100.0).abs() < 0.01,
            "All on-time deliveries must yield 100.0% rate"
        );
    }

    #[test]
    fn returns_0_when_all_late() {
        let mut c = make_carrier("JNT");
        for _ in 0..10 { c.record_delivery(false); }
        assert_eq!(c.on_time_rate(), 0.0, "All late deliveries must yield 0.0% rate");
    }

    #[test]
    fn returns_correct_pct_for_mixed_deliveries() {
        let mut c = make_carrier("JNT");
        for _ in 0..3 { c.record_delivery(true); }
        for _ in 0..1 { c.record_delivery(false); }
        // 3/4 = 75.0%
        let rate = c.on_time_rate();
        assert!((rate - 75.0).abs() < 0.01, "3 of 4 on-time must yield 75.0%, got {}", rate);
    }

    #[test]
    fn returns_50_pct_for_equal_split() {
        let mut c = make_carrier("JNT");
        for _ in 0..5 { c.record_delivery(true); }
        for _ in 0..5 { c.record_delivery(false); }
        let rate = c.on_time_rate();
        assert!((rate - 50.0).abs() < 0.01, "5 of 10 on-time must yield 50.0%, got {}", rate);
    }

    #[test]
    fn on_time_rate_consistent_with_on_time_count_and_total() {
        let mut c = make_carrier("JNT");
        for _ in 0..7  { c.record_delivery(true); }
        for _ in 0..3  { c.record_delivery(false); }
        let expected = c.on_time_count as f64 / c.total_shipments as f64 * 100.0;
        let actual = c.on_time_rate();
        assert!(
            (actual - expected).abs() < 0.001,
            "on_time_rate() must equal on_time_count/total_shipments * 100"
        );
    }

    #[test]
    fn one_on_time_delivery_returns_100() {
        let mut c = make_carrier("JNT");
        c.record_delivery(true);
        assert!((c.on_time_rate() - 100.0).abs() < 0.01);
    }

    #[test]
    fn one_late_delivery_returns_0() {
        let mut c = make_carrier("JNT");
        c.record_delivery(false);
        assert_eq!(c.on_time_rate(), 0.0);
    }
}
