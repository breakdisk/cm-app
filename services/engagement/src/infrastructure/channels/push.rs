//! ExpoPushAdapter — delivers push notifications via Expo Push API.
//!
//! Service isolation: engagement does NOT touch the identity database directly.
//! Push tokens are fetched via identity's internal endpoint
//! `GET /internal/push-tokens?user_id=<uuid>&app=customer`. This endpoint is
//! protected by Docker network isolation (not exposed through the API gateway).
//!
//! The `recipient` field passed to `send()` is the customer's user_id as a string.

use async_trait::async_trait;
use serde_json::json;
use uuid::Uuid;

use super::ChannelAdapter;

const EXPO_PUSH_URL: &str = "https://exp.host/--/api/v2/push/send";

pub struct ExpoPushAdapter {
    identity_base_url: String,
    client: reqwest::Client,
}

impl ExpoPushAdapter {
    pub fn new(identity_base_url: String) -> Self {
        Self {
            identity_base_url,
            client: reqwest::Client::new(),
        }
    }

    async fn fetch_tokens(&self, user_id: Uuid) -> Result<Vec<String>, String> {
        let url = format!(
            "{}/internal/push-tokens?user_id={}&app=customer",
            self.identity_base_url.trim_end_matches('/'),
            user_id
        );
        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("identity push-tokens request failed: {e}"))?;

        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(format!("identity push-tokens {status}: {body_text}"));
        }

        let parsed: serde_json::Value = serde_json::from_str(&body_text)
            .map_err(|e| format!("identity push-tokens parse failed: {e}"))?;

        let tokens = parsed
            .get("data")
            .and_then(|d| d.get("tokens"))
            .and_then(|t| t.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect::<Vec<_>>())
            .unwrap_or_default();
        Ok(tokens)
    }
}

#[async_trait]
impl ChannelAdapter for ExpoPushAdapter {
    async fn send(&self, recipient: &str, body: &str, subject: Option<&str>) -> Result<String, String> {
        let user_id = Uuid::parse_str(recipient)
            .map_err(|e| format!("invalid user_id '{recipient}': {e}"))?;

        let tokens = self.fetch_tokens(user_id).await?;
        if tokens.is_empty() {
            return Err(format!("no push tokens registered for user {user_id}"));
        }

        let title = subject.unwrap_or("LogisticOS");
        let messages: Vec<_> = tokens.iter().map(|t| json!({
            "to": t,
            "title": title,
            "body": body,
            "sound": "default",
            "priority": "high",
        })).collect();

        let response = self.client
            .post(EXPO_PUSH_URL)
            .header("accept", "application/json")
            .header("accept-encoding", "gzip, deflate")
            .header("content-type", "application/json")
            .json(&messages)
            .send()
            .await
            .map_err(|e| format!("Expo push request failed: {e}"))?;

        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(format!("Expo push error {status}: {body_text}"));
        }

        let parsed: serde_json::Value = serde_json::from_str(&body_text)
            .map_err(|e| format!("Expo push parse failed: {e}"))?;

        if let Some(data) = parsed.get("data").and_then(|d| d.as_array()) {
            let first_error = data.iter().find(|t| t.get("status").and_then(|s| s.as_str()) == Some("error"));
            if let Some(err) = first_error {
                return Err(format!("Expo ticket error: {err}"));
            }
            let first_id = data.first()
                .and_then(|t| t.get("id"))
                .and_then(|id| id.as_str())
                .unwrap_or("expo-push-ok");
            return Ok(first_id.to_owned());
        }
        Ok("expo-push-ok".into())
    }
}
