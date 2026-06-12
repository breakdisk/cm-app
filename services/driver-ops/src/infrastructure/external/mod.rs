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

// ── Task-assigned push payload ──────────────────────────────────────────────

/// Field bundle for the rich `task_assigned` FCM data push. The Android app's
/// DriverMessagingService reads these keys to populate AssignmentPayload and
/// render the offer card without a network round-trip.
#[derive(Debug, Clone)]
pub struct TaskAssignedPush {
    pub assignment_id:     String,
    pub shipment_id:       String,
    pub task_type:         String,
    pub customer_name:     String,
    pub merchant_name:     String,
    pub address:           String,
    pub tracking_number:   String,
    pub cod_amount_cents:  i64,
    pub delivery_category: String,
    pub weight_grams:      i64,
    pub pickup_lat:        Option<f64>,
    pub pickup_lng:        Option<f64>,
    pub delivery_lat:      Option<f64>,
    pub delivery_lng:      Option<f64>,
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

    /// Send a typed instruction push to a driver (e.g. "return_to_hub", "call_support").
    /// All errors are logged and swallowed — push is best-effort.
    pub async fn notify_driver_instruction(
        &self,
        driver_user_id: Uuid,
        instruction_type: &str,
        message: &str,
    ) {
        match self.fetch_push_tokens(driver_user_id).await {
            Err(e) => {
                tracing::warn!(driver_id = %driver_user_id, err = %e, "FCM: failed to fetch push tokens for instruction");
                return;
            }
            Ok(tokens) if tokens.is_empty() => {
                tracing::debug!(driver_id = %driver_user_id, "FCM: no tokens registered, skipping instruction push");
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
                            if let Err(e) = self.send_fcm_instruction(&token, &access_token, instruction_type, message).await {
                                tracing::warn!(driver_id = %driver_user_id, err = %e, "FCM: instruction send failed");
                            } else {
                                tracing::info!(driver_id = %driver_user_id, instruction_type, "FCM: instruction push sent");
                            }
                        }
                    }
                }
            }
        }
    }

    /// Rich task-assignment push: data map type = "task_assigned" with the full
    /// offer payload so the app renders the Accept/Decline card immediately.
    /// All errors are logged and swallowed — push is best-effort.
    pub async fn notify_task_assigned(&self, driver_user_id: Uuid, push: &TaskAssignedPush) {
        let tokens = match self.fetch_push_tokens(driver_user_id).await {
            Err(e) => {
                tracing::warn!(driver_id = %driver_user_id, err = %e, "FCM: failed to fetch push tokens");
                return;
            }
            Ok(tokens) if tokens.is_empty() => {
                tracing::debug!(driver_id = %driver_user_id, "FCM: no tokens registered, skipping push");
                return;
            }
            Ok(tokens) => tokens,
        };
        let access_token = match self.get_access_token().await {
            Err(e) => {
                tracing::warn!(err = %e, "FCM: failed to obtain Google access token");
                return;
            }
            Ok(t) => t,
        };

        // FCM data maps are string→string; numeric/optional fields serialized
        // as strings ("" for absent coordinates).
        let opt = |v: Option<f64>| v.map(|f| f.to_string()).unwrap_or_default();
        let data = serde_json::json!({
            "type":              "task_assigned",
            "title":             "New task offer",
            "body":              format!("{} — {}", push.merchant_name, push.address),
            "assignment_id":     push.assignment_id,
            "shipment_id":       push.shipment_id,
            "task_type":         push.task_type,
            "customer_name":     push.customer_name,
            "merchant_name":     push.merchant_name,
            "address":           push.address,
            "tracking_number":   push.tracking_number,
            "cod_amount_cents":  push.cod_amount_cents.to_string(),
            "delivery_category": push.delivery_category,
            "weight_grams":      push.weight_grams.to_string(),
            "pickup_lat":        opt(push.pickup_lat),
            "pickup_lng":        opt(push.pickup_lng),
            "delivery_lat":      opt(push.delivery_lat),
            "delivery_lng":      opt(push.delivery_lng),
        });

        for token in tokens {
            let body = serde_json::json!({
                "message": {
                    "token": token,
                    "data": data,
                    "android": { "priority": "HIGH" }
                }
            });
            if let Err(e) = self.send_fcm_raw(&body, &access_token).await {
                tracing::warn!(driver_id = %driver_user_id, err = %e, "FCM: task_assigned send failed");
            } else {
                tracing::info!(driver_id = %driver_user_id, "FCM: task_assigned push sent");
            }
        }
    }

    /// Generic data push — send an arbitrary string→string data map to all of
    /// a driver's registered devices. Used by the gig offer fan-out
    /// (`task_offer` / `offer_closed`). Best-effort: errors logged, swallowed.
    pub async fn notify_data(&self, driver_user_id: Uuid, data: &serde_json::Value) {
        let tokens = match self.fetch_push_tokens(driver_user_id).await {
            Err(e) => {
                tracing::warn!(driver_id = %driver_user_id, err = %e, "FCM: failed to fetch push tokens");
                return;
            }
            Ok(tokens) if tokens.is_empty() => {
                tracing::debug!(driver_id = %driver_user_id, "FCM: no tokens registered, skipping push");
                return;
            }
            Ok(tokens) => tokens,
        };
        let access_token = match self.get_access_token().await {
            Err(e) => {
                tracing::warn!(err = %e, "FCM: failed to obtain Google access token");
                return;
            }
            Ok(t) => t,
        };
        let kind = data.get("type").and_then(|v| v.as_str()).unwrap_or("data").to_string();
        for token in tokens {
            let body = serde_json::json!({
                "message": {
                    "token": token,
                    "data": data,
                    "android": { "priority": "HIGH" }
                }
            });
            if let Err(e) = self.send_fcm_raw(&body, &access_token).await {
                tracing::warn!(driver_id = %driver_user_id, push_type = %kind, err = %e, "FCM: data send failed");
            } else {
                tracing::info!(driver_id = %driver_user_id, push_type = %kind, "FCM: data push sent");
            }
        }
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

    async fn send_fcm_instruction(
        &self,
        device_token: &str,
        access_token: &str,
        instruction_type: &str,
        message: &str,
    ) -> Result<(), String> {
        let url = format!(
            "https://fcm.googleapis.com/v1/projects/{}/messages:send",
            self.project_id
        );
        let body = serde_json::json!({
            "message": {
                "token": device_token,
                "data": {
                    "type": "driver_instruction",
                    "instruction_type": instruction_type,
                    "title": "Dispatch instruction",
                    "body": message,
                },
                "android": { "priority": "HIGH" }
            }
        });
        let resp = self.http
            .post(&url)
            .bearer_auth(access_token)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("FCM instruction request: {e}"))?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("FCM instruction {status}: {text}"));
        }
        Ok(())
    }

    /// POST a pre-built FCM v1 message body. Shared by the typed senders.
    async fn send_fcm_raw(&self, body: &serde_json::Value, access_token: &str) -> Result<(), String> {
        let url = format!(
            "https://fcm.googleapis.com/v1/projects/{}/messages:send",
            self.project_id
        );
        let resp = self.http
            .post(&url)
            .bearer_auth(access_token)
            .json(body)
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
