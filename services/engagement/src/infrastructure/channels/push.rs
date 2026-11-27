//! ExpoPushAdapter — delivers push notifications via Expo Push API.
//!
//! Service isolation: engagement does NOT touch the identity database directly.
//! Push tokens are fetched via identity's internal endpoint
//! `GET /internal/push-tokens?user_id=<uuid>&app=customer`. This endpoint is
//! protected by Docker network isolation (not exposed through the API gateway).
//!
//! The `recipient` passed to `send()` is the customer's user_id, or — for a
//! campaign, which addresses a CDP profile — a contact form
//! `contact:<tenant_id>:<phone>:<email>` (either may be empty) that identity
//! resolves to the logins holding that phone or email.

use async_trait::async_trait;
use serde_json::json;
use uuid::Uuid;

use super::ChannelAdapter;

const EXPO_PUSH_URL: &str = "https://exp.host/--/api/v2/push/send";

/// Who a push is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recipient {
    User(Uuid),
    Contact { tenant_id: Uuid, phone: String, email: String },
}

impl Recipient {
    /// A contact recipient for a campaign send: None when there is no
    /// address to find the customer by.
    pub fn contact(tenant_id: Uuid, phone: &str, email: &str) -> Option<String> {
        let (phone, email) = (phone.trim(), email.trim());
        (!phone.is_empty() || !email.is_empty()).then(|| format!("contact:{tenant_id}:{phone}:{email}"))
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        if let Some(rest) = s.strip_prefix("contact:") {
            let mut parts = rest.splitn(3, ':');
            let tenant_id = parts.next().and_then(|t| Uuid::parse_str(t).ok()).ok_or("contact recipient without a tenant")?;
            let phone = parts.next().unwrap_or_default().to_owned();
            let email = parts.next().unwrap_or_default().to_owned();
            if phone.is_empty() && email.is_empty() {
                return Err("contact recipient without a phone or email".into());
            }
            return Ok(Self::Contact { tenant_id, phone, email });
        }
        Uuid::parse_str(s).map(Self::User).map_err(|e| format!("invalid user_id '{s}': {e}"))
    }
}

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

    async fn fetch_tokens(&self, recipient: &Recipient) -> Result<Vec<String>, String> {
        let url = format!("{}/internal/push-tokens", self.identity_base_url.trim_end_matches('/'));
        let query: Vec<(&str, String)> = match recipient {
            Recipient::User(id) => vec![("app", "customer".into()), ("user_id", id.to_string())],
            Recipient::Contact { tenant_id, phone, email } => {
                let mut q = vec![("app", "customer".into()), ("tenant_id", tenant_id.to_string())];
                if !phone.is_empty() { q.push(("phone", phone.clone())); }
                if !email.is_empty() { q.push(("email", email.clone())); }
                q
            }
        };
        let response = self.client
            .get(&url)
            .query(&query)
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
    async fn send(&self, recipient: &str, body: &str, subject: Option<&str>, data: Option<&serde_json::Value>) -> Result<String, String> {
        let target = Recipient::parse(recipient)?;
        let tokens = self.fetch_tokens(&target).await?;
        if tokens.is_empty() {
            return Err(format!("no push tokens registered for {recipient}"));
        }

        let title = subject.unwrap_or("LogisticOS");
        let messages: Vec<_> = tokens.iter().map(|t| {
            let mut msg = json!({
                "to":       t,
                "title":    title,
                "body":     body,
                "sound":    "default",
                "priority": "high",
            });
            // Attach deep_link (and any other keys) into the Expo `data` field
            // so the customer app can navigate directly to the right screen.
            if let Some(d) = data {
                if d.as_object().is_some_and(|o| !o.is_empty()) {
                    msg["data"] = d.clone();
                }
            }
            msg
        }).collect();

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

#[cfg(test)]
mod recipient_tests {
    use super::*;

    #[test]
    fn a_user_id_and_a_contact_both_parse() {
        let id = Uuid::new_v4();
        assert_eq!(Recipient::parse(&id.to_string()), Ok(Recipient::User(id)));
        let c = Recipient::contact(id, "+639175550123", "ana@example.com").unwrap();
        assert_eq!(
            Recipient::parse(&c),
            Ok(Recipient::Contact { tenant_id: id, phone: "+639175550123".into(), email: "ana@example.com".into() })
        );
    }

    #[test]
    fn a_contact_needs_something_to_find_by() {
        let id = Uuid::new_v4();
        assert_eq!(Recipient::contact(id, " ", ""), None);
        assert!(Recipient::parse(&format!("contact:{id}::")).is_err());
        assert!(Recipient::parse("contact:not-a-uuid:+63:").is_err());
    }
}

