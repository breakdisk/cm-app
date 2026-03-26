//! SMS adapter for sending OTP codes to delivery recipients.
//! Uses the Twilio REST API (same credentials as the engagement service).

use async_trait::async_trait;

#[async_trait]
pub trait SmsAdapter: Send + Sync {
    async fn send(&self, to: &str, body: &str) -> anyhow::Result<()>;
}

pub struct TwilioSmsAdapter {
    client: reqwest::Client,
    account_sid: String,
    auth_token: String,
    from_number: String,
}

impl TwilioSmsAdapter {
    pub fn new(account_sid: String, auth_token: String, from_number: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            account_sid,
            auth_token,
            from_number,
        }
    }
}

#[async_trait]
impl SmsAdapter for TwilioSmsAdapter {
    async fn send(&self, to: &str, body: &str) -> anyhow::Result<()> {
        let url = format!(
            "https://api.twilio.com/2010-04-01/Accounts/{}/Messages.json",
            self.account_sid
        );

        let response = self.client
            .post(&url)
            .basic_auth(&self.account_sid, Some(&self.auth_token))
            .form(&[
                ("To",   to),
                ("From", &self.from_number),
                ("Body", body),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Twilio SMS failed (HTTP {}): {}", status, body);
        }

        tracing::info!(to = %to, "OTP SMS sent via Twilio");
        Ok(())
    }
}
