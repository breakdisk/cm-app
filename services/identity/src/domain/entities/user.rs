use chrono::{DateTime, Utc};
use logisticos_types::{UserId, TenantId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub tenant_id: TenantId,
    pub email: String,
    /// Never serialized: every user endpoint returns this struct as JSON, and
    /// until this they returned the hash with it.
    #[serde(skip_serializing, default)]
    pub password_hash: String,
    pub first_name: String,
    pub last_name: String,
    pub roles: Vec<String>,
    pub is_active: bool,
    pub email_verified: bool,
    /// E.164-normalised phone number, e.g. `+639171234567`.
    /// Set when a driver is pre-registered by a Partner portal admin so the
    /// Driver App OTP login can resolve the correct identity user by phone.
    pub phone_number: Option<String>,
    pub last_login_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl User {
    pub fn new(
        tenant_id: TenantId,
        email: String,
        password_hash: String,
        first_name: String,
        last_name: String,
        roles: Vec<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: UserId::new(),
            tenant_id,
            email,
            password_hash,
            first_name,
            last_name,
            roles,
            is_active: true,
            email_verified: false,
            phone_number: None,
            last_login_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn full_name(&self) -> String {
        format!("{} {}", self.first_name, self.last_name)
    }

    /// Business rule: deactivated users cannot log in.
    pub fn can_login(&self) -> bool {
        self.is_active && self.email_verified
    }

    pub fn record_login(&mut self) {
        self.last_login_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    pub fn assign_role(&mut self, role: &str) {
        if !self.roles.contains(&role.to_owned()) {
            self.roles.push(role.to_owned());
            self.updated_at = Utc::now();
        }
    }

    pub fn revoke_role(&mut self, role: &str) {
        self.roles.retain(|r| r != role);
        self.updated_at = Utc::now();
    }
}

#[cfg(test)]
mod serialization_tests {
    use super::*;

    #[test]
    fn a_user_as_json_never_carries_the_password_hash() {
        let u = User::new(TenantId::new(), "a@b.co".into(), "$argon2id$secret".into(), "A".into(), "B".into(), vec![]);
        let json = serde_json::to_value(&u).unwrap();
        assert!(json.get("password_hash").is_none());
        assert!(!json.to_string().contains("argon2id"));
    }
}
