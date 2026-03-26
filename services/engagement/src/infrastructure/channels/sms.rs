use async_trait::async_trait;
use super::ChannelAdapter;

pub struct TwilioSmsAdapter {
    account_sid: String,
    auth_token: String,
    from_number: String,
    client: reqwest::Client,
}

impl TwilioSmsAdapter {
    pub fn new(account_sid: String, auth_token: String, from_number: String) -> Self {
        Self { account_sid, auth_token, from_number, client: reqwest::Client::new() }
    }
}

#[async_trait]
impl ChannelAdapter for TwilioSmsAdapter {
    async fn send(&self, recipient: &str, body: &str, _subject: Option<&str>) -> Result<String, String> {
        let url = format!(
            "https://api.twilio.com/2010-04-01/Accounts/{}/Messages.json",
            self.account_sid
        );
        let params = [("From", self.from_number.as_str()), ("To", recipient), ("Body", body)];
        let response = self.client
            .post(&url)
            .basic_auth(&self.account_sid, Some(&self.auth_token))
            .form(&params)
            .send().await
            .map_err(|e| format!("Twilio SMS failed: {e}"))?;

        if !response.status().is_success() {
            return Err(format!("Twilio SMS error: {}", response.text().await.unwrap_or_default()));
        }
        let json: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
        Ok(json["sid"].as_str().unwrap_or("unknown").to_owned())
    }
}
