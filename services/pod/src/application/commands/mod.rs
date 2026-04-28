use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct InitiatePodCommand {
    pub shipment_id: Uuid,
    pub task_id: Uuid,
    pub recipient_name: String,
    pub capture_lat: f64,
    pub capture_lng: f64,
    /// Whether the task requires a delivery photo. Defaults to true for backward
    /// compatibility — older clients that don't send this field still enforce evidence.
    #[serde(default = "default_true")]
    pub requires_photo: bool,
    /// Whether the task requires a recipient signature. Defaults to true.
    #[serde(default = "default_true")]
    pub requires_signature: bool,
}

fn default_true() -> bool { true }

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
    pub upload_url: String,     // Pre-signed S3 PUT URL (30-second expiry)
    pub s3_key: String,         // Key to pass back in AttachPhotoCommand
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
