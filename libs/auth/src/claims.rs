use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Standard JWT claims for every request in LogisticOS.
/// Carried in the `Authorization: Bearer <token>` header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    // ── Standard JWT fields ──────────────────────────────────
    pub sub: String,         // UserId (UUID string)
    pub iat: i64,            // Issued at (Unix timestamp)
    pub exp: i64,            // Expiry (Unix timestamp)
    pub jti: String,         // JWT ID — unique per token (for revocation)

    // ── Tenant context ───────────────────────────────────────
    pub tenant_id: Uuid,
    pub tenant_slug: String,
    pub subscription_tier: String,  // "starter" | "growth" | "business" | "enterprise"

    // ── Identity ─────────────────────────────────────────────
    pub user_id: Uuid,
    pub email: String,
    pub roles: Vec<String>,         // e.g. ["admin", "dispatcher"]
    pub permissions: Vec<String>,   // e.g. ["shipments:create", "drivers:assign"]
}

impl Claims {
    pub fn new(
        user_id: Uuid,
        tenant_id: Uuid,
        tenant_slug: String,
        subscription_tier: String,
        email: String,
        roles: Vec<String>,
        permissions: Vec<String>,
        expiry_seconds: i64,
    ) -> Self {
        let now = Utc::now();
        Self {
            sub: user_id.to_string(),
            iat: now.timestamp(),
            exp: (now + Duration::seconds(expiry_seconds)).timestamp(),
            jti: Uuid::new_v4().to_string(),
            tenant_id,
            tenant_slug,
            subscription_tier,
            user_id,
            email,
            roles,
            permissions,
        }
    }

    /// Check if the claims include a specific permission.
    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.contains(&permission.to_owned())
            || self.permissions.contains(&"*".to_owned())  // superadmin wildcard
    }

    /// Check if claims include a specific role.
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.contains(&role.to_owned())
    }

    /// Returns true if the subscription tier allows AI features.
    pub fn can_use_ai(&self) -> bool {
        matches!(self.subscription_tier.as_str(), "business" | "enterprise")
    }
}

/// Minimal claims embedded in a refresh token (no permissions — must be exchanged for access token).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshClaims {
    pub sub: String,
    pub jti: String,
    pub tenant_id: Uuid,
    pub iat: i64,
    pub exp: i64,
}

impl RefreshClaims {
    pub fn new(user_id: Uuid, tenant_id: Uuid, expiry_seconds: i64) -> Self {
        let now = Utc::now();
        Self {
            sub: user_id.to_string(),
            jti: Uuid::new_v4().to_string(),
            tenant_id,
            iat: now.timestamp(),
            exp: (now + Duration::seconds(expiry_seconds)).timestamp(),
        }
    }
}
