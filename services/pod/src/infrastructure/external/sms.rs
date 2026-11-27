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
/// It reports failure, so the caller records that the PIN never reached the
/// recipient. It never logs the message body: that holds the plaintext PIN.
pub struct NoOpSmsAdapter;

/// Last four digits only, e.g. `***1234`.
pub fn mask_recipient(to: &str) -> String {
    let digits: Vec<char> = to.chars().filter(char::is_ascii_digit).collect();
    let tail: String = digits[digits.len().saturating_sub(4)..].iter().collect();
    format!("***{tail}")
}

#[async_trait]
impl SmsAdapter for NoOpSmsAdapter {
    async fn send(&self, to: &str, _body: &str) -> anyhow::Result<()> {
        tracing::warn!(
            to = %mask_recipient(to),
            "NoOpSmsAdapter: SMS not sent (no SMS provider configured). \
             Set TWILIO_ACCOUNT_SID / TWILIO_AUTH_TOKEN / TWILIO_FROM_NUMBER for real delivery."
        );
        Err(anyhow::anyhow!("no SMS provider configured"))
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
            // Never the raw body: Twilio echoes the To number back, and callers log this.
            match twilio_error_code(&body) {
                Some(code) => anyhow::bail!("Twilio SMS failed (HTTP {status}, Twilio code {code})"),
                None => anyhow::bail!("Twilio SMS failed (HTTP {status})"),
            }
        }

        tracing::info!(to = %mask_recipient(to), "OTP SMS sent via Twilio");
        Ok(())
    }
}

/// Twilio's numeric error `code` from an error body, if it parses out.
fn twilio_error_code(body: &str) -> Option<i64> {
    serde_json::from_str::<serde_json::Value>(body).ok()?.get("code")?.as_i64()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_twilios_numeric_code_survives_from_an_error_body() {
        let body = r#"{"code":21211,"message":"The 'To' number +639171234567 is not a valid phone number.","status":400}"#;
        assert_eq!(twilio_error_code(body), Some(21211));
        assert_eq!(twilio_error_code("<html>+639171234567</html>"), None);
        assert_eq!(twilio_error_code(r#"{"code":"x"}"#), None);
    }

    #[tokio::test]
    async fn the_noop_adapter_reports_that_nothing_was_sent() {
        let err = NoOpSmsAdapter.send("+639171234567", "Your PIN is 123456").await.unwrap_err();
        assert!(err.to_string().contains("no SMS provider configured"));
    }

    #[test]
    fn the_recipient_is_masked_to_the_last_four_digits() {
        assert_eq!(mask_recipient("+639171234567"), "***4567");
        assert_eq!(mask_recipient("12"), "***12");
        assert_eq!(mask_recipient(""), "***");
    }
}
