use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use std::sync::Arc;

use crate::{
    domain::entities::template::NotificationChannel,
    infrastructure::channels::{
        sms::TwilioSmsAdapter, whatsapp::TwilioWhatsAppAdapter,
        ChannelAdapter,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantChannelConfig {
    pub id:        Uuid,
    pub tenant_id: Uuid,

    pub whatsapp_enabled:     bool,
    pub whatsapp_account_sid: Option<String>,
    #[serde(skip_serializing)]
    pub whatsapp_auth_token:  Option<String>,
    pub whatsapp_from_number: Option<String>,

    pub sms_enabled:     bool,
    pub sms_account_sid: Option<String>,
    #[serde(skip_serializing)]
    pub sms_auth_token:  Option<String>,
    pub sms_from_number: Option<String>,

    pub email_enabled:       bool,
    pub email_from_address:  Option<String>,
    pub email_from_name:     Option<String>,

    pub push_enabled: bool,

    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TenantChannelConfig {
    pub fn is_channel_enabled(&self, ch: NotificationChannel) -> bool {
        match ch {
            NotificationChannel::WhatsApp => self.whatsapp_enabled,
            NotificationChannel::Sms      => self.sms_enabled,
            NotificationChannel::Email    => self.email_enabled,
            NotificationChannel::Push     => self.push_enabled,
        }
    }

    /// Returns a per-tenant channel adapter built from stored credentials,
    /// or `None` if credentials are absent (caller falls back to global adapter).
    pub fn build_adapter(&self, ch: NotificationChannel) -> Option<Arc<dyn ChannelAdapter>> {
        match ch {
            NotificationChannel::WhatsApp => {
                let sid   = self.whatsapp_account_sid.as_deref()?;
                let token = self.whatsapp_auth_token.as_deref()?;
                let from  = self.whatsapp_from_number.as_deref()?;
                Some(Arc::new(TwilioWhatsAppAdapter::new(
                    sid.to_owned(), token.to_owned(), from.to_owned(),
                )))
            }
            NotificationChannel::Sms => {
                let sid   = self.sms_account_sid.as_deref()?;
                let token = self.sms_auth_token.as_deref()?;
                let from  = self.sms_from_number.as_deref()?;
                Some(Arc::new(TwilioSmsAdapter::new(
                    sid.to_owned(), token.to_owned(), from.to_owned(),
                )))
            }
            // Email uses SES (async init) — always falls back to global adapter.
            NotificationChannel::Email => None,
            // Push uses Expo via identity service — no per-tenant creds needed.
            NotificationChannel::Push => None,
        }
    }

    pub fn whatsapp_configured(&self) -> bool {
        self.whatsapp_account_sid.as_deref().is_some_and(|s| !s.is_empty())
            && self.whatsapp_from_number.as_deref().is_some_and(|s| !s.is_empty())
    }

    pub fn sms_configured(&self) -> bool {
        self.sms_account_sid.as_deref().is_some_and(|s| !s.is_empty())
            && self.sms_from_number.as_deref().is_some_and(|s| !s.is_empty())
    }

    pub fn email_configured(&self) -> bool {
        self.email_from_address.as_deref().is_some_and(|s| !s.is_empty())
    }
}

/// Masked view returned by the API — never exposes raw tokens.
#[derive(Debug, Serialize)]
pub struct ChannelConfigView {
    pub whatsapp: ChannelView,
    pub sms:      ChannelView,
    pub email:    ChannelView,
    pub push:     ChannelView,
}

#[derive(Debug, Serialize)]
pub struct ChannelView {
    pub enabled:    bool,
    pub configured: bool,
    /// Last 4 chars of the account SID if set, for display confirmation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_sid_hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_name: Option<String>,
}

impl TenantChannelConfig {
    pub fn to_view(&self) -> ChannelConfigView {
        ChannelConfigView {
            whatsapp: ChannelView {
                enabled:          self.whatsapp_enabled,
                configured:       self.whatsapp_configured(),
                account_sid_hint: self.whatsapp_account_sid.as_deref().map(hint_last4),
                from_number:      self.whatsapp_from_number.clone(),
                from_address:     None,
                from_name:        None,
            },
            sms: ChannelView {
                enabled:          self.sms_enabled,
                configured:       self.sms_configured(),
                account_sid_hint: self.sms_account_sid.as_deref().map(hint_last4),
                from_number:      self.sms_from_number.clone(),
                from_address:     None,
                from_name:        None,
            },
            email: ChannelView {
                enabled:          self.email_enabled,
                configured:       self.email_configured(),
                account_sid_hint: None,
                from_number:      None,
                from_address:     self.email_from_address.clone(),
                from_name:        self.email_from_name.clone(),
            },
            push: ChannelView {
                enabled:          self.push_enabled,
                configured:       true, // Expo needs no per-tenant creds
                account_sid_hint: None,
                from_number:      None,
                from_address:     None,
                from_name:        None,
            },
        }
    }
}

fn hint_last4(s: &str) -> String {
    if s.len() <= 4 {
        "****".to_owned()
    } else {
        format!("****{}", &s[s.len() - 4..])
    }
}

/// Upsert payload — tokens left as `None` are NOT overwritten.
#[derive(Debug, Deserialize)]
pub struct UpdateChannelConfigRequest {
    pub whatsapp: Option<WhatsAppConfigUpdate>,
    pub sms:      Option<SmsConfigUpdate>,
    pub email:    Option<EmailConfigUpdate>,
    pub push:     Option<PushConfigUpdate>,
}

#[derive(Debug, Deserialize)]
pub struct WhatsAppConfigUpdate {
    pub enabled:      Option<bool>,
    pub account_sid:  Option<String>,
    pub auth_token:   Option<String>,
    pub from_number:  Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SmsConfigUpdate {
    pub enabled:     Option<bool>,
    pub account_sid: Option<String>,
    pub auth_token:  Option<String>,
    pub from_number: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct EmailConfigUpdate {
    pub enabled:       Option<bool>,
    pub from_address:  Option<String>,
    pub from_name:     Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PushConfigUpdate {
    pub enabled: Option<bool>,
}
