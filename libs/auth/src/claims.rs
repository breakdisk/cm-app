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

    /// Feature keys enabled for this tenant's tier, populated at JWT mint time
    /// from the platform-wide `identity.pricing_features` matrix. Old tokens
    /// that lack this field decode as an empty vec; callers should fall back to
    /// tier-based checks when the vec is empty.
    #[serde(default)]
    pub enabled_features: Vec<String>,
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
            enabled_features: Vec::new(),
        }
    }

    /// Attach the caller's enabled feature keys (from the pricing feature matrix)
    /// to the token. Chainable after `Claims::new(...)`.
    #[must_use]
    pub fn with_features(mut self, features: Vec<String>) -> Self {
        self.enabled_features = features;
        self
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

    /// Returns true if a specific feature key is enabled for this token's tier.
    /// When `enabled_features` is non-empty (JWT minted after the feature matrix
    /// was introduced), the matrix result is authoritative. Old JWTs (empty vec)
    /// fall back to hardcoded tier thresholds so existing sessions remain valid.
    pub fn has_feature(&self, feature_key: &str) -> bool {
        if !self.enabled_features.is_empty() {
            return self.enabled_features.iter().any(|f| f == feature_key);
        }
        // Fallback: derive from tier for legacy tokens
        match feature_key {
            "real_time_tracking" | "cod_reconciliation" | "balikbayan_service" => true,
            "ai_dispatch" | "same_day_delivery" =>
                matches!(self.subscription_tier.as_str(), "growth" | "business" | "enterprise"),
            "ai_recovery_agent" | "loyalty_program" | "dynamic_pricing" =>
                matches!(self.subscription_tier.as_str(), "business" | "enterprise"),
            "enterprise_mcp" | "white_label" =>
                self.subscription_tier.as_str() == "enterprise",
            _ => false,
        }
    }

    /// Returns true if the subscription tier allows AI features.
    /// Checks the feature matrix when available; falls back to tier for old tokens.
    pub fn can_use_ai(&self) -> bool {
        if !self.enabled_features.is_empty() {
            return self.has_feature("ai_dispatch") || self.has_feature("ai_recovery_agent");
        }
        matches!(self.subscription_tier.as_str(), "business" | "enterprise")
    }

    /// Returns true if the subscription tier allows white-label branding.
    /// Checks the feature matrix when available; falls back to tier for old tokens.
    pub fn can_use_white_label(&self) -> bool {
        if !self.enabled_features.is_empty() {
            return self.has_feature("white_label");
        }
        self.subscription_tier.as_str() == "enterprise"
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
