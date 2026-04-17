//! LogChannelAdapter — a no-send adapter that logs the rendered notification
//! at INFO level and returns a synthetic provider id. Wired in place of a
//! real channel adapter (Twilio, SES, Expo) when that channel's credentials
//! are missing or set to placeholder values.
//!
//! Purpose: during MVP/pre-prod the engagement service still needs to *prove*
//! that events flow end-to-end through the template → dispatch → channel
//! path. Failing the real adapter (401 from Twilio, etc.) would mark the
//! notification failed and muddy the signal. The log adapter succeeds,
//! prints the full body to container stdout, and thereby makes booking a
//! test shipment on prod a working verification harness.

use async_trait::async_trait;
use super::ChannelAdapter;

pub struct LogChannelAdapter {
    channel: &'static str,
}

impl LogChannelAdapter {
    pub fn new(channel: &'static str) -> Self {
        Self { channel }
    }
}

#[async_trait]
impl ChannelAdapter for LogChannelAdapter {
    async fn send(&self, recipient: &str, body: &str, subject: Option<&str>) -> Result<String, String> {
        tracing::info!(
            channel = self.channel,
            recipient = recipient,
            subject = subject.unwrap_or(""),
            body_len = body.len(),
            body = body,
            "LogChannelAdapter: would send notification (no credentials configured)",
        );
        Ok(format!("log-{}-{}", self.channel, uuid::Uuid::new_v4()))
    }
}
