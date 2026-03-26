// Per-tenant channel configuration: which provider credentials to use.
// Stored encrypted in Vault; referenced here by key name only.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantChannelConfig {
    pub tenant_id: uuid::Uuid,
    pub whatsapp_enabled: bool,
    pub sms_enabled: bool,
    pub email_enabled: bool,
    pub push_enabled: bool,
    // Vault key paths for credentials — never store raw secrets in DB
    pub twilio_vault_key: Option<String>,
    pub sendgrid_vault_key: Option<String>,
    pub firebase_vault_key: Option<String>,
}
