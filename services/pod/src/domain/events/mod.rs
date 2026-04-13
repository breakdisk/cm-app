use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PodCaptured {
    pub pod_id:              Uuid,
    pub shipment_id:         Uuid,
    pub task_id:             Uuid,
    pub tenant_id:           Uuid,
    pub driver_id:           Uuid,
    pub recipient_name:      String,
    pub has_signature:       bool,
    pub photo_count:         usize,
    pub otp_verified:        bool,
    pub cod_collected_cents: Option<i64>,
    pub captured_at:         chrono::DateTime<chrono::Utc>,
    /// 3-char tenant code for invoice number generation.
    #[serde(default)]
    pub tenant_code:         String,
    /// True if the shipment was self-booked via customer app (B2C).
    #[serde(default)]
    pub booked_by_customer:  bool,
    /// Customer UUID — populated when `booked_by_customer` is true.
    #[serde(default)]
    pub customer_id:         Option<Uuid>,
    /// Customer email for receipt delivery.
    #[serde(default)]
    pub customer_email:      Option<String>,
}
