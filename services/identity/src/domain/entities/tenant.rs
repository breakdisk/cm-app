use chrono::{DateTime, Utc};
use logisticos_types::{TenantId, SubscriptionTier};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tenant {
    pub id: TenantId,
    pub name: String,
    pub slug: String,          // URL-safe unique identifier, e.g. "acme-logistics"
    pub subscription_tier: SubscriptionTier,
    pub is_active: bool,
    pub owner_email: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Tenant {
    pub fn new(name: String, slug: String, owner_email: String) -> Self {
        let now = Utc::now();
        Self {
            id: TenantId::new(),
            name,
            slug,
            subscription_tier: SubscriptionTier::Starter,
            is_active: true,
            owner_email,
            created_at: now,
            updated_at: now,
        }
    }

    /// Business rule: tenant slug must be lowercase alphanumeric + hyphens, 3-50 chars.
    pub fn validate_slug(slug: &str) -> Result<(), &'static str> {
        if slug.len() < 3 || slug.len() > 50 {
            return Err("Slug must be between 3 and 50 characters");
        }
        if !slug.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            return Err("Slug may only contain lowercase letters, digits, and hyphens");
        }
        if slug.starts_with('-') || slug.ends_with('-') {
            return Err("Slug cannot start or end with a hyphen");
        }
        Ok(())
    }

    /// Business rule: a suspended tenant cannot be re-activated on Starter tier
    /// (must upgrade first). Enterprise tenants can be restored by support.
    pub fn can_reactivate(&self) -> bool {
        !self.is_active && self.subscription_tier != SubscriptionTier::Starter
    }

    pub fn upgrade_tier(&mut self, tier: SubscriptionTier) {
        self.subscription_tier = tier;
        self.updated_at = Utc::now();
    }

    pub fn deactivate(&mut self) {
        self.is_active = false;
        self.updated_at = Utc::now();
    }
}
