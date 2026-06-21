use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingFeature {
    pub feature_key:      String,
    pub feature_name:     String,
    pub feature_category: String,
    pub description:      Option<String>,
    /// Which subscription tier slugs include this feature (e.g. ["growth","business","enterprise"]).
    pub enabled_tiers:    Vec<String>,
    /// System features can have their tier list changed but cannot be deleted.
    pub is_system:        bool,
    pub sort_order:       i32,
    pub created_at:       DateTime<Utc>,
    pub updated_at:       DateTime<Utc>,
}

impl PricingFeature {
    pub fn is_enabled_for_tier(&self, tier: &str) -> bool {
        self.enabled_tiers.iter().any(|t| t == tier)
    }
}
