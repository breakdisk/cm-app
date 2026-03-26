use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
pub struct RegisterDriverCommand {
    pub user_id: Uuid,
    #[validate(length(min = 1, max = 100))]
    pub first_name: String,
    #[validate(length(min = 1, max = 100))]
    pub last_name: String,
    #[validate(length(min = 7, max = 20))]
    pub phone: String,
    pub vehicle_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateLocationCommand {
    pub lat: f64,
    pub lng: f64,
    pub accuracy_m: Option<f32>,
    pub speed_kmh: Option<f32>,
    pub heading: Option<f32>,
    pub battery_pct: Option<u8>,
    pub recorded_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct StartTaskCommand {
    pub task_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct CompleteTaskCommand {
    pub task_id: Uuid,
    pub pod_id: Option<Uuid>,       // Required for delivery tasks
    pub cod_collected_cents: Option<i64>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct FailTaskCommand {
    pub task_id: Uuid,
    #[validate(length(min = 3, max = 500))]
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct TaskSummary {
    pub task_id: Uuid,
    pub shipment_id: Uuid,
    pub sequence: u32,
    pub status: String,
    pub task_type: String,
    pub customer_name: String,
    pub address: String,
    pub cod_amount_cents: Option<i64>,
}
