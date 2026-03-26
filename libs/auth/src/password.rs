use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use crate::error::AuthError;

/// Hash a plaintext password using Argon2id.
/// Call this once at registration/password-change time — never store plaintext.
pub fn hash_password(plaintext: &str) -> Result<String, AuthError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(plaintext.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AuthError::PasswordHash(e.to_string()))
}

/// Verify a plaintext password against a stored Argon2 hash.
/// Returns Ok(()) if they match, Err if they don't or the hash is malformed.
pub fn verify_password(plaintext: &str, hash: &str) -> Result<(), AuthError> {
    let parsed = PasswordHash::new(hash)
        .map_err(|e| AuthError::PasswordHash(e.to_string()))?;
    Argon2::default()
        .verify_password(plaintext.as_bytes(), &parsed)
        .map_err(|_| AuthError::InvalidCredentials)
}
