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

    /// Draft-tenant onboarding flag. When `true`, the subject is still in the
    /// lazy-onboarding flow — the JWT was minted for a draft tenant via
    /// `/v1/internal/auth/exchange-firebase` and only carries the narrow
    /// `tenants:update-self` / `billing:setup` permission set. Gateway and
    /// service middleware can use this as a defensive belt-and-suspenders
    /// check alongside permission gating (e.g. block non-finalize mutations
    /// on operational services even if a permission was accidentally granted).
    ///
    /// `#[serde(default)]` keeps existing tokens deserializable after the
    /// upgrade — old JWTs without this field decode as `onboarding: false`.
    #[serde(default)]
    pub onboarding: bool,
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
            onboarding: false,
        }
    }

    /// Mark the claims as an onboarding (draft-tenant) token. Chainable on
    /// `Claims::new(...)` so existing call sites stay untouched; only the
    /// draft-merchant branch in `exchange_firebase` needs to set this.
    #[must_use]
    pub fn with_onboarding(mut self, onboarding: bool) -> Self {
        self.onboarding = onboarding;
        self
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
