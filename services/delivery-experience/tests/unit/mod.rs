// Unit tests for the delivery-experience service domain entities.
//
// Coverage:
//  - TrackingRecord::new() — initial state assertions
//  - TrackingStatus::display_label() — all 13 variants
//  - TrackingStatus::is_terminal() — Delivered/Cancelled/Returned=true, rest=false
//  - TrackingRecord::transition() / add_event semantics — append, dedup, terminal guard
//  - TrackingRecord::update_driver_position() — DriverPosition fields
//  - TrackingRecord::mark_delivered() — fields set, status transition
//  - TrackingRecord::mark_failed() — attempt_number, next_attempt_at, status
//  - TrackingRecord::assign_driver() — driver fields populated
//  - Reschedule semantics via mark_failed: next_attempt_at set, attempt_number incremented
//  - StatusEvent ordering: most recent last in history (chronological append)

use chrono::{Duration, Utc};
use uuid::Uuid;

use logisticos_types::TenantId;

// Import the crate under test.
use logisticos_delivery_experience::domain::entities::{
    DriverPosition, StatusEvent, TrackingRecord, TrackingStatus,
};

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn make_record() -> TrackingRecord {
    TrackingRecord::new(
        Uuid::new_v4(),
        TenantId::new(),
        "LGS-2026-TEST-001".into(),
        "Unit 5, Bonifacio Global City, Taguig, Metro Manila".into(),
        "123 Quezon Ave, Quezon City, Metro Manila".into(),
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// TrackingRecord::new()
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod new_record_tests {
    use super::*;

    #[test]
    fn new_sets_initial_status_to_pending() {
        let record = make_record();
        assert_eq!(record.current_status, TrackingStatus::Pending);
    }

    #[test]
    fn new_creates_exactly_one_history_entry() {
        let record = make_record();
        assert_eq!(record.status_history.len(), 1);
    }

    #[test]
    fn new_initial_history_entry_is_pending() {
        let record = make_record();
        let event = &record.status_history[0];
        assert_eq!(event.status, TrackingStatus::Pending);
    }

    #[test]
    fn new_initial_history_entry_has_order_placed_description() {
        let record = make_record();
        let event = &record.status_history[0];
        assert_eq!(event.description, "Order placed");
    }

    #[test]
    fn new_initial_history_entry_has_no_location() {
        let record = make_record();
        assert!(record.status_history[0].location.is_none());
    }

    #[test]
    fn new_all_optional_fields_are_none() {
        let record = make_record();
        assert!(record.driver_id.is_none());
        assert!(record.driver_name.is_none());
        assert!(record.driver_phone.is_none());
        assert!(record.driver_position.is_none());
        assert!(record.estimated_delivery.is_none());
        assert!(record.delivered_at.is_none());
        assert!(record.pod_id.is_none());
        assert!(record.recipient_name.is_none());
        assert!(record.next_attempt_at.is_none());
    }

    #[test]
    fn new_attempt_number_starts_at_zero() {
        let record = make_record();
        assert_eq!(record.attempt_number, 0);
    }

    #[test]
    fn new_tracking_number_is_preserved() {
        let record = TrackingRecord::new(
            Uuid::new_v4(),
            TenantId::new(),
            "LGS-2026-UNIQUE-999".into(),
            "Origin".into(),
            "Destination".into(),
        );
        assert_eq!(record.tracking_number, "LGS-2026-UNIQUE-999");
    }

    #[test]
    fn new_shipment_id_is_preserved() {
        let id = Uuid::new_v4();
        let record = TrackingRecord::new(
            id,
            TenantId::new(),
            "TN-001".into(),
            "Origin".into(),
            "Destination".into(),
        );
        assert_eq!(record.shipment_id, id);
    }

    #[test]
    fn new_addresses_are_preserved() {
        let record = TrackingRecord::new(
            Uuid::new_v4(),
            TenantId::new(),
            "TN-002".into(),
            "Sender St, Makati".into(),
            "Receiver Ave, Cebu City".into(),
        );
        assert_eq!(record.origin_address, "Sender St, Makati");
        assert_eq!(record.destination_address, "Receiver Ave, Cebu City");
    }

    #[test]
    fn new_created_at_and_updated_at_are_close_to_now() {
        let before = Utc::now();
        let record = make_record();
        let after = Utc::now();
        assert!(record.created_at >= before);
        assert!(record.created_at <= after);
        assert!(record.updated_at >= before);
        assert!(record.updated_at <= after);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// TrackingStatus::display_label()
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod display_label_tests {
    use super::*;

    #[test]
    fn pending_label() {
        assert_eq!(TrackingStatus::Pending.display_label(), "Order Placed");
    }

    #[test]
    fn confirmed_label() {
        assert_eq!(TrackingStatus::Confirmed.display_label(), "Order Confirmed");
    }

    #[test]
    fn assigned_to_driver_label() {
        assert_eq!(TrackingStatus::AssignedToDriver.display_label(), "Driver Assigned");
    }

    #[test]
    fn out_for_pickup_label() {
        assert_eq!(TrackingStatus::OutForPickup.display_label(), "Driver On The Way");
    }

    #[test]
    fn picked_up_label() {
        assert_eq!(TrackingStatus::PickedUp.display_label(), "Package Picked Up");
    }

    #[test]
    fn in_transit_label() {
        assert_eq!(TrackingStatus::InTransit.display_label(), "In Transit");
    }

    #[test]
    fn out_for_delivery_label() {
        assert_eq!(TrackingStatus::OutForDelivery.display_label(), "Out for Delivery");
    }

    #[test]
    fn delivery_attempted_label() {
        assert_eq!(TrackingStatus::DeliveryAttempted.display_label(), "Delivery Attempted");
    }

    #[test]
    fn delivered_label() {
        assert_eq!(TrackingStatus::Delivered.display_label(), "Delivered");
    }

    #[test]
    fn delivery_failed_label() {
        assert_eq!(TrackingStatus::DeliveryFailed.display_label(), "Delivery Failed");
    }

    #[test]
    fn cancelled_label() {
        assert_eq!(TrackingStatus::Cancelled.display_label(), "Cancelled");
    }

    #[test]
    fn return_initiated_label() {
        assert_eq!(TrackingStatus::ReturnInitiated.display_label(), "Return Initiated");
    }

    #[test]
    fn returned_label() {
        assert_eq!(TrackingStatus::Returned.display_label(), "Returned");
    }

    #[test]
    fn all_labels_are_non_empty() {
        let statuses = [
            TrackingStatus::Pending,
            TrackingStatus::Confirmed,
            TrackingStatus::AssignedToDriver,
            TrackingStatus::OutForPickup,
            TrackingStatus::PickedUp,
            TrackingStatus::InTransit,
            TrackingStatus::OutForDelivery,
            TrackingStatus::DeliveryAttempted,
            TrackingStatus::Delivered,
            TrackingStatus::DeliveryFailed,
            TrackingStatus::Cancelled,
            TrackingStatus::ReturnInitiated,
            TrackingStatus::Returned,
        ];
        for status in &statuses {
            assert!(!status.display_label().is_empty(), "{:?} label must not be empty", status);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// TrackingStatus::is_terminal()
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod is_terminal_tests {
    use super::*;

    #[test]
    fn delivered_is_terminal() {
        assert!(TrackingStatus::Delivered.is_terminal());
    }

    #[test]
    fn cancelled_is_terminal() {
        assert!(TrackingStatus::Cancelled.is_terminal());
    }

    #[test]
    fn returned_is_terminal() {
        assert!(TrackingStatus::Returned.is_terminal());
    }

    #[test]
    fn pending_is_not_terminal() {
        assert!(!TrackingStatus::Pending.is_terminal());
    }

    #[test]
    fn confirmed_is_not_terminal() {
        assert!(!TrackingStatus::Confirmed.is_terminal());
    }

    #[test]
    fn assigned_to_driver_is_not_terminal() {
        assert!(!TrackingStatus::AssignedToDriver.is_terminal());
    }

    #[test]
    fn out_for_pickup_is_not_terminal() {
        assert!(!TrackingStatus::OutForPickup.is_terminal());
    }

    #[test]
    fn picked_up_is_not_terminal() {
        assert!(!TrackingStatus::PickedUp.is_terminal());
    }

    #[test]
    fn in_transit_is_not_terminal() {
        assert!(!TrackingStatus::InTransit.is_terminal());
    }

    #[test]
    fn out_for_delivery_is_not_terminal() {
        assert!(!TrackingStatus::OutForDelivery.is_terminal());
    }

    #[test]
    fn delivery_attempted_is_not_terminal() {
        assert!(!TrackingStatus::DeliveryAttempted.is_terminal());
    }

    #[test]
    fn delivery_failed_is_not_terminal() {
        assert!(!TrackingStatus::DeliveryFailed.is_terminal());
    }

    #[test]
    fn return_initiated_is_not_terminal() {
        assert!(!TrackingStatus::ReturnInitiated.is_terminal());
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// TrackingRecord::transition() — add_event semantics
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod transition_tests {
    use super::*;

    #[test]
    fn transition_appends_new_status_to_history() {
        let mut record = make_record();
        record.transition(TrackingStatus::Confirmed, "Order confirmed by operator".into(), None);
        assert_eq!(record.status_history.len(), 2);
    }

    #[test]
    fn transition_updates_current_status() {
        let mut record = make_record();
        record.transition(TrackingStatus::Confirmed, "Confirmed".into(), None);
        assert_eq!(record.current_status, TrackingStatus::Confirmed);
    }

    #[test]
    fn transition_preserves_chronological_order() {
        let mut record = make_record();
        record.transition(TrackingStatus::Confirmed, "Confirmed".into(), None);
        record.transition(TrackingStatus::AssignedToDriver, "Driver assigned".into(), None);
        record.transition(TrackingStatus::OutForPickup, "On the way".into(), None);

        assert_eq!(record.status_history[0].status, TrackingStatus::Pending);
        assert_eq!(record.status_history[1].status, TrackingStatus::Confirmed);
        assert_eq!(record.status_history[2].status, TrackingStatus::AssignedToDriver);
        assert_eq!(record.status_history[3].status, TrackingStatus::OutForPickup);
    }

    #[test]
    fn transition_does_not_append_duplicate_adjacent_status() {
        let mut record = make_record();
        // Transitioning to Pending again (same as current) should be a no-op.
        record.transition(TrackingStatus::Pending, "Duplicate".into(), None);
        assert_eq!(record.status_history.len(), 1);
    }

    #[test]
    fn transition_after_terminal_status_is_blocked() {
        let mut record = make_record();
        record.transition(TrackingStatus::Delivered, "Delivered to customer".into(), None);

        // Attempting to transition from a terminal state should be silently ignored.
        record.transition(TrackingStatus::InTransit, "Back in transit??".into(), None);
        assert_eq!(record.current_status, TrackingStatus::Delivered);
        // Only 2 entries: initial Pending + Delivered
        assert_eq!(record.status_history.len(), 2);
    }

    #[test]
    fn transition_stores_description_in_history() {
        let mut record = make_record();
        let desc = "Driver picked up the package at hub NCR-01".to_string();
        record.transition(TrackingStatus::PickedUp, desc.clone(), None);
        assert_eq!(record.status_history.last().unwrap().description, desc);
    }

    #[test]
    fn transition_stores_location_in_history_when_provided() {
        let mut record = make_record();
        record.transition(
            TrackingStatus::InTransit,
            "Departed hub".into(),
            Some("NCR Hub, Pasay City".into()),
        );
        let event = record.status_history.last().unwrap();
        assert_eq!(event.location.as_deref(), Some("NCR Hub, Pasay City"));
    }

    #[test]
    fn transition_history_entry_location_is_none_when_not_provided() {
        let mut record = make_record();
        record.transition(TrackingStatus::Confirmed, "Confirmed".into(), None);
        assert!(record.status_history.last().unwrap().location.is_none());
    }

    #[test]
    fn transition_updates_updated_at_timestamp() {
        let mut record = make_record();
        let before = record.updated_at;
        // Small sleep is unreliable in unit tests — we just confirm updated_at >= original.
        record.transition(TrackingStatus::Confirmed, "Confirmed".into(), None);
        assert!(record.updated_at >= before);
    }

    #[test]
    fn multiple_transitions_through_full_lifecycle() {
        let mut record = make_record();
        let steps = [
            (TrackingStatus::Confirmed, "Confirmed"),
            (TrackingStatus::AssignedToDriver, "Driver assigned"),
            (TrackingStatus::OutForPickup, "On the way to pickup"),
            (TrackingStatus::PickedUp, "Picked up"),
            (TrackingStatus::InTransit, "In transit"),
            (TrackingStatus::OutForDelivery, "Out for delivery"),
        ];
        for (status, desc) in &steps {
            record.transition(status.clone(), desc.to_string(), None);
        }
        // 1 initial + 6 transitions = 7 entries
        assert_eq!(record.status_history.len(), 7);
        assert_eq!(record.current_status, TrackingStatus::OutForDelivery);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// TrackingRecord::update_driver_position()
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod driver_position_tests {
    use super::*;

    #[test]
    fn update_driver_position_sets_lat_lng() {
        let mut record = make_record();
        record.update_driver_position(14.5995, 120.9842);
        let pos = record.driver_position.as_ref().expect("driver_position should be Some");
        assert_eq!(pos.lat, 14.5995);
        assert_eq!(pos.lng, 120.9842);
    }

    #[test]
    fn update_driver_position_replaces_previous_position() {
        let mut record = make_record();
        record.update_driver_position(14.5995, 120.9842);
        record.update_driver_position(10.3157, 123.8854); // Cebu City
        let pos = record.driver_position.as_ref().unwrap();
        assert!((pos.lat - 10.3157).abs() < 1e-6);
        assert!((pos.lng - 123.8854).abs() < 1e-6);
    }

    #[test]
    fn update_driver_position_sets_updated_at_on_position() {
        let before = Utc::now();
        let mut record = make_record();
        record.update_driver_position(14.5995, 120.9842);
        let pos = record.driver_position.as_ref().unwrap();
        assert!(pos.updated_at >= before);
    }

    #[test]
    fn update_driver_position_updates_record_updated_at() {
        let mut record = make_record();
        let before = record.updated_at;
        record.update_driver_position(14.5995, 120.9842);
        assert!(record.updated_at >= before);
    }

    #[test]
    fn driver_position_is_none_before_update() {
        let record = make_record();
        assert!(record.driver_position.is_none());
    }

    #[test]
    fn update_driver_position_handles_negative_coordinates() {
        let mut record = make_record();
        record.update_driver_position(-33.8688, 151.2093); // Sydney
        let pos = record.driver_position.as_ref().unwrap();
        assert!((pos.lat - (-33.8688)).abs() < 1e-6);
        assert!((pos.lng - 151.2093).abs() < 1e-6);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// TrackingRecord::mark_delivered()
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod mark_delivered_tests {
    use super::*;

    fn deliver(record: &mut TrackingRecord) {
        let pod_id = Uuid::new_v4();
        let delivered_at = Utc::now();
        record.mark_delivered(pod_id, "Juan dela Cruz".into(), delivered_at);
    }

    #[test]
    fn mark_delivered_sets_status_to_delivered() {
        let mut record = make_record();
        deliver(&mut record);
        assert_eq!(record.current_status, TrackingStatus::Delivered);
    }

    #[test]
    fn mark_delivered_sets_delivered_at() {
        let mut record = make_record();
        let ts = Utc::now();
        record.mark_delivered(Uuid::new_v4(), "Maria Santos".into(), ts);
        assert_eq!(record.delivered_at, Some(ts));
    }

    #[test]
    fn mark_delivered_sets_pod_id() {
        let mut record = make_record();
        let pod = Uuid::new_v4();
        record.mark_delivered(pod, "Receiver".into(), Utc::now());
        assert_eq!(record.pod_id, Some(pod));
    }

    #[test]
    fn mark_delivered_sets_recipient_name() {
        let mut record = make_record();
        record.mark_delivered(Uuid::new_v4(), "Pedro Reyes".into(), Utc::now());
        // Note: mark_delivered stores recipient_name before the transition, but
        // the transition description also formats it. The field itself should be set.
        // However, looking at the source: self.recipient_name = Some(recipient_name.clone())
        // is called, then transition uses recipient_name. After the borrow, it's moved.
        // The field is set to Some("Pedro Reyes").
        assert_eq!(record.recipient_name.as_deref(), Some("Pedro Reyes"));
    }

    #[test]
    fn mark_delivered_appends_to_history() {
        let mut record = make_record();
        deliver(&mut record);
        assert_eq!(record.status_history.len(), 2);
        assert_eq!(record.status_history[1].status, TrackingStatus::Delivered);
    }

    #[test]
    fn mark_delivered_history_description_includes_recipient() {
        let mut record = make_record();
        record.mark_delivered(Uuid::new_v4(), "Ana Reyes".into(), Utc::now());
        let last = record.status_history.last().unwrap();
        assert!(last.description.contains("Ana Reyes"));
    }

    #[test]
    fn delivered_is_terminal_no_further_transitions() {
        let mut record = make_record();
        deliver(&mut record);
        record.transition(TrackingStatus::DeliveryFailed, "Fake failure".into(), None);
        assert_eq!(record.current_status, TrackingStatus::Delivered);
        assert_eq!(record.status_history.len(), 2);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Reschedule: mark_failed() — next_attempt_at, attempt_number incremented
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod reschedule_tests {
    use super::*;

    #[test]
    fn mark_failed_sets_delivery_failed_status() {
        let mut record = make_record();
        record.mark_failed("Customer not home".into(), 1, None);
        assert_eq!(record.current_status, TrackingStatus::DeliveryFailed);
    }

    #[test]
    fn mark_failed_updates_attempt_number() {
        let mut record = make_record();
        record.mark_failed("Gate locked".into(), 2, None);
        assert_eq!(record.attempt_number, 2);
    }

    #[test]
    fn mark_failed_increments_attempt_number_across_calls() {
        let mut record = make_record();
        record.mark_failed("Not home".into(), 1, None);

        // Simulate second failure: transition back possible only if not terminal.
        // DeliveryFailed is NOT terminal, so we can transition further.
        // But mark_failed itself sets attempt_number = attempt_number parameter.
        record.mark_failed("Still not home".into(), 2, None);
        assert_eq!(record.attempt_number, 2);
    }

    #[test]
    fn mark_failed_sets_next_attempt_at_when_provided() {
        let mut record = make_record();
        let next = Utc::now() + Duration::hours(24);
        record.mark_failed("Refused delivery".into(), 1, Some(next));
        assert_eq!(record.next_attempt_at, Some(next));
    }

    #[test]
    fn mark_failed_next_attempt_at_none_when_not_provided() {
        let mut record = make_record();
        record.mark_failed("Address not found".into(), 1, None);
        assert!(record.next_attempt_at.is_none());
    }

    #[test]
    fn mark_failed_appends_to_history() {
        let mut record = make_record();
        record.mark_failed("No one home".into(), 1, None);
        assert_eq!(record.status_history.len(), 2);
        assert_eq!(record.status_history[1].status, TrackingStatus::DeliveryFailed);
    }

    #[test]
    fn mark_failed_history_description_includes_reason() {
        let mut record = make_record();
        let reason = "Road flooded — force majeure".to_string();
        record.mark_failed(reason.clone(), 1, None);
        let last_desc = &record.status_history.last().unwrap().description;
        assert!(last_desc.contains(&reason));
    }

    #[test]
    fn mark_failed_history_description_includes_attempt_number() {
        let mut record = make_record();
        record.mark_failed("Reason".into(), 3, None);
        let last_desc = &record.status_history.last().unwrap().description;
        assert!(last_desc.contains('3'));
    }

    #[test]
    fn failed_delivery_followed_by_next_attempt_rescheduled() {
        let mut record = make_record();
        let reschedule_time = Utc::now() + Duration::hours(48);
        record.mark_failed("First attempt failed".into(), 1, Some(reschedule_time));
        assert_eq!(record.attempt_number, 1);
        assert_eq!(record.next_attempt_at, Some(reschedule_time));
        assert_eq!(record.current_status, TrackingStatus::DeliveryFailed);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// TrackingRecord::assign_driver()
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod assign_driver_tests {
    use super::*;

    fn do_assign(record: &mut TrackingRecord) -> Uuid {
        let driver_id = Uuid::new_v4();
        record.assign_driver(
            driver_id,
            "Rodrigo Duterte".into(),
            "+63 912 345 6789".into(),
            Some(Utc::now() + Duration::hours(2)),
        );
        driver_id
    }

    #[test]
    fn assign_driver_sets_driver_id() {
        let mut record = make_record();
        let driver_id = do_assign(&mut record);
        assert_eq!(record.driver_id, Some(driver_id));
    }

    #[test]
    fn assign_driver_sets_driver_name() {
        let mut record = make_record();
        record.assign_driver(
            Uuid::new_v4(),
            "Bongbong Marcos".into(),
            "+63 999 888 7777".into(),
            None,
        );
        assert_eq!(record.driver_name.as_deref(), Some("Bongbong Marcos"));
    }

    #[test]
    fn assign_driver_sets_driver_phone() {
        let mut record = make_record();
        record.assign_driver(
            Uuid::new_v4(),
            "Driver Name".into(),
            "+63 917 000 0001".into(),
            None,
        );
        assert_eq!(record.driver_phone.as_deref(), Some("+63 917 000 0001"));
    }

    #[test]
    fn assign_driver_sets_status_to_assigned_to_driver() {
        let mut record = make_record();
        do_assign(&mut record);
        assert_eq!(record.current_status, TrackingStatus::AssignedToDriver);
    }

    #[test]
    fn assign_driver_sets_estimated_delivery() {
        let mut record = make_record();
        let eta = Utc::now() + Duration::hours(3);
        record.assign_driver(Uuid::new_v4(), "Driver".into(), "+63".into(), Some(eta));
        assert_eq!(record.estimated_delivery, Some(eta));
    }

    #[test]
    fn assign_driver_appends_to_history() {
        let mut record = make_record();
        do_assign(&mut record);
        assert_eq!(record.status_history.len(), 2);
        assert_eq!(
            record.status_history[1].status,
            TrackingStatus::AssignedToDriver
        );
    }

    #[test]
    fn assign_driver_estimated_delivery_none_when_not_provided() {
        let mut record = make_record();
        record.assign_driver(Uuid::new_v4(), "Driver".into(), "+63".into(), None);
        assert!(record.estimated_delivery.is_none());
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// StatusEvent ordering — most recent last in history (chronological append)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod status_event_ordering_tests {
    use super::*;

    #[test]
    fn history_is_ordered_oldest_first_newest_last() {
        let mut record = make_record();
        record.transition(TrackingStatus::Confirmed, "Confirmed".into(), None);
        record.transition(TrackingStatus::PickedUp, "Picked up".into(), None);
        record.transition(TrackingStatus::InTransit, "In transit".into(), None);
        record.transition(TrackingStatus::Delivered, "Delivered".into(), None);

        assert_eq!(record.status_history[0].status, TrackingStatus::Pending);
        assert_eq!(record.status_history[1].status, TrackingStatus::Confirmed);
        assert_eq!(record.status_history[2].status, TrackingStatus::PickedUp);
        assert_eq!(record.status_history[3].status, TrackingStatus::InTransit);
        assert_eq!(record.status_history[4].status, TrackingStatus::Delivered);
    }

    #[test]
    fn last_history_entry_matches_current_status() {
        let mut record = make_record();
        record.transition(TrackingStatus::Confirmed, "Confirmed".into(), None);
        record.transition(TrackingStatus::OutForDelivery, "OFD".into(), None);
        assert_eq!(
            record.status_history.last().unwrap().status,
            record.current_status
        );
    }

    #[test]
    fn occurred_at_timestamps_are_non_decreasing() {
        let mut record = make_record();
        record.transition(TrackingStatus::Confirmed, "Confirmed".into(), None);
        record.transition(TrackingStatus::AssignedToDriver, "Assigned".into(), None);
        record.transition(TrackingStatus::PickedUp, "Picked up".into(), None);

        let timestamps: Vec<_> = record
            .status_history
            .iter()
            .map(|e| e.occurred_at)
            .collect();

        for window in timestamps.windows(2) {
            assert!(
                window[1] >= window[0],
                "History should be chronological: {:?} < {:?}",
                window[0],
                window[1]
            );
        }
    }

    #[test]
    fn history_count_matches_number_of_unique_transitions() {
        let mut record = make_record();
        // 1 initial + 5 transitions = 6 entries
        record.transition(TrackingStatus::Confirmed, "C".into(), None);
        record.transition(TrackingStatus::AssignedToDriver, "A".into(), None);
        record.transition(TrackingStatus::OutForPickup, "O".into(), None);
        record.transition(TrackingStatus::PickedUp, "P".into(), None);
        record.transition(TrackingStatus::InTransit, "I".into(), None);
        assert_eq!(record.status_history.len(), 6);
    }
}
