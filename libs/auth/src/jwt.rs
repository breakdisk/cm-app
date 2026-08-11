use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, TokenData, Validation};
use crate::{claims::{Claims, RefreshClaims}, error::AuthError};

pub struct JwtService {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    access_expiry_seconds: i64,
    refresh_expiry_seconds: i64,
}

impl JwtService {
    pub fn new(secret: &str, access_expiry_seconds: i64, refresh_expiry_seconds: i64) -> Self {
        Self {
            encoding_key: EncodingKey::from_secret(secret.as_bytes()),
            decoding_key: DecodingKey::from_secret(secret.as_bytes()),
            access_expiry_seconds,
            refresh_expiry_seconds,
        }
    }

    pub fn issue_access_token(&self, claims: Claims) -> Result<String, AuthError> {
        encode(&Header::new(Algorithm::HS256), &claims, &self.encoding_key)
            .map_err(|e| AuthError::TokenCreation(e.to_string()))
    }

    pub fn issue_refresh_token(&self, claims: RefreshClaims) -> Result<String, AuthError> {
        encode(&Header::new(Algorithm::HS256), &claims, &self.encoding_key)
            .map_err(|e| AuthError::TokenCreation(e.to_string()))
    }

    /// Clock skew allowed on `exp`, in seconds.
    ///
    /// This is jsonwebtoken's own default, stated here rather than inherited.
    /// Tokens are minted by identity and validated in twenty other containers,
    /// so some drift between them is normal and a hard boundary would produce
    /// spurious 401s. It is worth being explicit because it is surprising: a
    /// token one second past `exp` still validates, and a test that expires one
    /// by a second and expects rejection is testing the leeway, not the expiry.
    pub const CLOCK_SKEW_LEEWAY_SECS: u64 = 60;

    pub fn validate_access_token(&self, token: &str) -> Result<TokenData<Claims>, AuthError> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;
        validation.leeway = Self::CLOCK_SKEW_LEEWAY_SECS;
        decode::<Claims>(token, &self.decoding_key, &validation)
            .map_err(|e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::TokenExpired,
                _ => AuthError::TokenInvalid(e.to_string()),
            })
    }

    pub fn validate_refresh_token(&self, token: &str) -> Result<TokenData<RefreshClaims>, AuthError> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;
        validation.leeway = Self::CLOCK_SKEW_LEEWAY_SECS;
        decode::<RefreshClaims>(token, &self.decoding_key, &validation)
            .map_err(|e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::TokenExpired,
                _ => AuthError::TokenInvalid(e.to_string()),
            })
    }

    pub fn access_expiry_seconds(&self) -> i64 { self.access_expiry_seconds }
    pub fn refresh_expiry_seconds(&self) -> i64 { self.refresh_expiry_seconds }
}
