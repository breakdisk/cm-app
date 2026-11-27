use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

/// A user's binding to one partner (carrier or neighborhood store). The partner
/// id from an *active* row is what token issue puts in the `pid` claim.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PartnerMembership {
    pub id:         Uuid,
    pub tenant_id:  Uuid,
    pub user_id:    Uuid,
    pub carrier_id: Uuid,
    pub role:       String,
    /// `active` or `suspended`.
    pub status:     String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewPartnerMembership {
    pub tenant_id:  Uuid,
    pub user_id:    Uuid,
    pub carrier_id: Uuid,
    pub role:       String,
}

/// Roles a membership may carry. A label for what the person does for the
/// partner; the permissions come from the user's platform roles.
pub const MEMBERSHIP_ROLES: &[&str] = &["partner", "store_manager", "store_clerk", "store_rider"];
