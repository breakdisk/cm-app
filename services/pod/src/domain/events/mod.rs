use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PodCaptured {
    pub pod_id: Uuid,
    pub shipment_id: Uuid,
    pub task_id: Uuid,
    pub tenant_id: Uuid,
    pub driver_id: Uuid,
    pub recipient_name: String,
    pub has_signature: bool,
    pub photo_count: usize,
    pub otp_verified: bool,
    pub cod_collected_cents: Option<i64>,
    pub captured_at: chrono::DateTime<chrono::Utc>,
}
