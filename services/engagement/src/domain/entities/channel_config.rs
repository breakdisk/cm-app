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
    // Social / CRM integration channels — disabled by default until the
    // tenant connects their platform account via the Connectors settings page.
    #[serde(default)]
    pub messenger_enabled: bool,
    #[serde(default)]
    pub telegram_enabled: bool,
    #[serde(default)]
    pub x_enabled: bool,
    #[serde(default)]
    pub viber_enabled: bool,
    #[serde(default)]
    pub wechat_enabled: bool,
    #[serde(default)]
    pub line_enabled: bool,
    #[serde(default)]
    pub slack_enabled: bool,
    // Vault key paths for credentials — never store raw secrets in DB
    pub twilio_vault_key: Option<String>,
    pub sendgrid_vault_key: Option<String>,
    pub firebase_vault_key: Option<String>,
    // Social connector vault keys (per-tenant app credentials)
    #[serde(default)]
    pub meta_messenger_vault_key: Option<String>,
    #[serde(default)]
    pub telegram_bot_vault_key: Option<String>,
    #[serde(default)]
    pub slack_bot_vault_key: Option<String>,
}
