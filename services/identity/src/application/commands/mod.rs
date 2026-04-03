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
