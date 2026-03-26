use chrono::{DateTime, Utc};
use logisticos_types::{UserId, TenantId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    pub tenant_id: TenantId,
    pub email: String,
    pub password_hash: String,
    pub first_name: String,
    pub last_name: String,
    pub roles: Vec<String>,
    pub is_active: bool,
    pub email_verified: bool,
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
