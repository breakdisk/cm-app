use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct VehicleQuery {
    pub id: Uuid,
    pub plate_number: String,
    pub vehicle_type: String,
    pub make: String,
    pub model: String,
    pub year: u16,
    pub color: String,
    pub status: String,
    pub assigned_driver_id: Option<Uuid>,
    pub odometer_km: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MaintenanceQuery {
    pub description: String,
    pub scheduled_date: String,
    pub completed_at: Option<String>,
    pub odometer_km: Option<i32>,
    pub cost_cents: Option<i64>,
}
