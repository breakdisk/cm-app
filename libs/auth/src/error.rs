use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Token has expired")]
    TokenExpired,

    #[error("Token is invalid: {0}")]
    TokenInvalid(String),

    #[error("Token creation failed: {0}")]
    TokenCreation(String),

    #[error("Missing authorization header")]
    MissingToken,

    #[error("Insufficient permissions: required '{required}', user has {has:?}")]
    InsufficientPermissions { required: String, has: Vec<String> },

    #[error("Password hashing failed: {0}")]
    PasswordHash(String),

    #[error("API key is invalid or revoked")]
    InvalidApiKey,
}
