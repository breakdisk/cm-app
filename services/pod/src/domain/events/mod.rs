use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PodCaptured {
    pub pod_id:             Uuid,
    pub shipment_id:        Uuid,
    pub task_id:            Uuid,
    pub tenant_id:          Uuid,
    pub driver_id:          Uuid,
    pub recipient_name:     String,
    pub has_signature:      bool,
    pub photo_count:        usize,
    pub otp_verified:       bool,
    /// COD amount collected at doorstep in cents (0 when none collected).
    /// Field name and type match canonical logisticos_events::payloads::PodCaptured
    /// so the payments pod_consumer can deserialize without aliasing or null handling.
    #[serde(default)]
    pub cod_amount_cents:   i64,
    /// ISO-8601 string (from DateTime<Utc>.to_rfc3339()) so downstream
    /// consumers don't need to import chrono to parse.
    pub captured_at:        String,
    /// 3-char tenant code for invoice number generation.
    #[serde(default)]
    pub tenant_code:        String,
    /// True if the shipment was self-booked via customer app (B2C).
    #[serde(default)]
    pub booked_by_customer: bool,
    /// Customer UUID — populated when `booked_by_customer` is true.
    #[serde(default)]
    pub customer_id:        Option<Uuid>,
    /// Customer email for receipt delivery.
    #[serde(default)]
    pub customer_email:     Option<String>,
    /// Customer phone — forwarded to payments/engagement for WhatsApp delivery.
    /// Carried from SubmitPodCommand (driver app has it from the task screen).
    #[serde(default)]
    pub customer_phone:     String,
}
