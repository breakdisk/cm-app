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
    #[validate(length(min = 7, max = 20))]
    pub phone_number: String,
    /// Optional tenant slug; defaults to "default" if omitted.
    pub tenant_slug: Option<String>,
    /// "driver" or "customer"; determines auto-registration role. Defaults to "driver".
    pub role: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct OtpVerifyCommand {
    #[validate(length(min = 7, max = 20))]
    pub phone_number: String,
    #[validate(length(equal = 6))]
    pub otp_code: String,
    /// Optional tenant slug; defaults to "default" if omitted.
    pub tenant_slug: Option<String>,
    /// "driver" or "customer"; determines auto-registration role. Defaults to "driver".
    pub role: Option<String>,
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
