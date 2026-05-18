use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::NaiveDate;

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateVehicleCommand {
    pub plate_number: String,
    pub vehicle_type: String,
    pub make: String,
    pub model: String,
    pub year: u16,
    pub color: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UpdateVehicleCommand {
    pub plate_number: Option<String>,
    pub color: Option<String>,
    pub odometer_km: Option<i32>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AssignDriverCommand {
    pub driver_id: Uuid,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ScheduleMaintenanceCommand {
    pub description: String,
    pub scheduled_date: NaiveDate,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CompleteMaintenanceCommand {
    pub odometer_km: i32,
    pub cost_cents: i64,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DecommissionVehicleCommand {
    pub reason: Option<String>,
}
