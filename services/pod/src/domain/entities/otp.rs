use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One-time password for recipient confirmation on high-value deliveries.
/// Sent via SMS to the recipient's phone number before the driver arrives.
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
}

impl OtpCode {
    pub fn new(tenant_id: Uuid, shipment_id: Uuid, phone: String, code_hash: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            shipment_id,
            phone,
            code_hash,
            is_used: false,
            expires_at: now + chrono::Duration::minutes(15),
            created_at: now,
        }
    }

    pub fn is_valid(&self) -> bool {
        !self.is_used && Utc::now() < self.expires_at
    }

    pub fn mark_used(&mut self) {
        self.is_used = true;
    }
}
