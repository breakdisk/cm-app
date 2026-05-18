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
    /// This is the server receipt timestamp. Use `device_timestamp` for SLA audit.
    pub captured_at:        String,
    /// Hardware clock timestamp from the driver's device at moment of submission.
    /// Absent for older clients. Use for chain-of-custody and SLA calculations.
    #[serde(default)]
    pub device_timestamp:   Option<String>,
    /// Soft audit flag — set when driver was > 50m from delivery address.
    /// Payments service writes this to billing invoice `workflow_metadata`.
    #[serde(default)]
    pub out_of_bounds_handover: bool,
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

/// Emitted when a driver successfully submits a Proof of Pickup.
/// Opens the chain of custody for the shipment; POD closes it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PickupCaptured {
    pub pop_id:             Uuid,
    pub shipment_id:        Uuid,
    pub task_id:            Uuid,
    pub tenant_id:          Uuid,
    pub driver_id:          Uuid,
    /// True when the driver was within 200m of the pickup address.
    pub geofence_verified:  bool,
    /// True when the driver was > 50m from the pickup address (soft audit flag).
    #[serde(default)]
    pub out_of_bounds_handover: bool,
    pub barcode_scanned:    bool,
    /// Actual weight in grams as measured at pickup (for Track B overage billing).
    #[serde(default)]
    pub actual_weight_g:    Option<i64>,
    /// Declared weight in grams from the shipment booking.
    #[serde(default)]
    pub declared_weight_g:  Option<i64>,
    /// Weight overage ratio — positive when actual > declared.
    /// Computed by `ProofOfPickup::weight_overage_ratio()`.
    #[serde(default)]
    pub weight_overage_ratio: Option<f64>,
    /// S3 key of the parcel condition photo (optional).
    #[serde(default)]
    pub photo_s3_key:       Option<String>,
    /// ISO-8601 server receipt timestamp (backend clock).
    pub captured_at:        String,
    /// ISO-8601 device clock timestamp from driver's device.
    #[serde(default)]
    pub device_timestamp:   Option<String>,
    /// 3-char tenant code for invoice number generation.
    #[serde(default)]
    pub tenant_code:        String,
}
