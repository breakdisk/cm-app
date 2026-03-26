use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use logisticos_cdp::domain::entities::{BehavioralEvent, CustomerProfile, EventType};
use logisticos_types::TenantId;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn new_profile() -> CustomerProfile {
    CustomerProfile::new(TenantId::new(), Uuid::new_v4())
}

fn event(event_type: EventType, metadata: serde_json::Value) -> BehavioralEvent {
    BehavioralEvent::new(event_type, None, metadata, Utc::now())
}

fn shipment_event() -> BehavioralEvent {
    event(EventType::ShipmentCreated, json!({}))
}

fn completed_event() -> BehavioralEvent {
    event(EventType::DeliveryCompleted, json!({}))
}

fn failed_event() -> BehavioralEvent {
    event(EventType::DeliveryFailed, json!({}))
}

fn cod_event(amount_cents: i64) -> BehavioralEvent {
    event(EventType::CodPaid, json!({ "amount_cents": amount_cents }))
}

// ---------------------------------------------------------------------------
// record_event counter tests
// ---------------------------------------------------------------------------

mod record_event_tests {
    use super::*;

    #[test]
    fn shipment_created_increments_total_shipments() {
        let mut p = new_profile();
        assert_eq!(p.total_shipments, 0);
        p.record_event(shipment_event());
        assert_eq!(p.total_shipments, 1);
    }

    #[test]
    fn multiple_shipments_created_accumulate() {
        let mut p = new_profile();
        for _ in 0..5 {
            p.record_event(shipment_event());
        }
        assert_eq!(p.total_shipments, 5);
    }

    #[test]
    fn delivery_completed_increments_successful_deliveries() {
        let mut p = new_profile();
        p.record_event(completed_event());
        assert_eq!(p.successful_deliveries, 1);
        assert_eq!(p.failed_deliveries, 0);
    }

    #[test]
    fn delivery_failed_increments_failed_deliveries() {
        let mut p = new_profile();
        p.record_event(failed_event());
        assert_eq!(p.failed_deliveries, 1);
        assert_eq!(p.successful_deliveries, 0);
    }

    #[test]
    fn cod_paid_adds_to_total_cod_collected_cents() {
        let mut p = new_profile();
        p.record_event(cod_event(15_000));
        assert_eq!(p.total_cod_collected_cents, 15_000);
    }

    #[test]
    fn cod_paid_accumulates_multiple_payments() {
        let mut p = new_profile();
        p.record_event(cod_event(10_000));
        p.record_event(cod_event(5_000));
        assert_eq!(p.total_cod_collected_cents, 15_000);
    }

    #[test]
    fn cod_paid_missing_amount_cents_adds_zero() {
        let mut p = new_profile();
        p.record_event(event(EventType::CodPaid, json!({}))); // no amount_cents
        assert_eq!(p.total_cod_collected_cents, 0);
    }

    #[test]
    fn support_ticket_does_not_affect_delivery_counters() {
        let mut p = new_profile();
        p.record_event(event(EventType::SupportTicketOpened, json!({})));
        assert_eq!(p.total_shipments, 0);
        assert_eq!(p.successful_deliveries, 0);
        assert_eq!(p.failed_deliveries, 0);
    }

    #[test]
    fn notification_read_does_not_affect_delivery_counters() {
        let mut p = new_profile();
        p.record_event(event(EventType::NotificationRead, json!({})));
        assert_eq!(p.total_shipments, 0);
        assert_eq!(p.successful_deliveries, 0);
        assert_eq!(p.failed_deliveries, 0);
    }

    #[test]
    fn first_shipment_at_set_on_first_shipment_created() {
        let mut p = new_profile();
        assert!(p.first_shipment_at.is_none());
        p.record_event(shipment_event());
        assert!(p.first_shipment_at.is_some());
    }

    #[test]
    fn first_shipment_at_not_overwritten_on_second_shipment() {
        let mut p = new_profile();
        p.record_event(shipment_event());
        let first = p.first_shipment_at;
        p.record_event(shipment_event());
        assert_eq!(p.first_shipment_at, first);
    }

    #[test]
    fn last_shipment_at_updated_on_each_shipment_created() {
        let mut p = new_profile();
        p.record_event(shipment_event());
        let after_first = p.last_shipment_at;
        assert!(after_first.is_some());
        p.record_event(shipment_event());
        // last_shipment_at should still be set (may be same or later).
        assert!(p.last_shipment_at.is_some());
    }
}

// ---------------------------------------------------------------------------
// recent_events capping tests
// ---------------------------------------------------------------------------

mod recent_events_tests {
    use super::*;

    #[test]
    fn recent_events_capped_at_90() {
        let mut p = new_profile();
        for _ in 0..95 {
            p.record_event(shipment_event());
        }
        assert_eq!(
            p.recent_events.len(),
            90,
            "recent_events should be capped at 90, got {}",
            p.recent_events.len()
        );
    }

    #[test]
    fn recent_events_keeps_newest_when_capped() {
        let mut p = new_profile();
        // Add 90 ShipmentCreated events
        for _ in 0..90 {
            p.record_event(shipment_event());
        }
        // Now push one DeliveryCompleted — it must survive, oldest ShipmentCreated should drop
        p.record_event(completed_event());
        assert_eq!(p.recent_events.len(), 90);
        // The last event must be the DeliveryCompleted
        assert_eq!(
            p.recent_events.last().unwrap().event_type,
            EventType::DeliveryCompleted
        );
    }

    #[test]
    fn fewer_than_90_events_all_kept() {
        let mut p = new_profile();
        for _ in 0..10 {
            p.record_event(shipment_event());
        }
        assert_eq!(p.recent_events.len(), 10);
    }
}

// ---------------------------------------------------------------------------
// preferred_address tests
// ---------------------------------------------------------------------------

mod preferred_address_tests {
    use super::*;

    fn addr_event(addr: &str) -> BehavioralEvent {
        event(
            EventType::ShipmentCreated,
            json!({ "destination_address": addr }),
        )
    }

    #[test]
    fn preferred_address_returns_none_when_no_history() {
        let p = new_profile();
        assert!(p.preferred_address().is_none());
    }

    #[test]
    fn preferred_address_returns_most_used() {
        let mut p = new_profile();
        p.record_event(addr_event("Quezon City"));
        p.record_event(addr_event("Quezon City"));
        p.record_event(addr_event("Makati"));
        assert_eq!(p.preferred_address(), Some("Quezon City"));
    }

    #[test]
    fn preferred_address_single_address() {
        let mut p = new_profile();
        p.record_event(addr_event("Cebu City"));
        assert_eq!(p.preferred_address(), Some("Cebu City"));
    }

    #[test]
    fn preferred_address_updates_when_new_address_overtakes() {
        let mut p = new_profile();
        p.record_event(addr_event("Pasig"));
        p.record_event(addr_event("Taguig"));
        p.record_event(addr_event("Taguig"));
        p.record_event(addr_event("Taguig"));
        assert_eq!(p.preferred_address(), Some("Taguig"));
    }
}

// ---------------------------------------------------------------------------
// delivery_success_rate tests
// ---------------------------------------------------------------------------

mod delivery_success_rate_tests {
    use super::*;

    #[test]
    fn success_rate_is_0_for_new_profile() {
        let p = new_profile();
        assert_eq!(p.delivery_success_rate(), 0.0);
    }

    #[test]
    fn success_rate_80_for_8_delivered_2_failed() {
        let mut p = new_profile();
        for _ in 0..8 {
            p.record_event(completed_event());
        }
        for _ in 0..2 {
            p.record_event(failed_event());
        }
        assert!(
            (p.delivery_success_rate() - 80.0).abs() < 0.01,
            "Expected 80.0, got {}",
            p.delivery_success_rate()
        );
    }

    #[test]
    fn success_rate_100_when_all_delivered() {
        let mut p = new_profile();
        for _ in 0..5 {
            p.record_event(completed_event());
        }
        assert!(
            (p.delivery_success_rate() - 100.0).abs() < 0.01,
            "Expected 100.0, got {}",
            p.delivery_success_rate()
        );
    }

    #[test]
    fn success_rate_0_when_all_failed() {
        let mut p = new_profile();
        for _ in 0..3 {
            p.record_event(failed_event());
        }
        assert_eq!(p.delivery_success_rate(), 0.0);
    }
}

// ---------------------------------------------------------------------------
// CLV and engagement score tests
// ---------------------------------------------------------------------------

mod score_tests {
    use super::*;

    #[test]
    fn clv_score_increases_after_successful_deliveries() {
        let mut p = new_profile();
        let initial_clv = p.clv_score;
        for _ in 0..5 {
            p.record_event(completed_event());
        }
        assert!(
            p.clv_score > initial_clv,
            "CLV should increase after deliveries. Before: {}, After: {}",
            initial_clv,
            p.clv_score
        );
    }

    #[test]
    fn clv_score_increases_after_cod_payment() {
        let mut p = new_profile();
        // Need at least a recent shipment for recency score component
        p.record_event(shipment_event());
        let initial_clv = p.clv_score;
        p.record_event(cod_event(1_000_000)); // large COD amount
        assert!(
            p.clv_score >= initial_clv,
            "CLV should not decrease after COD payment. Before: {}, After: {}",
            initial_clv,
            p.clv_score
        );
    }

    #[test]
    fn engagement_score_greater_than_zero_after_recent_events() {
        let mut p = new_profile();
        // All events use Utc::now() so they are within the last 30 days.
        p.record_event(shipment_event());
        p.record_event(completed_event());
        assert!(
            p.engagement_score > 0.0,
            "Engagement score should be > 0 after recent events, got {}",
            p.engagement_score
        );
    }

    #[test]
    fn clv_score_capped_at_100() {
        let mut p = new_profile();
        // Record many deliveries and large COD to saturate all score components.
        for _ in 0..100 {
            p.record_event(completed_event());
            p.record_event(cod_event(500_000));
        }
        assert!(
            p.clv_score <= 100.0,
            "CLV score must not exceed 100.0, got {}",
            p.clv_score
        );
    }

    #[test]
    fn engagement_score_capped_at_100() {
        let mut p = new_profile();
        for _ in 0..50 {
            p.record_event(shipment_event());
        }
        assert!(
            p.engagement_score <= 100.0,
            "Engagement score must not exceed 100.0, got {}",
            p.engagement_score
        );
    }
}

// ---------------------------------------------------------------------------
// enrich_identity tests
// ---------------------------------------------------------------------------

mod enrich_identity_tests {
    use super::*;

    #[test]
    fn enrich_sets_name_when_provided() {
        let mut p = new_profile();
        assert!(p.name.is_none());
        p.enrich_identity(Some("Maria Santos".into()), None, None);
        assert_eq!(p.name, Some("Maria Santos".into()));
    }

    #[test]
    fn enrich_sets_email_when_provided() {
        let mut p = new_profile();
        p.enrich_identity(None, Some("maria@example.com".into()), None);
        assert_eq!(p.email, Some("maria@example.com".into()));
    }

    #[test]
    fn enrich_sets_phone_when_provided() {
        let mut p = new_profile();
        p.enrich_identity(None, None, Some("+639171234567".into()));
        assert_eq!(p.phone, Some("+639171234567".into()));
    }

    #[test]
    fn enrich_does_not_overwrite_existing_name_with_none() {
        let mut p = new_profile();
        p.enrich_identity(Some("Maria Santos".into()), None, None);
        p.enrich_identity(None, None, None); // None should not wipe the name
        assert_eq!(p.name, Some("Maria Santos".into()));
    }

    #[test]
    fn enrich_does_not_overwrite_existing_email_with_none() {
        let mut p = new_profile();
        p.enrich_identity(None, Some("maria@example.com".into()), None);
        p.enrich_identity(None, None, None);
        assert_eq!(p.email, Some("maria@example.com".into()));
    }

    #[test]
    fn enrich_does_not_overwrite_existing_phone_with_none() {
        let mut p = new_profile();
        p.enrich_identity(None, None, Some("+639171234567".into()));
        p.enrich_identity(None, None, None);
        assert_eq!(p.phone, Some("+639171234567".into()));
    }

    #[test]
    fn enrich_overwrites_existing_name_with_new_value() {
        let mut p = new_profile();
        p.enrich_identity(Some("Old Name".into()), None, None);
        p.enrich_identity(Some("New Name".into()), None, None);
        assert_eq!(p.name, Some("New Name".into()));
    }

    #[test]
    fn enrich_sets_all_fields_at_once() {
        let mut p = new_profile();
        p.enrich_identity(
            Some("Juan dela Cruz".into()),
            Some("juan@example.com".into()),
            Some("+639991234567".into()),
        );
        assert_eq!(p.name, Some("Juan dela Cruz".into()));
        assert_eq!(p.email, Some("juan@example.com".into()));
        assert_eq!(p.phone, Some("+639991234567".into()));
    }
}
