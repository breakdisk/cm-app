pub mod email;
pub mod push;
pub mod sms;
pub mod whatsapp;

use async_trait::async_trait;

#[async_trait]
pub trait ChannelAdapter: Send + Sync {
    async fn send(&self, recipient: &str, body: &str, subject: Option<&str>) -> Result<String, String>;
}
