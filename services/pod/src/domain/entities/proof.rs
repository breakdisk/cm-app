use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Proof of Delivery — the complete evidence bundle for a successful delivery.
/// Immutable after creation; status transitions are the only mutations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofOfDelivery {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub shipment_id: Uuid,
    pub task_id: Uuid,
    pub driver_id: Uuid,
    pub status: PodStatus,

    // Signature capture — base64-encoded SVG/PNG of recipient's signature pad drawing
    pub signature_data: Option<String>,
    pub recipient_name: String,

    // Photo evidence — S3 object keys (pre-signed URLs generated on request)
    pub photos: Vec<PodPhoto>,

    // GPS verification — driver must be within geofence of delivery address
    pub capture_lat: f64,
    pub capture_lng: f64,
    pub geofence_verified: bool,

    // OTP verification — optional extra confirmation for high-value shipments
    pub otp_verified: bool,
    pub otp_id: Option<Uuid>,

    // COD collection record
    pub cod_collected_cents: Option<i64>,

    // Task-level evidence requirements — set at initiation time from the dispatch task config.
    // When both are false (e.g. OTP-only or low-risk deliveries), geofence alone satisfies
    // completeness and the driver is not blocked for missing photos/signature.
    #[serde(default = "default_true")]
    pub requires_photo: bool,
    #[serde(default = "default_true")]
    pub requires_signature: bool,

    pub captured_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

fn default_true() -> bool { true }

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PodStatus {
    Draft,      // In progress of being captured (multi-step flow)
    Submitted,  // Fully captured; awaiting async processing (photo upload)
    Verified,   // All verifications passed
    Disputed,   // Merchant has flagged this POD for review
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PodPhoto {
    pub id: Uuid,
    pub s3_key: String,         // bucket/tenant/shipment/pod_id/uuid.jpg
    pub content_type: String,   // "image/jpeg"
    pub size_bytes: u64,
    pub uploaded_at: DateTime<Utc>,
}

impl ProofOfDelivery {
    pub fn new(
        tenant_id: Uuid,
        shipment_id: Uuid,
        task_id: Uuid,
        driver_id: Uuid,
        recipient_name: String,
        capture_lat: f64,
        capture_lng: f64,
        geofence_verified: bool,
        requires_photo: bool,
        requires_signature: bool,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            shipment_id,
            task_id,
            driver_id,
            status: PodStatus::Draft,
            signature_data: None,
            recipient_name,
            photos: Vec::new(),
            capture_lat,
            capture_lng,
            geofence_verified,
            otp_verified: false,
            otp_id: None,
            cod_collected_cents: None,
            requires_photo,
            requires_signature,
            captured_at: now,
            created_at: now,
        }
    }

    pub fn attach_signature(&mut self, signature_data: String) {
        self.signature_data = Some(signature_data);
    }

    pub fn attach_photo(&mut self, photo: PodPhoto) {
        self.photos.push(photo);
    }

    pub fn mark_otp_verified(&mut self, otp_id: Uuid) {
        self.otp_verified = true;
        self.otp_id = Some(otp_id);
    }

    pub fn record_cod(&mut self, amount_cents: i64) {
        self.cod_collected_cents = Some(amount_cents);
    }

    /// Business rule: a POD is complete when geofence is verified and all required
    /// evidence types are present. When neither photo nor signature was required by the
    /// task (e.g. OTP-only or low-risk deliveries), geofence alone is sufficient.
    pub fn is_complete(&self) -> bool {
        let evidence_satisfied = match (self.requires_photo, self.requires_signature) {
            (false, false) => true,
            _ => self.signature_data.is_some() || !self.photos.is_empty(),
        };
        evidence_satisfied && self.geofence_verified
    }

    pub fn submit(&mut self) -> Result<(), &'static str> {
        if !self.is_complete() {
            return Err("POD is incomplete: missing required evidence or geofence not verified");
        }
        self.status = PodStatus::Submitted;
        Ok(())
    }

    pub fn verify(&mut self) {
        self.status = PodStatus::Verified;
    }

    pub fn dispute(&mut self) {
        self.status = PodStatus::Disputed;
    }
}
