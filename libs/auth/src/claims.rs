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

    /// Anonymous table-session flag. When `true`, the subject is a diner who
    /// scanned a QR code on a restaurant table and has no account at all — the
    /// token was minted by `services/omnideliv` against a `table_sessions` row,
    /// carries a synthetic `user_id` (the session id), an empty `email`, no
    /// permissions, and a short expiry.
    ///
    /// It is NOT an identity-service user and never becomes one. Services can
    /// use this as a belt-and-suspenders check alongside permission gating: any
    /// route that would be wrong to expose to an unauthenticated diner should
    /// refuse on this flag even if a permission were somehow granted. Exactly
    /// the role `onboarding` above plays for draft tenants.
    ///
    /// `#[serde(default)]` keeps every existing token deserializable — old JWTs
    /// without the field decode as `table_session: false`.
    #[serde(default)]
    pub table_session: bool,

    /// Feature keys enabled for this tenant's tier, populated at JWT mint time
    /// from the platform-wide `identity.pricing_features` matrix. Old tokens
    /// that lack this field decode as an empty vec; callers should fall back to
    /// tier-based checks when the vec is empty.
    #[serde(default)]
    pub enabled_features: Vec<String>,

    /// The subject's own phone number, from `identity.users.phone_number`.
    ///
    /// Carried on the token so a service that needs to put a courier in touch
    /// with a customer does not have to call identity on the money path.
    /// OmniDeliv's checkout used to infer this from the login address, which
    /// only works for the minted `<digits>@customer.logisticos.app` form — every
    /// customer who signed in any other way left the courier with no number to
    /// dial, while identity had it in a column all along.
    ///
    /// `#[serde(default)]` so tokens minted before this decode as `None`; the
    /// login-derived fallback still covers them until they expire.
    #[serde(default)]
    pub phone: Option<String>,

    /// The tenant's billing currency (e.g. "AED", "PHP"), from `Tenant.currency`.
    /// Carried on the token so services that price or charge money don't need a
    /// cross-service call to identity per request. `None` for a draft tenant that
    /// hasn't finished onboarding, or a token minted before this field existed.
    #[serde(default)]
    pub currency: Option<String>,
}

impl Claims {
    /// Eight arguments, because a JWT claim set has eight fields and every one
    /// is required. Grouping them into a builder or a params struct would move
    /// the arity rather than remove it, and would let a caller construct a
    /// half-populated token — the opposite of what this type is for.
    #[allow(clippy::too_many_arguments)]
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
            table_session: false,
            enabled_features: Vec::new(),
            phone: None,
            currency: None,
        }
    }

    /// Mints the anonymous principal for a diner who scanned a table QR code.
    ///
    /// A dedicated constructor rather than `new(..)` plus a flag, because the
    /// scope is the whole point: no roles, no permissions, an empty email, and
    /// a synthetic `user_id` that is the `table_sessions` row id. Someone
    /// copying `new` and remembering to set `table_session` but forgetting to
    /// blank the permissions would mint a diner token that can act as a
    /// merchant, and that is exactly the mistake this signature removes.
    ///
    /// `expiry_seconds` should be minutes, not hours: the code that mints this
    /// is printed on vinyl in a public room.
    #[must_use]
    pub fn for_table_session(
        session_id: Uuid,
        tenant_id: Uuid,
        tenant_slug: String,
        subscription_tier: String,
        expiry_seconds: i64,
    ) -> Self {
        let mut c = Self::new(
            session_id,
            tenant_id,
            tenant_slug,
            subscription_tier,
            String::new(),
            Vec::new(),
            Vec::new(),
            expiry_seconds,
        );
        c.table_session = true;
        c
    }

    /// Attach the caller's enabled feature keys (from the pricing feature matrix)
    /// to the token. Chainable after `Claims::new(...)`.
    #[must_use]
    pub fn with_features(mut self, features: Vec<String>) -> Self {
        self.enabled_features = features;
        self
    }

    /// Attach the subject's phone number. Chainable, like [`Self::with_features`],
    /// so no existing `Claims::new` call site has to change.
    pub fn with_phone(mut self, phone: Option<String>) -> Self {
        self.phone = phone.filter(|p| !p.trim().is_empty());
        self
    }

    /// Attach the tenant's billing currency. Chainable, like [`Self::with_phone`].
    #[must_use]
    pub fn with_currency(mut self, currency: Option<String>) -> Self {
        self.currency = currency;
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

#[cfg(test)]
mod table_session_tests {
    use super::*;

    fn diner() -> Claims {
        Claims::for_table_session(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "demo".into(),
            "growth".into(),
            600,
        )
    }

    #[test]
    fn a_table_session_principal_can_do_nothing_by_permission() {
        // The load-bearing assertion. This token is minted from a code printed
        // on vinyl in a public room; anything it can do by permission, a
        // stranger photographing the table can do.
        let c = diner();
        assert!(c.permissions.is_empty(), "a diner must hold no permissions");
        assert!(c.roles.is_empty(), "and no roles");
        assert!(c.email.is_empty(), "there is no person to name");
        assert!(c.table_session, "and it must be marked as what it is");
        assert!(!c.onboarding, "it is not a draft-tenant principal");
    }

    #[test]
    fn the_synthetic_user_id_is_the_session_id() {
        // This is what lets `orders.customer_id` stay non-null for a diner with
        // no account, so nothing downstream has to learn anonymity exists.
        let session_id = Uuid::new_v4();
        let c = Claims::for_table_session(session_id, Uuid::new_v4(), "d".into(), "growth".into(), 600);
        assert_eq!(c.user_id, session_id);
        assert_eq!(c.sub, session_id.to_string());
    }

    #[test]
    fn an_ordinary_token_is_not_a_table_session() {
        let c = Claims::new(
            Uuid::new_v4(), Uuid::new_v4(), "demo".into(), "growth".into(),
            "a@b.com".into(), vec!["merchant".into()], vec!["shipments:create".into()], 3600,
        );
        assert!(!c.table_session, "a normal principal must never read as anonymous");
    }

    #[test]
    fn a_token_minted_before_this_field_existed_still_decodes() {
        // Same compatibility rule every other added claim follows.
        let json = r#"{
            "sub": "11111111-1111-1111-1111-111111111111",
            "iat": 0, "exp": 0, "jti": "x",
            "tenant_id": "11111111-1111-1111-1111-111111111111",
            "tenant_slug": "acme",
            "subscription_tier": "starter",
            "user_id": "11111111-1111-1111-1111-111111111111",
            "email": "a@b.com",
            "roles": [], "permissions": []
        }"#;
        let c: Claims = serde_json::from_str(json).expect("an old token must still decode");
        assert!(!c.table_session);
    }
}

#[cfg(test)]
mod claims_currency_tests {
    use super::*;

    #[test]
    fn old_token_json_without_currency_field_still_deserializes() {
        // Simulates a JWT minted before this field existed — no `currency` key at all.
        let json = r#"{
            "sub": "11111111-1111-1111-1111-111111111111",
            "iat": 0, "exp": 0, "jti": "x",
            "tenant_id": "11111111-1111-1111-1111-111111111111",
            "tenant_slug": "acme",
            "subscription_tier": "starter",
            "user_id": "11111111-1111-1111-1111-111111111111",
            "email": "a@b.com",
            "roles": [], "permissions": []
        }"#;
        let claims: Claims = serde_json::from_str(json).expect("must deserialize");
        assert_eq!(claims.currency, None);
    }

    #[test]
    fn with_currency_sets_the_field() {
        let claims = Claims::new(
            Uuid::new_v4(), Uuid::new_v4(), "acme".into(), "starter".into(),
            "a@b.com".into(), vec![], vec![], 3600,
        ).with_currency(Some("AED".into()));
        assert_eq!(claims.currency.as_deref(), Some("AED"));
    }
}
