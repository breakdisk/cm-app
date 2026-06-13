use async_trait::async_trait;
use super::ChannelAdapter;

/// Sends WhatsApp messages via the Meta Cloud API (direct-to-Meta).
///
/// Env vars required:
///   META_WHATSAPP_ACCESS_TOKEN     — permanent system-user token from Meta Business Manager
///   META_WHATSAPP_PHONE_NUMBER_ID  — the registered WhatsApp Business phone number ID
pub struct MetaWhatsAppAdapter {
    access_token:    String,
    phone_number_id: String,
    client:          reqwest::Client,
}

impl MetaWhatsAppAdapter {
    pub fn new(access_token: String, phone_number_id: String) -> Self {
        Self { access_token, phone_number_id, client: reqwest::Client::new() }
    }
}

#[async_trait]
impl ChannelAdapter for MetaWhatsAppAdapter {
    async fn send(
        &self,
        recipient: &str,
        body: &str,
        _subject: Option<&str>,
        _data: Option<&serde_json::Value>,
    ) -> Result<String, String> {
        let url = format!(
            "https://graph.facebook.com/v20.0/{}/messages",
            self.phone_number_id
        );

        // Meta expects E.164 format without the "whatsapp:" prefix.
        let to = recipient.strip_prefix("whatsapp:").unwrap_or(recipient);

        let payload = serde_json::json!({
            "messaging_product": "whatsapp",
            "to": to,
            "type": "text",
            "text": { "body": body }
        });

        let response = self.client
            .post(&url)
            .bearer_auth(&self.access_token)
            .json(&payload)
            .send().await
            .map_err(|e| format!("Meta WhatsApp request failed: {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(format!("Meta WhatsApp error {status}: {text}"));
        }

        let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
        Ok(json["messages"][0]["id"].as_str().unwrap_or("unknown").to_owned())
    }
}
