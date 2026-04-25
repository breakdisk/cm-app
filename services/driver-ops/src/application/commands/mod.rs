use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;
use crate::domain::entities::DriverType;

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

/// Partial-update command from the partner portal. Every field is optional;
/// only provided fields are written to the driver record.
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateDriverCommand {
    #[validate(length(min = 1, max = 100))]
    pub first_name: Option<String>,
    #[validate(length(min = 1, max = 100))]
    pub last_name: Option<String>,
    #[validate(length(min = 7, max = 20))]
    pub phone: Option<String>,
    pub driver_type: Option<DriverType>,
    #[validate(range(min = 0, max = 10_000_00))]
    pub per_delivery_rate_cents: Option<i32>,
    #[validate(range(min = 0, max = 10_000))]
    pub cod_commission_rate_bps: Option<i32>,
    #[validate(length(max = 100))]
    pub zone: Option<String>,
    #[validate(length(max = 50))]
    pub vehicle_type: Option<String>,
    pub is_active: Option<bool>,
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
    #[serde(default)]
    pub task_id: Uuid,
    pub pod_id: Option<Uuid>,       // Required for delivery tasks
    pub cod_collected_cents: Option<i64>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct FailTaskCommand {
    #[serde(default)]
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
    pub customer_phone: String,
    pub address: String,
    pub tracking_number: Option<String>,
    pub cod_amount_cents: Option<i64>,
    pub lat: Option<f64>,
    pub lng: Option<f64>,
    /// Capture requirements derived from task_type — driver app uses these to
    /// decide which sections of POD/Pickup screens to render. Pickup always
    /// needs the parcel photo + AWB scan; delivery additionally needs a
    /// signature, plus OTP/COD as applicable.
    pub requires_photo: bool,
    pub requires_signature: bool,
    pub requires_otp: bool,
}
