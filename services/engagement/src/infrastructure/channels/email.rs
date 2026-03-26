use async_trait::async_trait;
use super::ChannelAdapter;

pub struct SendGridEmailAdapter {
    api_key: String,
    from_email: String,
    from_name: String,
    client: reqwest::Client,
}

impl SendGridEmailAdapter {
    pub fn new(api_key: String, from_email: String, from_name: String) -> Self {
        Self { api_key, from_email, from_name, client: reqwest::Client::new() }
    }
}

#[async_trait]
impl ChannelAdapter for SendGridEmailAdapter {
    async fn send(&self, recipient: &str, body: &str, subject: Option<&str>) -> Result<String, String> {
        let payload = serde_json::json!({
            "personalizations": [{ "to": [{ "email": recipient }] }],
            "from": { "email": self.from_email, "name": self.from_name },
            "subject": subject.unwrap_or("LogisticOS Notification"),
            "content": [{ "type": "text/html", "value": body }]
        });
        let response = self.client
            .post("https://api.sendgrid.com/v3/mail/send")
            .bearer_auth(&self.api_key)
            .json(&payload)
            .send().await
            .map_err(|e| format!("SendGrid request failed: {e}"))?;

        if !response.status().is_success() {
            return Err(format!("SendGrid error: {}", response.text().await.unwrap_or_default()));
        }
        let msg_id = response.headers()
            .get("x-message-id")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown")
            .to_owned();
        Ok(msg_id)
    }
}
