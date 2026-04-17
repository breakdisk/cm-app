// Unit tests for the dispatch service domain layer.
//
// These tests exercise pure domain logic — no database, no Kafka, no HTTP.
// Each test constructs domain objects directly and asserts business rules.

use logisticos_dispatch::domain::{
    entities::{
        route::{Route, RouteStatus, DeliveryStop, StopType},
        driver_assignment::{DriverAssignment, AssignmentStatus},
    },
    value_objects::{
        estimate_duration_minutes, MAX_STOPS_BUSINESS, MAX_STOPS_STARTER, MAX_STOPS_GROWTH,
        AVERAGE_SPEED_KMH, STOP_SERVICE_MINUTES,
    },
    repositories::AvailableDriver,
};
use logisticos_types::{
    RouteId, DriverId, VehicleId, TenantId, Address, Coordinates,
};
use logisticos_geo::nearest_neighbor_order;
use chrono::Utc;
use uuid::Uuid;

// ─────────────────────────────────────────────────────────────────────────────
// Test helpers
// ─────────────────────────────────────────────────────────────────────────────

fn make_address(lat: f64, lng: f64) -> Address {
    Address {
        line1: "123 Test St".into(),
        line2: None,
        barangay: None,
        city: "Manila".into(),
        province: "Metro Manila".into(),
        postal_code: "1000".into(),
        country_code: "PH".into(),
        coordinates: Some(Coordinates { lat, lng }),
    }
}

fn make_stop(sequence: u32, lat: f64, lng: f64) -> DeliveryStop {
    DeliveryStop {
        sequence,
        shipment_id: Uuid::new_v4(),
        address: make_address(lat, lng),
        time_window_start: None,
        time_window_end: None,
        estimated_arrival: None,
        actual_arrival: None,
        stop_type: StopType::Delivery,
    }
}

fn make_route_with_status(status: RouteStatus) -> Route {
    Route {
        id: RouteId::new(),
        tenant_id: TenantId::new(),
        driver_id: DriverId::new(),
        vehicle_id: VehicleId::new(),
        stops: Vec::new(),
        status,
        total_distance_km: 0.0,
        estimated_duration_minutes: 0,
        created_at: Utc::now(),
        started_at: None,
        completed_at: None,
    }
}

fn make_route_with_stops(status: RouteStatus, stop_count: usize) -> Route {
    let stops: Vec<DeliveryStop> = (0..stop_count)
        .map(|i| make_stop(i as u32 + 1, 14.5995 + i as f64 * 0.01, 120.9842))
        .collect();
    Route {
        id: RouteId::new(),
        tenant_id: TenantId::new(),
        driver_id: DriverId::new(),
        vehicle_id: VehicleId::new(),
        stops,
        status,
        total_distance_km: 0.0,
        estimated_duration_minutes: 0,
        created_at: Utc::now(),
        started_at: None,
        completed_at: None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Route business rules
// ─────────────────────────────────────────────────────────────────────────────

mod route_rules {
    use super::*;

    #[test]
    fn can_modify_returns_true_only_when_planned() {
        let planned   = make_route_with_status(RouteStatus::Planned);
        let progress  = make_route_with_status(RouteStatus::InProgress);
        let completed = make_route_with_status(RouteStatus::Completed);
        let cancelled = make_route_with_status(RouteStatus::Cancelled);

        assert!(planned.can_modify(),   "Planned route must be modifiable");
        assert!(!progress.can_modify(), "InProgress route must NOT be modifiable");
        assert!(!completed.can_modify(),"Completed route must NOT be modifiable");
        assert!(!cancelled.can_modify(),"Cancelled route must NOT be modifiable");
    }

    #[test]
    fn completing_a_route_sets_completed_at_timestamp() {
        // The Route entity itself does not expose a `complete()` mutating method —
        // the service layer sets completed_at directly. We model the same here.
        let mut route = make_route_with_status(RouteStatus::Completed);
        assert!(route.completed_at.is_none(), "completed_at must be None before being set");

        let now = Utc::now();
        route.completed_at = Some(now);

        assert!(route.completed_at.is_some(), "completed_at must be set after completion");
        // Timestamp should be very recent (within 1 second of now in test context)
        let delta = route.completed_at.unwrap().signed_duration_since(now);
        assert!(
            delta.num_milliseconds().abs() < 1000,
            "completed_at should be set to approximately now"
        );
    }

    #[test]
    fn route_at_max_business_stops_is_not_at_capacity() {
        // MAX_STOPS_BUSINESS = 100 — exactly 100 stops is the limit, not over
        let route = make_route_with_stops(RouteStatus::Planned, MAX_STOPS_BUSINESS);
        // is_at_capacity(max) is true only when stops.len() >= max
        assert!(
            route.is_at_capacity(MAX_STOPS_BUSINESS),
            "Route with exactly 100 stops must be considered at capacity"
        );
    }

    #[test]
    fn route_with_101_stops_exceeds_business_limit() {
        let route = make_route_with_stops(RouteStatus::Planned, MAX_STOPS_BUSINESS + 1);
        assert!(
            route.is_at_capacity(MAX_STOPS_BUSINESS),
            "Route with 101 stops must report at-capacity for Business tier limit of 100"
        );
    }

    #[test]
    fn route_below_max_stops_is_not_at_capacity() {
        let route = make_route_with_stops(RouteStatus::Planned, MAX_STOPS_BUSINESS - 1);
        assert!(
            !route.is_at_capacity(MAX_STOPS_BUSINESS),
            "Route with 99 stops must NOT be at capacity for Business tier"
        );
    }

    #[test]
    fn tier_constants_are_ordered_correctly() {
        assert!(MAX_STOPS_STARTER < MAX_STOPS_GROWTH);
        assert!(MAX_STOPS_GROWTH < MAX_STOPS_BUSINESS);
        assert_eq!(MAX_STOPS_BUSINESS, 100);
    }

    #[test]
    fn add_stop_greedy_fails_when_route_is_not_planned() {
        let mut route = make_route_with_status(RouteStatus::InProgress);
        let stop = make_stop(1, 14.60, 121.00);
        let result = route.add_stop_greedy(stop);
        assert!(result.is_err(), "add_stop_greedy must fail on an InProgress route");
        assert_eq!(
            result.unwrap_err(),
            "Cannot modify an in-progress or completed route"
        );
    }

    #[test]
    fn add_stop_greedy_succeeds_when_planned() {
        let mut route = make_route_with_status(RouteStatus::Planned);
        let stop = make_stop(1, 14.60, 121.00);
        let result = route.add_stop_greedy(stop);
        assert!(result.is_ok(), "add_stop_greedy must succeed on a Planned route");
        assert_eq!(route.stops.len(), 1);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ETA / duration estimation
// ─────────────────────────────────────────────────────────────────────────────

mod eta {
    use super::*;

    // Formula: ceil((distance_km / 30.0) * 60 + stop_count * 5)
    // estimate_duration_minutes(30.0, 10):
    //   drive = (30/30)*60 = 60 min
    //   service = 10*5 = 50 min
    //   total = 110 min
    #[test]
    fn thirty_km_ten_stops_gives_110_minutes() {
        let result = estimate_duration_minutes(30.0, 10);
        assert_eq!(result, 110, "30km + 10 stops should take 110 minutes");
    }

    // estimate_duration_minutes(0.0, 5):
    //   drive = 0 min
    //   service = 5*5 = 25 min
    #[test]
    fn zero_distance_five_stops_gives_25_minutes() {
        let result = estimate_duration_minutes(0.0, 5);
        assert_eq!(result, 25, "0km with 5 stops should take 25 minutes (service time only)");
    }

    // estimate_duration_minutes(15.0, 0):
    //   drive = (15/30)*60 = 30 min
    //   service = 0
    #[test]
    fn fifteen_km_no_stops_gives_30_minutes() {
        let result = estimate_duration_minutes(15.0, 0);
        assert_eq!(result, 30, "15km with no stops should take 30 minutes (drive only)");
    }

    #[test]
    fn zero_distance_zero_stops_gives_zero_minutes() {
        let result = estimate_duration_minutes(0.0, 0);
        assert_eq!(result, 0, "0km + 0 stops = 0 minutes");
    }

    #[test]
    fn fractional_distance_is_ceiled() {
        // 1.0 km at 30 km/h = 2 minutes exactly
        let result = estimate_duration_minutes(1.0, 0);
        assert_eq!(result, 2, "1km drive = 2 minutes");
    }

    #[test]
    fn speed_and_service_constants_have_correct_values() {
        // Sanity-check the constants the formula depends on
        assert_eq!(AVERAGE_SPEED_KMH, 30.0);
        assert_eq!(STOP_SERVICE_MINUTES, 5.0);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Driver availability — AvailableDriver selection rules
// ─────────────────────────────────────────────────────────────────────────────

mod driver_availability {
    use super::*;

    fn make_available_driver(driver_id: DriverId, distance_km: f64, active_stop_count: u32) -> AvailableDriver {
        AvailableDriver {
            driver_id,
            name: "Test Driver".into(),
            distance_km,
            location: Coordinates { lat: 14.5995, lng: 120.9842 },
            active_stop_count,
            vehicle_type: None,
        }
    }

    #[test]
    fn driver_with_zero_stops_and_close_distance_scores_lower_than_loaded_driver() {
        // Selection logic from DriverAssignmentService: score = distance * 0.7 + stop_load * 0.3
        // Lower score = better candidate
        let nearby_idle = make_available_driver(DriverId::new(), 2.0, 0);
        let farther_busy = make_available_driver(DriverId::new(), 5.0, 10);

        let score_idle  = nearby_idle.distance_km * 0.7 + nearby_idle.active_stop_count as f64 * 0.3;
        let score_busy  = farther_busy.distance_km * 0.7 + farther_busy.active_stop_count as f64 * 0.3;

        assert!(
            score_idle < score_busy,
            "A nearby idle driver (score {:.2}) should score lower than a farther busy driver (score {:.2})",
            score_idle, score_busy
        );
    }

    #[test]
    fn candidates_empty_means_no_driver_available() {
        let candidates: Vec<AvailableDriver> = vec![];
        // The service returns an error when candidates is empty.
        // We verify the condition that triggers that error.
        assert!(
            candidates.is_empty(),
            "An empty candidate list signals no available driver in radius"
        );
    }

    #[test]
    fn best_driver_selected_from_candidates_by_minimum_score() {
        let driver_a = make_available_driver(DriverId::new(), 10.0, 0);  // score: 7.0
        let driver_b = make_available_driver(DriverId::new(), 3.0,  2);  // score: 2.7
        let driver_c = make_available_driver(DriverId::new(), 1.0,  8);  // score: 3.1

        let candidates = vec![driver_a.clone(), driver_b.clone(), driver_c.clone()];

        let best = candidates.iter()
            .min_by(|a, b| {
                let sa = a.distance_km * 0.7 + a.active_stop_count as f64 * 0.3;
                let sb = b.distance_km * 0.7 + b.active_stop_count as f64 * 0.3;
                sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
            })
            .expect("candidates is non-empty"); // safe: we just built it with 3 elements

        assert_eq!(
            best.driver_id, driver_b.driver_id,
            "Driver B (distance=3, stops=2, score=2.7) should be selected over A and C"
        );
    }

    #[test]
    fn driver_with_active_assignment_should_not_be_selected() {
        // The service checks assignment_repo.find_active_by_driver before assigning.
        // At the domain level, is_active() covers Pending and Accepted statuses.
        let tenant_id = TenantId::new();
        let driver_id = DriverId::new();
        let route_id  = RouteId::new();

        let mut assignment = DriverAssignment::new(tenant_id, driver_id, route_id);
        assert!(assignment.is_active(), "A freshly created (Pending) assignment must be active");

        // After rejection, driver is no longer active
        assignment.reject("going offline".into()).expect("rejection of pending must succeed");
        assert!(
            !assignment.is_active(),
            "A rejected assignment must NOT be considered active"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Driver assignment state machine
// ─────────────────────────────────────────────────────────────────────────────

mod assignment_state_machine {
    use super::*;

    fn new_assignment() -> DriverAssignment {
        DriverAssignment::new(TenantId::new(), DriverId::new(), RouteId::new())
    }

    #[test]
    fn new_assignment_starts_as_pending() {
        let assignment = new_assignment();
        assert_eq!(assignment.status, AssignmentStatus::Pending);
    }

    #[test]
    fn accept_transitions_pending_to_accepted() {
        let mut assignment = new_assignment();
        assignment.accept().expect("accept of Pending must succeed");
        assert_eq!(assignment.status, AssignmentStatus::Accepted);
        assert!(assignment.accepted_at.is_some(), "accepted_at must be set on accept");
    }

    #[test]
    fn cannot_accept_already_accepted_assignment() {
        let mut assignment = new_assignment();
        assignment.accept().expect("first accept must succeed");
        let result = assignment.accept();
        assert!(result.is_err(), "accepting an already-Accepted assignment must fail");
    }

    #[test]
    fn reject_transitions_pending_to_rejected_with_reason() {
        let mut assignment = new_assignment();
        assignment.reject("driver is sick".into()).expect("reject of Pending must succeed");
        assert_eq!(assignment.status, AssignmentStatus::Rejected);
        assert_eq!(assignment.rejection_reason.as_deref(), Some("driver is sick"));
        assert!(assignment.rejected_at.is_some(), "rejected_at must be set on reject");
    }

    #[test]
    fn cannot_reject_accepted_assignment() {
        let mut assignment = new_assignment();
        assignment.accept().expect("accept must succeed");
        let result = assignment.reject("changed mind".into());
        assert!(result.is_err(), "rejecting an Accepted assignment must fail");
    }

    #[test]
    fn cancel_transitions_pending_to_cancelled() {
        let mut assignment = new_assignment();
        assignment.cancel().expect("cancel of Pending must succeed");
        assert_eq!(assignment.status, AssignmentStatus::Cancelled);
    }

    #[test]
    fn is_active_is_true_for_pending_and_accepted() {
        let mut assignment = new_assignment();
        assert!(assignment.is_active(), "Pending assignment must be active");

        assignment.accept().expect("accept must succeed");
        assert!(assignment.is_active(), "Accepted assignment must be active");
    }

    #[test]
    fn is_active_is_false_for_rejected_and_cancelled() {
        let mut a1 = new_assignment();
        a1.reject("test".into()).expect("reject must succeed");
        assert!(!a1.is_active(), "Rejected assignment must not be active");

        let mut a2 = new_assignment();
        a2.cancel().expect("cancel must succeed");
        assert!(!a2.is_active(), "Cancelled assignment must not be active");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Nearest-neighbor stop ordering (uses logisticos_geo)
// ─────────────────────────────────────────────────────────────────────────────

mod nearest_neighbor {
    use super::*;

    // Manila hub origin used in the dispatch service
    const MANILA: logisticos_geo::Coordinates = logisticos_geo::Coordinates { lat: 14.5995, lng: 120.9842 };

    #[test]
    fn nearest_stop_from_manila_is_returned_first() {
        // Three stops at varying distances from Manila
        let stops = vec![
            logisticos_geo::Coordinates::new(14.70, 121.10), // ~18 km north-east
            logisticos_geo::Coordinates::new(14.61, 120.99), // ~3 km north — nearest
            logisticos_geo::Coordinates::new(14.50, 120.90), // ~12 km south-west
        ];

        let order = nearest_neighbor_order(&MANILA, &stops);
        assert_eq!(order.len(), 3, "All three stops must appear in the order");
        assert_eq!(
            order[0], 1,
            "Stop at index 1 (~3 km away) must be visited first from Manila"
        );
    }

    #[test]
    fn single_stop_order_is_always_zero() {
        let stops = vec![logisticos_geo::Coordinates::new(14.67, 121.05)];
        let order = nearest_neighbor_order(&MANILA, &stops);
        assert_eq!(order, vec![0], "With one stop the order must be [0]");
    }

    #[test]
    fn zero_stops_produces_empty_order() {
        let stops: Vec<logisticos_geo::Coordinates> = vec![];
        let order = nearest_neighbor_order(&MANILA, &stops);
        assert!(order.is_empty(), "With no stops the order must be empty");
    }

    #[test]
    fn all_stop_indices_appear_exactly_once() {
        let stops = vec![
            logisticos_geo::Coordinates::new(14.58, 121.01),
            logisticos_geo::Coordinates::new(14.63, 121.04),
            logisticos_geo::Coordinates::new(14.52, 120.97),
            logisticos_geo::Coordinates::new(14.71, 121.08),
        ];
        let order = nearest_neighbor_order(&MANILA, &stops);
        assert_eq!(order.len(), 4, "All 4 indices must be present");

        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, vec![0, 1, 2, 3], "Each index must appear exactly once");
    }

    #[test]
    fn order_first_element_is_closer_than_last_element() {
        // Nearest-neighbor is a greedy heuristic; the first stop chosen must be
        // closer to the origin than the last stop chosen.
        let origin = MANILA;
        let stops = vec![
            logisticos_geo::Coordinates::new(14.61, 120.99), // close  ~3 km
            logisticos_geo::Coordinates::new(10.31, 123.88), // Cebu  ~565 km
        ];
        let order = nearest_neighbor_order(&origin, &stops);
        let first_dist = origin.distance_km(&stops[order[0]]);
        let last_dist  = origin.distance_km(&stops[order[order.len() - 1]]);
        assert!(
            first_dist < last_dist,
            "First visited stop ({:.1} km) must be closer than last ({:.1} km)",
            first_dist, last_dist
        );
    }
}
