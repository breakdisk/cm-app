use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
pub struct CreateTenantCommand {
    #[validate(length(min = 2, max = 100))]
    pub name: String,
    #[validate(length(min = 3, max = 50))]
    pub slug: String,
    #[validate(email)]
    pub owner_email: String,
    #[validate(length(min = 8))]
    pub owner_password: String,
    pub owner_first_name: String,
    pub owner_last_name: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct LoginCommand {
    pub tenant_slug: String,
    #[validate(email)]
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResult {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub token_type: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct InviteUserCommand {
    #[validate(email)]
    pub email: String,
    pub first_name: String,
    pub last_name: String,
    pub roles: Vec<String>,
    /// Optional E.164 phone number. Required for drivers so the Driver App
    /// OTP login can resolve this identity user by phone number.
    pub phone_number: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RefreshTokenCommand {
    pub refresh_token: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateApiKeyCommand {
    #[validate(length(min = 1, max = 100))]
    pub name: String,
    pub scopes: Vec<String>,
    pub expires_in_days: Option<u32>,  // None = no expiry
}

#[derive(Debug, Serialize)]
pub struct CreateApiKeyResult {
    pub key_id: uuid::Uuid,
    pub raw_key: String,    // Only returned once — client must store this
    pub key_prefix: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct ForgotPasswordCommand {
    pub tenant_slug: String,
    #[validate(email)]
    pub email: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct ResetPasswordCommand {
    pub token: String,
    #[validate(length(min = 8))]
    pub new_password: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct SendVerificationEmailCommand {
    pub tenant_slug: String,
    #[validate(email)]
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct VerifyEmailCommand {
    pub token: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct RegisterCommand {
    pub tenant_slug: String,
    #[validate(email)]
    pub email: String,
    #[validate(length(min = 8))]
    pub password: String,
    pub first_name: String,
    pub last_name: String,
}

// ─── OTP-based authentication (driver app + customer app) ────────────────────

#[derive(Debug, Deserialize, Validate)]
pub struct OtpSendCommand {
    /// Phone number (E.164 format). Required for phone-based OTP; omit for email-based.
    #[validate(length(min = 7, max = 20))]
    #[serde(default)]
    pub phone_number: Option<String>,
    /// Email address. Required for email-based OTP; omit for phone-based.
    #[serde(default)]
    pub email: Option<String>,
    /// Optional tenant slug; defaults to "default" if omitted.
    pub tenant_slug: Option<String>,
    /// "driver" or "customer"; determines auto-registration role. Defaults to "driver".
    pub role: Option<String>,
}

impl OtpSendCommand {
    /// Returns the identifier used as the Redis key (phone number or email address).
    pub fn identifier(&self) -> Option<&str> {
        self.phone_number.as_deref().or(self.email.as_deref())
    }
}

#[derive(Debug, Deserialize, Validate)]
pub struct OtpVerifyCommand {
    /// Phone number (E.164 format). Required for phone-based OTP; omit for email-based.
    #[validate(length(min = 7, max = 20))]
    #[serde(default)]
    pub phone_number: Option<String>,
    /// Email address. Required for email-based OTP; omit for phone-based.
    #[serde(default)]
    pub email: Option<String>,
    #[validate(length(equal = 6))]
    pub otp_code: String,
    /// Optional tenant slug; defaults to "default" if omitted.
    pub tenant_slug: Option<String>,
    /// "driver" or "customer"; determines auto-registration role. Defaults to "driver".
    pub role: Option<String>,
}

impl OtpVerifyCommand {
    /// Returns the identifier used as the Redis key (phone number or email address).
    pub fn identifier(&self) -> Option<&str> {
        self.phone_number.as_deref().or(self.email.as_deref())
    }
}

// ─── Firebase token exchange (server-side bridge) ────────────────────────────
//
// Consumed by the internal endpoint POST /v1/internal/auth/exchange-firebase.
// The landing app verifies the Firebase ID token, then POSTs the claims here
// so identity mints a LogisticOS JWT. See the Firebase → LogisticOS JWT bridge
// spec under docs/superpowers/specs/.

#[derive(Debug, Deserialize, Validate)]
pub struct ExchangeFirebaseCommand {
    #[validate(length(min = 1))]
    pub firebase_uid: String,
    #[validate(email)]
    pub email: String,
    pub email_verified: bool,
    /// "merchant" | "admin" | "partner" | "customer"
    pub role: String,
    pub display_name: Option<String>,
    /// Signed white-label partner context for customer auto-link.
    /// `partner_sig` = HMAC-SHA256(LOGISTICOS_PARTNER_HMAC_SECRET, partner_slug + ":" + firebase_uid)
    pub partner_slug: Option<String>,
    pub partner_sig: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ExchangeFirebaseResult {
    pub access_token:  String,
    pub refresh_token: String,
    pub expires_in:    i64,
    pub token_type:    String,
    pub user:          ExchangedUser,
}

#[derive(Debug, Deserialize, Validate)]
pub struct FinalizeTenantCommand {
    #[validate(length(min = 2, max = 100))]
    pub business_name: String,
    /// ISO 4217 currency code, e.g. "USD", "AED", "PHP".
    #[validate(length(equal = 3))]
    pub currency: String,
    /// ISO 3166-1 alpha-2 region code, e.g. "PH", "AE".
    #[validate(length(equal = 2))]
    pub region: String,
}

/// Partial-update command for PUT /v1/tenants/:id. Slug + tier + status are
/// intentionally not editable here — those have first-class endpoints with
/// side-effects (cross-service slug rewrites, billing flips, ops audit).
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateTenantCommand {
    #[validate(length(min = 2, max = 100))]
    #[serde(default)]
    pub name: Option<String>,
    #[validate(email)]
    #[serde(default)]
    pub owner_email: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ExchangedUser {
    pub id:                   String,
    pub tenant_id:             String,
    pub tenant_slug:           String,
    pub email:                 String,
    pub roles:                 Vec<String>,
    pub onboarding_required:   bool,
}

#[derive(Debug, Serialize)]
pub struct OtpVerifyResult {
    pub access_token: String,
    pub refresh_token: String,
    pub driver_id: String,
    pub tenant_id: String,
    pub expires_in: i64,
    pub token_type: String,
}

/// Upgrade (or downgrade) the subscription tier for a tenant.
/// Accepted values match the `SubscriptionTier` snake_case variants:
/// "starter" | "growth" | "business" | "enterprise".
#[derive(Debug, Deserialize, Validate)]
pub struct UpgradeTierCommand {
    #[validate(length(min = 1))]
    pub tier: String,
}

// ─── Driver invite link generation ───────────────────────────────────────────

/// Generates a signed deep-link invite URL for a pre-registered driver.
/// The driver taps the link, the app pre-fills their phone + tenant, and
/// they complete normal OTP login — landing in the correct tenant automatically.
#[derive(Debug, Deserialize)]
pub struct GenerateDriverInviteLinkCommand {
    /// UUID of the pre-registered driver user (must have role "driver" and a phone_number).
    pub user_id: uuid::Uuid,
}

#[derive(Debug, Serialize)]
pub struct GenerateDriverInviteLinkResult {
    /// Fully qualified deep-link URL the admin copies and shares with the driver.
    pub invite_url: String,
    /// ISO 8601 UTC expiry — 72 hours from generation time.
    pub expires_at: String,
}

/// Partial self-update — authenticated user edits their own profile.
/// All fields are optional; absent fields are left unchanged.
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateUserSelfCommand {
    #[validate(length(min = 1, max = 100))]
    #[serde(default)]
    pub first_name: Option<String>,
    #[validate(length(min = 0, max = 100))]
    #[serde(default)]
    pub last_name: Option<String>,
    #[serde(default)]
    pub phone_number: Option<String>,
}
