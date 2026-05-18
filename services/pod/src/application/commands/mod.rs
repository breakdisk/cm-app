use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct InitiatePodCommand {
    pub shipment_id:    Uuid,
    pub task_id:        Uuid,
    pub recipient_name: String,
    pub capture_lat:    f64,
    pub capture_lng:    f64,
    /// Whether the task requires a delivery photo. Defaults to true for backward
    /// compatibility — older clients that don't send this field still enforce evidence.
    #[serde(default = "default_true")]
    pub requires_photo: bool,
    /// Whether the task requires a recipient signature. Defaults to true.
    #[serde(default = "default_true")]
    pub requires_signature: bool,
    /// Hardware clock timestamp from the driver's device at the moment of POD initiation.
    /// Optional — absent when client is an older build that does not yet send it.
    #[serde(default)]
    pub device_timestamp: Option<DateTime<Utc>>,
}

fn default_true() -> bool { true }
fn default_standard() -> String { "standard".to_owned() }

#[derive(Debug, Deserialize)]
pub struct AttachSignatureCommand {
    pub pod_id: Uuid,
    pub signature_data: String,   // Base64-encoded PNG of the signature pad drawing
}

#[derive(Debug, Deserialize)]
pub struct SubmitPodCommand {
    // Populated from the URL path (PUT /v1/pods/{id}/submit), not the body.
    // Without #[serde(default)] axum's Json<> deserializer 422s the request
    // because the client doesn't repeat the id in the body.
    #[serde(default)]
    pub pod_id:               Uuid,
    pub cod_collected_cents:  Option<i64>,
    pub otp_code:             Option<String>,  // Required if shipment requires OTP verification
    /// 3-char tenant code for invoice number generation (e.g. "PH1").
    #[serde(default)]
    pub tenant_code:          String,
    /// True if shipment was self-booked via customer app.
    #[serde(default)]
    pub booked_by_customer:   bool,
    /// Customer UUID — required when `booked_by_customer` is true.
    #[serde(default)]
    pub customer_id:          Option<Uuid>,
    /// Customer email for payment receipt delivery.
    #[serde(default)]
    pub customer_email:       Option<String>,
    /// Customer phone — driver app carries this from the task summary screen.
    /// Forwarded into PodCaptured so payments/engagement can route WhatsApp
    /// for receipt and COD notifications without a cross-service lookup.
    #[serde(default)]
    pub customer_phone:       String,
    /// Hardware clock timestamp from the driver's device at moment of submission.
    /// Forwarded into PodCaptured and telemetry log as the canonical event time.
    #[serde(default)]
    pub device_timestamp:     Option<DateTime<Utc>>,
}

// ── Pickup commands ────────────────────────────────────────────────────────────

/// Driver initiates a Proof of Pickup at the merchant/hub.
#[derive(Debug, Deserialize)]
pub struct InitiatePickupCommand {
    pub shipment_id:   Uuid,
    pub task_id:       Uuid,
    pub capture_lat:   f64,
    pub capture_lng:   f64,
    /// Pickup address coordinates — used for geofence and OUT_OF_BOUNDS_HANDOVER check.
    pub pickup_lat:    f64,
    pub pickup_lng:    f64,
    /// Declared weight in grams from the shipment booking (for Track B overage detection).
    #[serde(default)]
    pub declared_weight_g: Option<i64>,
    /// Hardware clock timestamp from driver device.
    #[serde(default)]
    pub device_timestamp:  Option<DateTime<Utc>>,
    /// Service code that classifies the shipment for billing routing.
    /// "balikbayan" → Track A driver ledger debit at pickup.
    /// "standard"   → Track B weight-based surcharge at hub weigh-in.
    /// Defaults to "standard" when absent (backward compat).
    #[serde(default = "default_standard")]
    pub service_code: String,
    /// Declared value of the shipment in cents (for insurance / Track A base billing).
    /// Optional — absent for standard parcels where billing is weight-based.
    #[serde(default)]
    pub declared_value_cents: Option<i64>,
}

/// Driver submits a completed Proof of Pickup.
#[derive(Debug, Deserialize)]
pub struct SubmitPickupCommand {
    // Path param — see SubmitPodCommand for rationale.
    #[serde(default)]
    pub pop_id:            Uuid,
    /// Barcode scanned from the physical AWB label.
    pub scanned_barcode:   String,
    /// Actual weight in grams as measured at pickup (Track B).
    #[serde(default)]
    pub actual_weight_g:   Option<i64>,
    /// S3 key of the parcel photo uploaded via presigned URL (optional).
    #[serde(default)]
    pub photo_s3_key:      Option<String>,
    #[serde(default)]
    pub photo_size_bytes:  Option<u64>,
    /// Hardware clock timestamp from driver device.
    #[serde(default)]
    pub device_timestamp:  Option<DateTime<Utc>>,
    /// 3-char tenant code for invoice number generation (e.g. "PH1").
    /// Forwarded into PickupCaptured so payments can issue the Track A/B invoice.
    #[serde(default)]
    pub tenant_code:       String,
}

#[derive(Debug, Deserialize)]
pub struct GenerateOtpCommand {
    pub shipment_id: Uuid,
    pub recipient_phone: String,
}

#[derive(Debug, Deserialize)]
pub struct VerifyOtpCommand {
    pub shipment_id: Uuid,
    pub code: String,
}

#[derive(Debug, Serialize)]
pub struct UploadUrlResponse {
    pub upload_url: String,     // Pre-signed S3 PUT URL
    pub s3_key: String,         // Key to pass back in AttachPhotoCommand
    /// Headers the client MUST include in the PUT request alongside the presigned URL.
    /// Forwarded verbatim from PresignedRequest::headers() — typically contains
    /// `x-amz-content-sha256` and `x-amz-date` when Cloudflare R2 signs them.
    pub upload_headers: std::collections::HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct AttachPhotoCommand {
    // Populated from the URL path (POST /v1/pods/{id}/photo) — see SubmitPodCommand for rationale.
    #[serde(default)]
    pub pod_id: Uuid,
    pub s3_key: String,
    pub content_type: String,
    pub size_bytes: u64,
}
