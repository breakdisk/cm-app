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
    /// How the PIN reached the recipient: `"sms"` or `"response"`. `None` means
    /// it is not known to have reached them (or the row predates tracking).
    #[serde(default)]
    pub delivered_via: Option<String>,
    #[serde(default)]
    pub delivered_at: Option<DateTime<Utc>>,
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
            delivered_via: None,
            delivered_at: None,
        }
    }

    pub fn is_valid(&self) -> bool {
        !self.is_used && Utc::now() < self.expires_at
    }

    pub fn is_locked(&self, max_attempts: i32) -> bool {
        self.failed_attempts >= max_attempts
    }

    /// Stamps how the PIN reached the recipient. A `None` channel leaves it unset.
    pub fn record_delivery(&mut self, via: Option<&str>) {
        if let Some(v) = via {
            self.delivered_via = Some(v.to_string());
            self.delivered_at = Some(Utc::now());
        }
    }

    pub fn mark_used(&mut self) {
        self.is_used = true;
    }
}
