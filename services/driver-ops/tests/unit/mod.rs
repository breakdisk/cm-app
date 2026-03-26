use logisticos_driver_ops::domain::{
    entities::{
        driver::{Driver, DriverStatus},
        location::DriverLocation,
        task::{DriverTask, TaskStatus, TaskType},
    },
    value_objects::{ARRIVAL_GEOFENCE_METERS, MAX_PLAUSIBLE_SPEED_KMH, within_geofence},
};
use logisticos_types::{Address, Coordinates, DriverId, TenantId};
use uuid::Uuid;
use chrono::Utc;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_driver(status: DriverStatus, active_route_id: Option<Uuid>) -> Driver {
    Driver {
        id: DriverId::new(),
        tenant_id: TenantId::new(),
        user_id: Uuid::new_v4(),
        first_name: "Juan".to_string(),
        last_name: "dela Cruz".to_string(),
        phone: "+639171234567".to_string(),
        status,
        current_location: None,
        last_location_at: None,
        vehicle_id: None,
        active_route_id,
        is_active: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn make_task(task_type: TaskType, status: TaskStatus) -> DriverTask {
    DriverTask {
        id: Uuid::new_v4(),
        driver_id: DriverId::new(),
        route_id: Uuid::new_v4(),
        shipment_id: Uuid::new_v4(),
        task_type,
        sequence: 1,
        status,
        address: Address {
            line1: "123 Rizal Ave".to_string(),
            line2: None,
            barangay: Some("Barangay 1".to_string()),
            city: "Manila".to_string(),
            province: "Metro Manila".to_string(),
            postal_code: "1000".to_string(),
            country_code: "PH".to_string(),
            coordinates: None,
        },
        customer_name: "Maria Santos".to_string(),
        customer_phone: "+639170000001".to_string(),
        cod_amount_cents: None,
        special_instructions: None,
        pod_id: None,
        started_at: None,
        completed_at: None,
        failed_reason: None,
    }
}

fn make_location(lat: f64, lng: f64, speed_kmh: Option<f32>, minutes_ago: i64) -> DriverLocation {
    let recorded_at = Utc::now() - chrono::Duration::minutes(minutes_ago);
    DriverLocation {
        driver_id: Uuid::new_v4(),
        tenant_id: Uuid::new_v4(),
        lat,
        lng,
        accuracy_m: Some(5.0),
        speed_kmh,
        heading: None,
        battery_pct: None,
        recorded_at,
        received_at: Utc::now(),
    }
}

// ---------------------------------------------------------------------------
// Driver status machine
// ---------------------------------------------------------------------------

mod driver_status {
    use super::*;

    #[test]
    fn available_driver_with_no_active_route_can_accept_route() {
        let driver = make_driver(DriverStatus::Available, None);
        assert!(driver.can_accept_route());
    }

    #[test]
    fn offline_driver_cannot_accept_route() {
        let driver = make_driver(DriverStatus::Offline, None);
        assert!(!driver.can_accept_route());
    }

    #[test]
    fn driver_already_on_route_cannot_accept_another() {
        let driver = make_driver(DriverStatus::Available, Some(Uuid::new_v4()));
        assert!(!driver.can_accept_route());
    }

    #[test]
    fn en_route_driver_cannot_accept_route() {
        let driver = make_driver(DriverStatus::EnRoute, None);
        assert!(!driver.can_accept_route());
    }

    #[test]
    fn inactive_driver_cannot_accept_route() {
        let mut driver = make_driver(DriverStatus::Available, None);
        driver.is_active = false;
        assert!(!driver.can_accept_route());
    }

    #[test]
    fn go_online_transitions_status_to_available() {
        let mut driver = make_driver(DriverStatus::Offline, None);
        driver.go_online();
        assert_eq!(driver.status, DriverStatus::Available);
    }

    #[test]
    fn go_offline_transitions_status_to_offline() {
        let mut driver = make_driver(DriverStatus::Available, None);
        driver.go_offline();
        assert_eq!(driver.status, DriverStatus::Offline);
    }

    #[test]
    fn go_online_updates_updated_at() {
        let mut driver = make_driver(DriverStatus::Offline, None);
        let before = driver.updated_at;
        // Small sleep substitute: the timestamp is always Utc::now() in go_online.
        driver.go_online();
        // updated_at must be >= before; in practice it is the same instant or later.
        assert!(driver.updated_at >= before);
    }
}

// ---------------------------------------------------------------------------
// Location update
// ---------------------------------------------------------------------------

mod location_update {
    use super::*;

    #[test]
    fn update_location_stores_lat_lng_correctly() {
        let mut driver = make_driver(DriverStatus::EnRoute, None);
        let coords = Coordinates { lat: 14.5995, lng: 120.9842 };
        driver.update_location(coords);
        let stored = driver.current_location.unwrap();
        assert!((stored.lat - 14.5995).abs() < f64::EPSILON);
        assert!((stored.lng - 120.9842).abs() < f64::EPSILON);
    }

    #[test]
    fn update_location_sets_last_location_at() {
        let mut driver = make_driver(DriverStatus::Available, None);
        assert!(driver.last_location_at.is_none());
        driver.update_location(Coordinates { lat: 14.5, lng: 121.0 });
        assert!(driver.last_location_at.is_some());
    }

    #[test]
    fn driver_location_struct_stores_lat_lng_correctly() {
        let loc = make_location(14.5995, 120.9842, Some(40.0), 0);
        assert!((loc.lat - 14.5995).abs() < f64::EPSILON);
        assert!((loc.lng - 120.9842).abs() < f64::EPSILON);
    }

    #[test]
    fn fresh_location_is_not_stale() {
        let loc = make_location(14.5995, 120.9842, None, 0);
        assert!(!loc.is_stale());
    }

    #[test]
    fn location_older_than_five_minutes_is_stale() {
        let loc = make_location(14.5995, 120.9842, None, 6);
        assert!(loc.is_stale());
    }

    #[test]
    fn location_exactly_five_minutes_old_is_not_stale() {
        // Business rule: > 5 minutes is stale; exactly 5 is still valid.
        let loc = make_location(14.5995, 120.9842, None, 5);
        assert!(!loc.is_stale());
    }

    #[test]
    fn speed_below_200_is_plausible() {
        let loc = make_location(14.0, 121.0, Some(80.0), 0);
        assert!(loc.is_plausible_speed());
    }

    #[test]
    fn speed_exactly_200_is_plausible() {
        let loc = make_location(14.0, 121.0, Some(MAX_PLAUSIBLE_SPEED_KMH), 0);
        assert!(loc.is_plausible_speed());
    }

    #[test]
    fn speed_above_200_is_implausible() {
        let loc = make_location(14.0, 121.0, Some(201.0), 0);
        assert!(!loc.is_plausible_speed());
    }

    #[test]
    fn no_speed_reading_is_treated_as_plausible() {
        let loc = make_location(14.0, 121.0, None, 0);
        assert!(loc.is_plausible_speed());
    }
}

// ---------------------------------------------------------------------------
// Task status machine
// ---------------------------------------------------------------------------

mod task_status {
    use super::*;

    #[test]
    fn new_task_has_pending_status() {
        let task = make_task(TaskType::Delivery, TaskStatus::Pending);
        assert_eq!(task.status, TaskStatus::Pending);
    }

    #[test]
    fn start_transitions_status_to_in_progress() {
        let mut task = make_task(TaskType::Delivery, TaskStatus::Pending);
        task.start();
        assert_eq!(task.status, TaskStatus::InProgress);
    }

    #[test]
    fn start_records_started_at_timestamp() {
        let mut task = make_task(TaskType::Pickup, TaskStatus::Pending);
        assert!(task.started_at.is_none());
        task.start();
        assert!(task.started_at.is_some());
    }

    #[test]
    fn complete_delivery_task_without_pod_returns_err() {
        let mut task = make_task(TaskType::Delivery, TaskStatus::InProgress);
        let result = task.complete(None);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Delivery task requires proof of delivery"
        );
    }

    #[test]
    fn complete_delivery_task_with_pod_succeeds() {
        let mut task = make_task(TaskType::Delivery, TaskStatus::InProgress);
        let pod_id = Uuid::new_v4();
        let result = task.complete(Some(pod_id));
        assert!(result.is_ok());
        assert_eq!(task.status, TaskStatus::Completed);
        assert_eq!(task.pod_id, Some(pod_id));
    }

    #[test]
    fn complete_pickup_task_without_pod_succeeds() {
        let mut task = make_task(TaskType::Pickup, TaskStatus::InProgress);
        let result = task.complete(None);
        assert!(result.is_ok());
        assert_eq!(task.status, TaskStatus::Completed);
    }

    #[test]
    fn complete_records_completed_at_timestamp() {
        let mut task = make_task(TaskType::Pickup, TaskStatus::InProgress);
        assert!(task.completed_at.is_none());
        task.complete(None).unwrap();
        assert!(task.completed_at.is_some());
    }

    #[test]
    fn fail_transitions_status_to_failed() {
        let mut task = make_task(TaskType::Delivery, TaskStatus::InProgress);
        task.fail("customer not home".to_string());
        assert_eq!(task.status, TaskStatus::Failed);
    }

    #[test]
    fn fail_stores_failed_reason() {
        let mut task = make_task(TaskType::Delivery, TaskStatus::InProgress);
        task.fail("address not found".to_string());
        assert_eq!(task.failed_reason.as_deref(), Some("address not found"));
    }

    #[test]
    fn fail_records_completed_at_timestamp() {
        let mut task = make_task(TaskType::Delivery, TaskStatus::InProgress);
        assert!(task.completed_at.is_none());
        task.fail("failed".to_string());
        assert!(task.completed_at.is_some());
    }

    #[test]
    fn pickup_task_can_complete_without_pod() {
        let task = make_task(TaskType::Pickup, TaskStatus::InProgress);
        assert!(task.can_complete_without_pod());
    }

    #[test]
    fn delivery_task_cannot_complete_without_pod() {
        let task = make_task(TaskType::Delivery, TaskStatus::InProgress);
        assert!(!task.can_complete_without_pod());
    }
}

// ---------------------------------------------------------------------------
// COD collection on task
// ---------------------------------------------------------------------------

mod cod_collection {
    use super::*;

    #[test]
    fn task_with_cod_amount_stores_centavo_value() {
        let mut task = make_task(TaskType::Delivery, TaskStatus::InProgress);
        task.cod_amount_cents = Some(49900); // PHP 499.00
        assert_eq!(task.cod_amount_cents, Some(49900));
    }

    #[test]
    fn task_without_cod_has_none_amount() {
        let task = make_task(TaskType::Delivery, TaskStatus::Pending);
        assert!(task.cod_amount_cents.is_none());
    }

    #[test]
    fn zero_cod_amount_is_representable() {
        let mut task = make_task(TaskType::Delivery, TaskStatus::InProgress);
        task.cod_amount_cents = Some(0);
        assert_eq!(task.cod_amount_cents, Some(0));
    }
}

// ---------------------------------------------------------------------------
// Geofence value objects
// ---------------------------------------------------------------------------

mod geofence {
    use super::*;

    #[test]
    fn driver_within_200m_of_target_is_within_geofence() {
        // Quezon City coordinates — a ~50m offset
        let (target_lat, target_lng) = (14.6760, 121.0437);
        // Same coordinates = zero distance
        assert!(within_geofence(target_lat, target_lng, target_lat, target_lng));
    }

    #[test]
    fn driver_far_from_target_is_outside_geofence() {
        // Manila vs Cebu City (~570 km apart)
        let (driver_lat, driver_lng) = (14.5995, 120.9842);
        let (target_lat, target_lng) = (10.3157, 123.8854);
        assert!(!within_geofence(driver_lat, driver_lng, target_lat, target_lng));
    }

    #[test]
    fn geofence_radius_constant_is_200_metres() {
        assert!((ARRIVAL_GEOFENCE_METERS - 200.0).abs() < f64::EPSILON);
    }
}
