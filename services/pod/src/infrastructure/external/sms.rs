//! SMS adapter for sending OTP codes to delivery recipients.
//! Uses the Twilio REST API (same credentials as the engagement service).
//! Falls back to a no-op adapter when Twilio credentials are not configured
//! so the pod service can boot and handle photo/signature PODs in environments
//! (e.g. staging) that have no SMS provider configured.

use async_trait::async_trait;

#[async_trait]
pub trait SmsAdapter: Send + Sync {
    async fn send(&self, to: &str, body: &str) -> anyhow::Result<()>;
}

/// No-op SMS adapter used when Twilio credentials are not configured.
/// OTP generation will succeed but no SMS will be sent; the driver must
/// relay the OTP out-of-band (or use the dev bypass).
pub struct NoOpSmsAdapter;

#[async_trait]
impl SmsAdapter for NoOpSmsAdapter {
    async fn send(&self, to: &str, _body: &str) -> anyhow::Result<()> {
        tracing::warn!(
            to = %to,
            "NoOpSmsAdapter: TWILIO_ACCOUNT_SID not configured — SMS not sent. \
             Set TWILIO_ACCOUNT_SID / TWILIO_AUTH_TOKEN / TWILIO_FROM_NUMBER to enable real OTP delivery."
        );
        Ok(())
    }
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
