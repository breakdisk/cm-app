use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The recipient's delivery PIN. Issued at booking (or at the door when the
/// recipient has none), required to submit the POD, and valid until delivery.
/// Policy lives in `value_objects::delivery_pin`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtpCode {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub shipment_id: Uuid,
    pub phone: String,
    pub code_hash: String,      // SHA-256 of the 6-digit code — never stored in plaintext
    pub is_used: bool,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    /// Wrong PINs entered against this code. It locks at the policy's `max_attempts`.
    #[serde(default)]
    pub failed_attempts: i32,
}

impl OtpCode {
    pub fn new(tenant_id: Uuid, shipment_id: Uuid, phone: String, code_hash: String, ttl: Duration) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            shipment_id,
            phone,
            code_hash,
            is_used: false,
            expires_at: now + ttl,
            created_at: now,
            failed_attempts: 0,
        }
    }

    pub fn is_valid(&self) -> bool {
        !self.is_used && Utc::now() < self.expires_at
    }

    pub fn is_locked(&self, max_attempts: i32) -> bool {
        self.failed_attempts >= max_attempts
    }

    pub fn mark_used(&mut self) {
        self.is_used = true;
    }
}
