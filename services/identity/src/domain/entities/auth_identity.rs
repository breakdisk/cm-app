use chrono::{DateTime, Utc};
use logisticos_types::UserId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// External identity provider linked to an internal `User`.
///
/// One user may be linked to several providers (e.g. Firebase + SAML).
/// Lookup is `(provider, provider_subject)` unique per identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthIdentity {
    pub id:               Uuid,
    pub user_id:          UserId,
    pub provider:         AuthProvider,
    pub provider_subject: String,
    pub email_at_link:    String,
    pub linked_at:        DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthProvider {
    Firebase,
    Saml,
    GoogleWorkspace,
}

impl AuthProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Firebase        => "firebase",
            Self::Saml            => "saml",
            Self::GoogleWorkspace => "google_workspace",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "firebase"         => Some(Self::Firebase),
            "saml"             => Some(Self::Saml),
            "google_workspace" => Some(Self::GoogleWorkspace),
            _                  => None,
        }
    }
}

impl AuthIdentity {
    pub fn new(
        user_id: UserId,
        provider: AuthProvider,
        provider_subject: String,
        email_at_link: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id,
            provider,
            provider_subject,
            email_at_link,
            linked_at: Utc::now(),
        }
    }
}
