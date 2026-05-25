pub mod email;
pub mod log_adapter;
pub mod push;
pub mod sms;
pub mod whatsapp;

use async_trait::async_trait;

#[async_trait]
pub trait ChannelAdapter: Send + Sync {
    /// Dispatch a single notification.
    ///
    /// - `data`: optional channel-specific metadata (e.g. `{"deep_link": "/tracking"}`
    ///   for push). Adapters that don't use it should accept and ignore the parameter.
    async fn send(
        &self,
        recipient: &str,
        body:      &str,
        subject:   Option<&str>,
        data:      Option<&serde_json::Value>,
    ) -> Result<String, String>;
}
