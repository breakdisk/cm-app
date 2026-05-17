use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VehicleEvent {
    VehicleCreated {
        vehicle_id: Uuid,
        tenant_id: Uuid,
        plate_number: String,
    },
    VehicleAssigned {
        vehicle_id: Uuid,
        driver_id: Uuid,
    },
    VehicleUnassigned {
        vehicle_id: Uuid,
    },
    MaintenanceScheduled {
        vehicle_id: Uuid,
        scheduled_date: String,
    },
    MaintenanceCompleted {
        vehicle_id: Uuid,
        odometer_km: i32,
    },
    VehicleDecommissioned {
        vehicle_id: Uuid,
    },
}
