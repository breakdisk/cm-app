// Unit tests for the fleet management domain layer.
use chrono::NaiveDate;
use uuid::Uuid;
use logisticos_fleet::domain::entities::{
    MaintenanceRecord, Vehicle, VehicleId, VehicleStatus, VehicleType,
};
use logisticos_types::TenantId;

fn tenant() -> TenantId { TenantId::from_uuid(Uuid::new_v4()) }

fn motorcycle(tenant_id: TenantId) -> Vehicle {
    Vehicle::new(tenant_id, "ABC 1234".into(), VehicleType::Motorcycle,
        "Honda".into(), "Beat".into(), 2022, "Red".into())
}

fn date_in(days: i64) -> NaiveDate {
    (chrono::Local::now() + chrono::Duration::days(days)).date_naive()
}

fn date_ago(days: i64) -> NaiveDate {
    (chrono::Local::now() - chrono::Duration::days(days)).date_naive()
}

#[test]
fn vehicle_new_defaults() {
    let t = tenant();
    let v = motorcycle(t.clone());
    assert_eq!(v.plate_number, "ABC 1234");
    assert_eq!(v.status, VehicleStatus::Active);
    assert_eq!(v.odometer_km, 0);
    assert!(v.assigned_driver_id.is_none());
    assert!(v.maintenance_history.is_empty());
    assert_eq!(v.tenant_id, t);
}
#[test]
fn assign_driver_succeeds() {
    let mut v = motorcycle(tenant());
    assert!(v.assign_driver(Uuid::new_v4()).is_ok());
}
#[test]
fn assign_driver_fails_under_maintenance() {
    let mut v = motorcycle(tenant());
    v.schedule_maintenance("Oil".into(), date_in(3));
    assert!(v.assign_driver(Uuid::new_v4()).is_err());
}
#[test]
fn assign_driver_fails_decommissioned() {
    let mut v = motorcycle(tenant());
    v.decommission();
    assert!(v.assign_driver(Uuid::new_v4()).is_err());
}
#[test]
fn unassign_driver_works() {
    let mut v = motorcycle(tenant());
    v.assign_driver(Uuid::new_v4()).unwrap();
    v.unassign_driver();
    assert!(v.assigned_driver_id.is_none());
}
#[test]
fn schedule_maintenance_adds_record() {
    let mut v = motorcycle(tenant());
    let date = date_in(7);
    v.schedule_maintenance("Oil change".into(), date);
    assert_eq!(v.maintenance_history.len(), 1);
    assert_eq!(v.maintenance_history[0].scheduled_date, date);
    assert_eq!(v.status, VehicleStatus::UnderMaintenance);
    assert_eq!(v.next_maintenance_due, Some(date));
}
#[test]
fn maintenance_history_capped_at_twenty() {
    let mut v = motorcycle(tenant());
    for i in 0..25i32 {
        v.schedule_maintenance(format!("S{}", i), date_in(i as i64 + 1));
        v.complete_maintenance(i * 1000, 50000, None).ok();
    }
    assert!(v.maintenance_history.len() <= 20);
}
#[test]
fn complete_maintenance_marks_done() {
    let mut v = motorcycle(tenant());
    v.schedule_maintenance("Full service".into(), date_in(3));
    assert!(v.complete_maintenance(15000, 350000, Some("Note".into())).is_ok());
    let r = v.maintenance_history.last().unwrap();
    assert!(r.completed_at.is_some());
    assert_eq!(r.odometer_km, Some(15000));
    assert_eq!(r.cost_cents, Some(350000));
}
#[test]
fn complete_maintenance_restores_active() {
    let mut v = motorcycle(tenant());
    v.schedule_maintenance("Check".into(), date_in(2));
    v.complete_maintenance(20000, 100000, None).unwrap();
    assert_eq!(v.status, VehicleStatus::Active);
    assert!(v.next_maintenance_due.is_none());
    assert_eq!(v.odometer_km, 20000);
}
#[test]
fn complete_maintenance_fails_when_no_pending() {
    let mut v = motorcycle(tenant());
    assert!(v.complete_maintenance(5000, 200000, None).is_err());
}
#[test]
fn maintenance_record_is_overdue() {
    let r = MaintenanceRecord::new("Old".into(), date_ago(2));
    assert!(r.is_overdue());
}
#[test]
fn maintenance_record_not_overdue_when_complete() {
    let mut r = MaintenanceRecord::new("Old".into(), date_ago(2));
    r.complete(10000, 5000, None);
    assert!(!r.is_overdue());
}
#[test]
fn maintenance_record_not_overdue_when_future() {
    let r = MaintenanceRecord::new("Future".into(), date_in(7));
    assert!(!r.is_overdue());
}
#[test]
fn maintenance_record_complete_sets_fields() {
    let mut r = MaintenanceRecord::new("Service".into(), date_in(3));
    r.complete(30000, 200000, Some("Note".into()));
    assert_eq!(r.odometer_km, Some(30000));
    assert_eq!(r.cost_cents, Some(200000));
    assert_eq!(r.notes, Some("Note".into()));
}
#[test]
fn decommission_sets_status_and_clears_driver() {
    let mut v = motorcycle(tenant());
    v.assign_driver(Uuid::new_v4()).unwrap();
    v.decommission();
    assert_eq!(v.status, VehicleStatus::Decommissioned);
    assert!(v.assigned_driver_id.is_none());
}
#[test]
fn decommission_is_idempotent() {
    let mut v = motorcycle(tenant());
    v.decommission(); v.decommission();
    assert_eq!(v.status, VehicleStatus::Decommissioned);
}
#[test]
fn is_maintenance_due_within_true() {
    let mut v = motorcycle(tenant());
    v.schedule_maintenance("Soon".into(), date_in(3));
    assert!(v.is_maintenance_due_within(7));
}
#[test]
fn is_maintenance_due_within_false_when_far() {
    let mut v = motorcycle(tenant());
    v.schedule_maintenance("Far".into(), date_in(30));
    assert!(!v.is_maintenance_due_within(7));
}
#[test]
fn is_maintenance_due_within_false_when_none() {
    let v = motorcycle(tenant());
    assert!(!v.is_maintenance_due_within(30));
}
#[test]
fn vehicle_ids_are_unique() {
    let v1 = motorcycle(tenant()); let v2 = motorcycle(tenant());
    assert_ne!(v1.id.inner(), v2.id.inner());
}
#[test]
fn vehicle_id_round_trip() {
    let v = motorcycle(tenant());
    let inner = v.id.inner();
    assert_eq!(VehicleId::from_uuid(inner).inner(), inner);
}
