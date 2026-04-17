use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverLocationUpdated {
    pub driver_id: Uuid,
    pub tenant_id: Uuid,
    pub lat: f64,
    pub lng: f64,
    pub speed_kmh: Option<f32>,
    pub heading: Option<f32>,
    pub recorded_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCompleted {
    pub task_id: Uuid,
    pub driver_id: Uuid,
    pub shipment_id: Uuid,
    pub tenant_id: Uuid,
    pub pod_id: Option<Uuid>,
    pub completed_at: chrono::DateTime<chrono::Utc>,
    // Customer fields — denormalized from DriverTask so engagement can send
    // delivery receipt without querying other services.
    #[serde(default)]
    pub customer_name: String,
    #[serde(default)]
    pub customer_phone: String,
    #[serde(default)]
    pub customer_email: String,
    #[serde(default)]
    pub tracking_number: String,
    #[serde(default)]
    pub cod_amount_cents: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskFailed {
    pub task_id: Uuid,
    pub driver_id: Uuid,
    pub shipment_id: Uuid,
    pub tenant_id: Uuid,
    pub reason: String,
    pub failed_at: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    pub customer_name: String,
    #[serde(default)]
    pub customer_phone: String,
    #[serde(default)]
    pub tracking_number: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverStatusChanged {
    pub driver_id: Uuid,
    pub tenant_id: Uuid,
    pub old_status: String,
    pub new_status: String,
}
