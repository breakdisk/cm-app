use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct InitiatePodCommand {
    pub shipment_id: Uuid,
    pub task_id: Uuid,
    pub recipient_name: String,
    pub capture_lat: f64,
    pub capture_lng: f64,
}

#[derive(Debug, Deserialize)]
pub struct AttachSignatureCommand {
    pub pod_id: Uuid,
    pub signature_data: String,   // Base64-encoded PNG of the signature pad drawing
}

#[derive(Debug, Deserialize)]
pub struct SubmitPodCommand {
    pub pod_id: Uuid,
    pub cod_collected_cents: Option<i64>,
    pub otp_code: Option<String>,  // Required if shipment requires OTP verification
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
    pub pod_id: Uuid,
    pub s3_key: String,
    pub content_type: String,
    pub size_bytes: u64,
}
