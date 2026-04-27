//! External HTTP adapters for driver-ops.
//!
//! `FcmClient` delivers push notifications to the Android driver app via
//! Firebase Cloud Messaging (FCM HTTP v1 API).  It is intentionally
//! fire-and-forget: a failed push does not block task creation.
//!
//! Token pipeline:
//!   1. Fetch the driver's FCM push token from identity's internal endpoint.
//!   2. Obtain a short-lived Google OAuth2 access token using a service-account
//!      JWT signed with RS256 (cached for 55 minutes to avoid hammering Google).
//!   3. POST the FCM data message.

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Service-account JSON shape ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ServiceAccount {
    client_email: String,
    private_key: String,
}

// ── JWT claims for Google OAuth2 ───────────────────────────────────────────

#[derive(Debug, Serialize)]
struct GoogleJwtClaims {
    iss: String,
    scope: String,
    aud: String,
    exp: i64,
    iat: i64,
}

// ── OAuth2 token response ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct GoogleTokenResponse {
    access_token: String,
    expires_in: u64,
}

// ── Cached access token ────────────────────────────────────────────────────

struct CachedToken {
    token: String,
    valid_until: Instant,
}

// ── FcmClient ─────────────────────────────────────────────────────────────

pub struct FcmClient {
    identity_base_url: String,
    project_id: String,
    service_account: ServiceAccount,
    http: reqwest::Client,
    cached_token: Arc<Mutex<Option<CachedToken>>>,
}

impl FcmClient {
    /// Create a new client.
    ///
    /// `service_account_json` is the raw service-account JSON (not base64).
    /// Returns `None` when either `project_id` or `service_account_json` is
    /// empty so callers can treat FCM as optional without error-checking.
    pub fn new(
        identity_base_url: String,
        project_id: String,
        service_account_json: &str,
    ) -> Option<Self> {
        if project_id.is_empty() || service_account_json.is_empty() {
            return None;
        }
        let raw = if service_account_json.trim_start().starts_with('{') {
            service_account_json.to_owned()
        } else {
            // Accept base64-encoded JSON for env-var friendliness.
            match base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                service_account_json.trim(),
            ) {
                Ok(bytes) => String::from_utf8(bytes).ok()?,
                Err(_) => return None,
            }
        };
        let service_account: ServiceAccount = serde_json::from_str(&raw).ok()?;
        Some(Self {
            identity_base_url,
            project_id,
            service_account,
            http: reqwest::Client::new(),
            cached_token: Arc::new(Mutex::new(None)),
        })
    }

    /// Main entry point: look up the driver's FCM token then send a push.
    /// All errors are logged and swallowed — push is best-effort.
    pub async fn notify_driver(&self, driver_user_id: Uuid) {
        match self.fetch_push_tokens(driver_user_id).await {
            Err(e) => {
                tracing::warn!(driver_id = %driver_user_id, err = %e, "FCM: failed to fetch push tokens");
                return;
            }
            Ok(tokens) if tokens.is_empty() => {
                tracing::debug!(driver_id = %driver_user_id, "FCM: no tokens registered, skipping push");
                return;
            }
            Ok(tokens) => {
                match self.get_access_token().await {
                    Err(e) => {
                        tracing::warn!(err = %e, "FCM: failed to obtain Google access token");
                        return;
                    }
                    Ok(access_token) => {
                        for token in tokens {
                            if let Err(e) = self.send_fcm(&token, &access_token).await {
                                tracing::warn!(driver_id = %driver_user_id, err = %e, "FCM: send failed");
                            } else {
                                tracing::info!(driver_id = %driver_user_id, "FCM: push sent");
                            }
                        }
                    }
                }
            }
        }
    }

    // ── Private helpers ────────────────────────────────────────────────────

    async fn fetch_push_tokens(&self, user_id: Uuid) -> Result<Vec<String>, String> {
        let url = format!(
            "{}/internal/push-tokens?user_id={}&app=driver",
            self.identity_base_url.trim_end_matches('/'),
            user_id
        );
        let resp = self.http.get(&url).send().await
            .map_err(|e| format!("identity request: {e}"))?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(format!("identity {status}: {body}"));
        }
        let parsed: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| format!("identity parse: {e}"))?;
        let tokens = parsed
            .pointer("/data/tokens")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|t| t.as_str().map(String::from)).collect())
            .unwrap_or_default();
        Ok(tokens)
    }

    async fn get_access_token(&self) -> Result<String, String> {
        let mut guard = self.cached_token.lock().await;
        if let Some(ref cached) = *guard {
            if cached.valid_until > Instant::now() {
                return Ok(cached.token.clone());
            }
        }
        let token = self.fetch_fresh_access_token().await?;
        *guard = Some(CachedToken {
            token: token.clone(),
            valid_until: Instant::now() + Duration::from_secs(55 * 60),
        });
        Ok(token)
    }

    async fn fetch_fresh_access_token(&self) -> Result<String, String> {
        use jsonwebtoken::{encode, EncodingKey, Header, Algorithm};

        let now = chrono::Utc::now().timestamp();
        let claims = GoogleJwtClaims {
            iss: self.service_account.client_email.clone(),
            scope: "https://www.googleapis.com/auth/firebase.messaging".into(),
            aud: "https://oauth2.googleapis.com/token".into(),
            iat: now,
            exp: now + 3600,
        };
        let header = Header::new(Algorithm::RS256);
        let key = EncodingKey::from_rsa_pem(self.service_account.private_key.as_bytes())
            .map_err(|e| format!("invalid RSA key: {e}"))?;
        let jwt = encode(&header, &claims, &key)
            .map_err(|e| format!("JWT sign: {e}"))?;

        let resp = self.http
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("grant_type", "urn:ietf:params:oauth2:grant-type:jwt-bearer"),
                ("assertion", jwt.as_str()),
            ])
            .send()
            .await
            .map_err(|e| format!("Google token request: {e}"))?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(format!("Google token {status}: {body}"));
        }
        let token_resp: GoogleTokenResponse = serde_json::from_str(&body)
            .map_err(|e| format!("Google token parse: {e}"))?;
        Ok(token_resp.access_token)
    }

    async fn send_fcm(&self, device_token: &str, access_token: &str) -> Result<(), String> {
        let url = format!(
            "https://fcm.googleapis.com/v1/projects/{}/messages:send",
            self.project_id
        );
        let body = serde_json::json!({
            "message": {
                "token": device_token,
                "data": {
                    "type": "dispatch_message",
                    "title": "New task assigned",
                    "body": "You have a new delivery task"
                },
                "android": {
                    "priority": "HIGH"
                }
            }
        });
        let resp = self.http
            .post(&url)
            .bearer_auth(access_token)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("FCM request: {e}"))?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("FCM {status}: {text}"));
        }
        Ok(())
    }
}
