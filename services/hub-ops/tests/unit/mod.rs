// Unit tests for the hub-ops service domain layer.
//
// Tests exercise Hub and ParcelInduction business rules in isolation.
// No database, no HTTP, no Kafka — pure domain logic exercised directly
// against entity methods and field values.

use logisticos_hub_ops::domain::entities::{
    Hub, HubId, InductionId, InductionStatus, ParcelInduction,
};
use logisticos_types::TenantId;
use uuid::Uuid;

// ─────────────────────────────────────────────────────────────────────────────
// Test helpers
// ─────────────────────────────────────────────────────────────────────────────

fn make_tenant() -> TenantId {
    TenantId::new()
}

fn make_hub(capacity: u32) -> Hub {
    Hub::new(
        make_tenant(),
        "Manila Sorting Hub".into(),
        "123 Buendia Ave, Makati City".into(),
        14.5547,
        121.0244,
        capacity,
    )
}

fn make_induction(hub: &Hub) -> ParcelInduction {
    ParcelInduction::new(
        hub.id.clone(),
        hub.tenant_id.clone(),
        Uuid::new_v4(),
        "LSPH0011223344".into(),
        Some(Uuid::new_v4()),
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Hub::new() — correct defaults
// ─────────────────────────────────────────────────────────────────────────────

mod hub_new {
    use super::*;

    #[test]
    fn new_hub_starts_with_zero_current_load() {
        let hub = make_hub(100);
        assert_eq!(hub.current_load, 0, "New hub must start with current_load = 0");
    }

    #[test]
    fn new_hub_is_active_by_default() {
        let hub = make_hub(100);
        assert!(hub.is_active, "New hub must be active by default");
    }

    #[test]
    fn new_hub_has_empty_serving_zones() {
        let hub = make_hub(100);
        assert!(hub.serving_zones.is_empty(), "New hub must have no serving zones");
    }

    #[test]
    fn new_hub_stores_name_and_address() {
        let hub = Hub::new(
            make_tenant(),
            "Cebu Distribution Center".into(),
            "456 Osmeña Blvd, Cebu City".into(),
            10.3157,
            123.8854,
            200,
        );
        assert_eq!(hub.name, "Cebu Distribution Center");
        assert_eq!(hub.address, "456 Osmeña Blvd, Cebu City");
    }

    #[test]
    fn new_hub_stores_coordinates() {
        let hub = Hub::new(
            make_tenant(),
            "Davao Hub".into(),
            "789 MacArthur Hwy, Davao City".into(),
            7.1907,
            125.4553,
            150,
        );
        assert!((hub.lat - 7.1907).abs() < 1e-6, "Latitude must be stored accurately");
        assert!((hub.lng - 125.4553).abs() < 1e-6, "Longitude must be stored accurately");
    }

    #[test]
    fn new_hub_stores_capacity() {
        let hub = make_hub(500);
        assert_eq!(hub.capacity, 500);
    }

    #[test]
    fn new_hub_has_unique_id() {
        let a = make_hub(100);
        let b = make_hub(100);
        assert_ne!(a.id.inner(), b.id.inner(), "Each hub must get a unique ID");
    }

    #[test]
    fn new_hub_created_at_equals_updated_at() {
        let hub = make_hub(100);
        // Both timestamps are set from the same `now` call in Hub::new
        assert_eq!(
            hub.created_at.timestamp(),
            hub.updated_at.timestamp(),
            "created_at and updated_at must be equal on a newly created hub"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Hub::induct_parcel()
// ─────────────────────────────────────────────────────────────────────────────

mod hub_induct_parcel {
    use super::*;

    #[test]
    fn induct_increments_current_load() {
        let mut hub = make_hub(10);
        hub.induct_parcel().expect("induct must succeed when below capacity");
        assert_eq!(hub.current_load, 1);
    }

    #[test]
    fn multiple_inductions_accumulate_load() {
        let mut hub = make_hub(10);
        for _ in 0..5 {
            hub.induct_parcel().expect("each induction must succeed");
        }
        assert_eq!(hub.current_load, 5);
    }

    #[test]
    fn induct_returns_ok_when_below_capacity() {
        let mut hub = make_hub(5);
        assert!(hub.induct_parcel().is_ok());
    }

    #[test]
    fn induct_returns_err_when_at_capacity() {
        let mut hub = make_hub(2);
        hub.induct_parcel().unwrap();
        hub.induct_parcel().unwrap();
        // Now at capacity: current_load == capacity == 2
        let result = hub.induct_parcel();
        assert!(result.is_err(), "Inducting into a full hub must return an error");
    }

    #[test]
    fn induct_error_message_contains_hub_name() {
        let mut hub = Hub::new(
            make_tenant(),
            "Quezon Hub".into(),
            "1 Commonwealth Ave".into(),
            14.6507,
            121.0490,
            1,
        );
        hub.induct_parcel().unwrap(); // fill it up
        let err = hub.induct_parcel().unwrap_err();
        assert!(
            err.to_string().contains("Quezon Hub"),
            "Error message must reference the hub name, got: {}",
            err
        );
    }

    #[test]
    fn induct_does_not_increment_load_on_error() {
        let mut hub = make_hub(1);
        hub.induct_parcel().unwrap(); // at capacity
        let _ = hub.induct_parcel(); // must fail
        assert_eq!(hub.current_load, 1, "current_load must not change on a failed induction");
    }

    #[test]
    fn capacity_zero_hub_always_errors_on_induct() {
        let mut hub = make_hub(0);
        let result = hub.induct_parcel();
        assert!(result.is_err(), "A hub with capacity=0 must immediately error on induction");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Hub::dispatch_parcel()
// ─────────────────────────────────────────────────────────────────────────────

mod hub_dispatch_parcel {
    use super::*;

    #[test]
    fn dispatch_decrements_current_load() {
        let mut hub = make_hub(10);
        hub.induct_parcel().unwrap();
        hub.induct_parcel().unwrap();
        hub.dispatch_parcel();
        assert_eq!(hub.current_load, 1);
    }

    #[test]
    fn dispatch_on_zero_load_is_safe() {
        let mut hub = make_hub(10);
        // No panics or underflow when dispatching from empty hub
        hub.dispatch_parcel();
        assert_eq!(hub.current_load, 0, "Dispatching from empty hub must keep load at 0");
    }

    #[test]
    fn dispatch_after_induct_returns_to_zero() {
        let mut hub = make_hub(5);
        hub.induct_parcel().unwrap();
        hub.dispatch_parcel();
        assert_eq!(hub.current_load, 0);
    }

    #[test]
    fn multiple_dispatches_do_not_underflow() {
        let mut hub = make_hub(5);
        hub.induct_parcel().unwrap();
        hub.dispatch_parcel();
        hub.dispatch_parcel(); // extra dispatch — must not underflow
        assert_eq!(hub.current_load, 0, "current_load must never go below 0");
    }

    #[test]
    fn dispatch_after_full_hub_opens_capacity() {
        let mut hub = make_hub(1);
        hub.induct_parcel().unwrap(); // full
        hub.dispatch_parcel();       // freed
        // Should be able to induct again
        assert!(hub.induct_parcel().is_ok(), "Capacity must be available after dispatch");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Hub::capacity_pct()
// ─────────────────────────────────────────────────────────────────────────────

mod hub_capacity_pct {
    use super::*;

    #[test]
    fn capacity_pct_is_zero_when_empty() {
        let hub = make_hub(100);
        assert_eq!(hub.capacity_pct(), 0.0);
    }

    #[test]
    fn capacity_pct_is_100_when_full() {
        let mut hub = make_hub(2);
        hub.induct_parcel().unwrap();
        hub.induct_parcel().unwrap();
        assert!(
            (hub.capacity_pct() - 100.0).abs() < 0.01,
            "Full hub must report 100% capacity"
        );
    }

    #[test]
    fn capacity_pct_is_50_at_half_load() {
        let mut hub = make_hub(4);
        hub.induct_parcel().unwrap();
        hub.induct_parcel().unwrap();
        assert!(
            (hub.capacity_pct() - 50.0).abs() < 0.01,
            "Half-loaded hub must report 50.0%, got {}",
            hub.capacity_pct()
        );
    }

    #[test]
    fn capacity_pct_correct_at_25_percent() {
        let mut hub = make_hub(4);
        hub.induct_parcel().unwrap();
        assert!(
            (hub.capacity_pct() - 25.0).abs() < 0.01,
            "One parcel in a 4-slot hub must report 25.0%, got {}",
            hub.capacity_pct()
        );
    }

    #[test]
    fn capacity_pct_returns_zero_for_zero_capacity_hub() {
        // Guard against division by zero
        let hub = make_hub(0);
        assert_eq!(
            hub.capacity_pct(), 0.0,
            "Hub with capacity=0 must return 0.0 pct (no division by zero)"
        );
    }

    #[test]
    fn capacity_pct_handles_large_hub() {
        let mut hub = make_hub(1000);
        for _ in 0..333 {
            hub.induct_parcel().unwrap();
        }
        let pct = hub.capacity_pct();
        assert!(
            (pct - 33.3).abs() < 0.1,
            "333/1000 must be approximately 33.3%, got {}",
            pct
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Hub::is_over_capacity()
// ─────────────────────────────────────────────────────────────────────────────

mod hub_is_over_capacity {
    use super::*;

    #[test]
    fn not_over_capacity_when_empty() {
        let hub = make_hub(10);
        assert!(!hub.is_over_capacity());
    }

    #[test]
    fn not_over_capacity_when_at_one_below_limit() {
        let mut hub = make_hub(3);
        hub.induct_parcel().unwrap();
        hub.induct_parcel().unwrap();
        // current_load=2, capacity=3 → not over
        assert!(!hub.is_over_capacity());
    }

    #[test]
    fn is_over_capacity_when_load_equals_capacity() {
        let mut hub = make_hub(2);
        hub.induct_parcel().unwrap();
        hub.induct_parcel().unwrap();
        // current_load == capacity → at capacity (is_over_capacity uses >=)
        assert!(hub.is_over_capacity(), "Hub at exactly full capacity must report is_over_capacity");
    }

    #[test]
    fn capacity_one_hub_not_over_when_empty() {
        let hub = make_hub(1);
        assert!(!hub.is_over_capacity());
    }

    #[test]
    fn capacity_one_hub_is_over_after_induction() {
        let mut hub = make_hub(1);
        hub.induct_parcel().unwrap();
        assert!(hub.is_over_capacity(), "Single-capacity hub must be over capacity after one induction");
    }

    #[test]
    fn capacity_zero_hub_is_always_over_capacity() {
        let hub = make_hub(0);
        // current_load=0, capacity=0 → 0 >= 0 → true
        assert!(
            hub.is_over_capacity(),
            "A hub with capacity=0 must always report over capacity"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ParcelInduction::new()
// ─────────────────────────────────────────────────────────────────────────────

mod parcel_induction_new {
    use super::*;

    #[test]
    fn new_induction_status_is_inducted() {
        let hub = make_hub(100);
        let induction = make_induction(&hub);
        assert_eq!(
            induction.status,
            InductionStatus::Inducted,
            "New induction must start with status=Inducted"
        );
    }

    #[test]
    fn new_induction_has_no_zone() {
        let hub = make_hub(100);
        let induction = make_induction(&hub);
        assert!(induction.zone.is_none(), "Newly inducted parcel must have no zone assigned");
    }

    #[test]
    fn new_induction_has_no_bay() {
        let hub = make_hub(100);
        let induction = make_induction(&hub);
        assert!(induction.bay.is_none(), "Newly inducted parcel must have no bay assigned");
    }

    #[test]
    fn new_induction_has_no_sorted_at() {
        let hub = make_hub(100);
        let induction = make_induction(&hub);
        assert!(induction.sorted_at.is_none());
    }

    #[test]
    fn new_induction_has_no_dispatched_at() {
        let hub = make_hub(100);
        let induction = make_induction(&hub);
        assert!(induction.dispatched_at.is_none());
    }

    #[test]
    fn new_induction_stores_tracking_number() {
        let hub = make_hub(100);
        let induction = ParcelInduction::new(
            hub.id.clone(),
            hub.tenant_id.clone(),
            Uuid::new_v4(),
            "LSPH0099887766".into(),
            None,
        );
        assert_eq!(induction.tracking_number, "LSPH0099887766");
    }

    #[test]
    fn new_induction_stores_shipment_id() {
        let hub = make_hub(100);
        let shipment_id = Uuid::new_v4();
        let induction = ParcelInduction::new(
            hub.id.clone(),
            hub.tenant_id.clone(),
            shipment_id,
            "LSPH0012345678".into(),
            None,
        );
        assert_eq!(induction.shipment_id, shipment_id);
    }

    #[test]
    fn new_induction_stores_inducted_by() {
        let hub = make_hub(100);
        let staff_id = Uuid::new_v4();
        let induction = ParcelInduction::new(
            hub.id.clone(),
            hub.tenant_id.clone(),
            Uuid::new_v4(),
            "LSPH0012345678".into(),
            Some(staff_id),
        );
        assert_eq!(induction.inducted_by, Some(staff_id));
    }

    #[test]
    fn new_induction_without_inducted_by_is_none() {
        let hub = make_hub(100);
        let induction = ParcelInduction::new(
            hub.id.clone(),
            hub.tenant_id.clone(),
            Uuid::new_v4(),
            "LSPH0012345678".into(),
            None,
        );
        assert!(induction.inducted_by.is_none());
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ParcelInduction::sort_to()
// ─────────────────────────────────────────────────────────────────────────────

mod parcel_induction_sort {
    use super::*;

    #[test]
    fn sort_sets_status_to_sorted() {
        let hub = make_hub(100);
        let mut induction = make_induction(&hub);
        induction.sort_to("ZONE-A".into(), "BAY-01".into());
        assert_eq!(induction.status, InductionStatus::Sorted);
    }

    #[test]
    fn sort_sets_zone() {
        let hub = make_hub(100);
        let mut induction = make_induction(&hub);
        induction.sort_to("ZONE-B".into(), "BAY-02".into());
        assert_eq!(induction.zone, Some("ZONE-B".to_string()));
    }

    #[test]
    fn sort_sets_bay() {
        let hub = make_hub(100);
        let mut induction = make_induction(&hub);
        induction.sort_to("ZONE-C".into(), "BAY-03".into());
        assert_eq!(induction.bay, Some("BAY-03".to_string()));
    }

    #[test]
    fn sort_sets_sorted_at_timestamp() {
        let hub = make_hub(100);
        let mut induction = make_induction(&hub);
        induction.sort_to("ZONE-A".into(), "BAY-01".into());
        assert!(
            induction.sorted_at.is_some(),
            "sorted_at must be set after sort_to is called"
        );
    }

    #[test]
    fn sort_does_not_set_dispatched_at() {
        let hub = make_hub(100);
        let mut induction = make_induction(&hub);
        induction.sort_to("ZONE-A".into(), "BAY-01".into());
        assert!(induction.dispatched_at.is_none(), "sort_to must not set dispatched_at");
    }

    #[test]
    fn sorting_an_inducted_parcel_is_valid() {
        let hub = make_hub(100);
        let mut induction = make_induction(&hub);
        // status=Inducted — sort_to must work without panic
        induction.sort_to("ZONE-A".into(), "BAY-01".into());
        assert_eq!(induction.status, InductionStatus::Sorted);
    }

    // NOTE: method would be on domain entity — sort_to has no guard preventing
    // sorting an already-dispatched parcel in the current domain entity.
    // The following test documents expected behaviour via direct field check.
    #[test]
    fn sort_overwrites_existing_zone_and_bay() {
        let hub = make_hub(100);
        let mut induction = make_induction(&hub);
        induction.sort_to("ZONE-A".into(), "BAY-01".into());
        induction.sort_to("ZONE-B".into(), "BAY-05".into()); // re-sort
        assert_eq!(induction.zone, Some("ZONE-B".to_string()));
        assert_eq!(induction.bay, Some("BAY-05".to_string()));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ParcelInduction::dispatch()
// ─────────────────────────────────────────────────────────────────────────────

mod parcel_induction_dispatch {
    use super::*;

    fn sorted_induction(hub: &Hub) -> ParcelInduction {
        let mut ind = make_induction(hub);
        ind.sort_to("ZONE-A".into(), "BAY-01".into());
        ind
    }

    #[test]
    fn dispatch_sets_status_to_dispatched() {
        let hub = make_hub(100);
        let mut induction = sorted_induction(&hub);
        induction.dispatch();
        assert_eq!(induction.status, InductionStatus::Dispatched);
    }

    #[test]
    fn dispatch_sets_dispatched_at_timestamp() {
        let hub = make_hub(100);
        let mut induction = sorted_induction(&hub);
        induction.dispatch();
        assert!(
            induction.dispatched_at.is_some(),
            "dispatched_at must be set after dispatch() is called"
        );
    }

    #[test]
    fn dispatch_preserves_zone_and_bay() {
        let hub = make_hub(100);
        let mut induction = sorted_induction(&hub);
        induction.dispatch();
        assert_eq!(induction.zone, Some("ZONE-A".to_string()));
        assert_eq!(induction.bay, Some("BAY-01".to_string()));
    }

    #[test]
    fn dispatch_preserves_sorted_at() {
        let hub = make_hub(100);
        let mut induction = sorted_induction(&hub);
        let sorted_at = induction.sorted_at;
        induction.dispatch();
        assert_eq!(induction.sorted_at, sorted_at, "sorted_at must not change when dispatching");
    }

    // NOTE: the domain entity's dispatch() does not guard against status transitions;
    // the service layer enforces that only sorted parcels are dispatched.
    // The following test documents that calling dispatch() on an Inducted parcel
    // (without sorting first) still sets the status field directly.
    #[test]
    fn dispatch_from_inducted_state_sets_dispatched_without_guard() {
        let hub = make_hub(100);
        let mut induction = make_induction(&hub); // status=Inducted, never sorted
        induction.dispatch();
        // The entity itself does not enforce the Sorted→Dispatched constraint —
        // that constraint lives in HubService::dispatch_parcel.
        assert_eq!(
            induction.status,
            InductionStatus::Dispatched,
            "dispatch() on the entity always transitions to Dispatched regardless of prior status"
        );
    }

    #[test]
    fn dispatch_sets_dispatched_at_after_inducted_at() {
        let hub = make_hub(100);
        let mut induction = sorted_induction(&hub);
        induction.dispatch();
        assert!(
            induction.dispatched_at.unwrap() >= induction.inducted_at,
            "dispatched_at must not be before inducted_at"
        );
    }
}
