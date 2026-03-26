use chrono::{DateTime, Utc};
use logisticos_types::{ApiKeyId, TenantId};
use serde::{Deserialize, Serialize};

/// An API key used by external systems (merchants, carriers, webhooks) to authenticate.
/// The raw key is shown once at creation; only the hash is stored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: ApiKeyId,
    pub tenant_id: TenantId,
    pub name: String,               // Human label: "Shopify Integration", "Carrier Webhook"
    pub key_hash: String,           // SHA-256 of the raw key — never store the raw key
    pub key_prefix: String,         // First 8 chars for display: "lsk_live_ab12..."
    pub scopes: Vec<String>,        // Subset of RBAC permissions
    pub is_active: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl ApiKey {
    /// Business rule: API keys expire after 1 year by default (unless set to None = no expiry).
    pub fn is_valid(&self) -> bool {
        if !self.is_active {
            return false;
        }
        if let Some(exp) = self.expires_at {
            return Utc::now() < exp;
        }
        true
    }

    pub fn record_usage(&mut self) {
        self.last_used_at = Some(Utc::now());
    }

    pub fn revoke(&mut self) {
        self.is_active = false;
    }
}
