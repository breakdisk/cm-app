use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use super::template::NotificationChannel;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: uuid::Uuid,
    pub tenant_id: uuid::Uuid,
    pub customer_id: uuid::Uuid,
    pub channel: NotificationChannel,
    pub recipient: String,
    pub template_id: String,
    pub rendered_body: String,
    pub subject: Option<String>,
    pub status: NotificationStatus,
    pub priority: NotificationPriority,
    pub provider_message_id: Option<String>,
    pub error_message: Option<String>,
    pub queued_at: DateTime<Utc>,
    pub sent_at: Option<DateTime<Utc>>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub retry_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum NotificationStatus { Queued, Sending, Sent, Delivered, Failed, Bounced }

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum NotificationPriority { Low = 1, Normal = 2, High = 3 }

impl Notification {
    /// Business rule: max 3 retries before marking permanently failed.
    pub fn can_retry(&self) -> bool {
        self.retry_count < 3 && self.status == NotificationStatus::Failed
    }

    pub fn mark_sent(&mut self, provider_id: String) {
        self.status = NotificationStatus::Sent;
        self.provider_message_id = Some(provider_id);
        self.sent_at = Some(Utc::now());
    }

    pub fn mark_failed(&mut self, error: String) {
        self.status = NotificationStatus::Failed;
        self.error_message = Some(error);
        self.retry_count += 1;
    }
}
