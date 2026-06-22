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
    /// POP reference — set for pickup tasks; None for delivery tasks.
    #[serde(default)]
    pub pop_id: Option<Uuid>,
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
    /// Customer UUID — populated when dispatch carries it in TaskAssigned
    /// (requires TaskAssigned.customer_id to be added). Absent in legacy events;
    /// engagement falls back to shipment_id as the audit key.
    #[serde(default)]
    pub customer_id: Option<Uuid>,
    /// Carrier this driver is linked to (from drivers.carrier_id).
    /// Payments uses this to credit the carrier wallet with the delivery margin.
    #[serde(default)]
    pub carrier_id: Option<Uuid>,
    /// Contractual payout snapshotted from task.payout_cents at completion.
    /// Absent for full-time drivers. Payments credits carrier wallet with this.
    #[serde(default)]
    pub payout_cents: Option<i64>,
    /// Driver's COD commission rate in basis points (from drivers.cod_commission_rate_bps).
    /// Payments multiplies this against cod_amount_cents to calculate carrier COD commission.
    #[serde(default)]
    pub cod_commission_rate_bps: i32,
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
    /// Customer UUID — absent in current events; engagement falls back to shipment_id.
    #[serde(default)]
    pub customer_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverStatusChanged {
    pub driver_id: Uuid,
    pub tenant_id: Uuid,
    pub old_status: String,
    pub new_status: String,
}
