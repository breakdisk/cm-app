use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebhookStatus {
    Active,
    Disabled,
}

impl WebhookStatus {
    pub fn as_str(self) -> &'static str {
        match self { Self::Active => "active", Self::Disabled => "disabled" }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s { "active" => Some(Self::Active), "disabled" => Some(Self::Disabled), _ => None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Webhook {
    pub id:                Uuid,
    pub tenant_id:         Uuid,
    pub url:               String,
    /// Subscribed event types. "*" means everything; otherwise exact match
    /// against `<source>.<event>` keys (e.g. "shipment.created").
    pub events:            Vec<String>,
    /// HMAC signing secret. Returned in plaintext exactly once at create
    /// time and is otherwise omitted from API responses (see WebhookDto).
    pub secret:            String,
    pub status:            WebhookStatus,
    pub description:       Option<String>,
    pub success_count:     i64,
    pub fail_count:        i64,
    pub last_delivery_at:  Option<DateTime<Utc>>,
    pub last_status_code:  Option<i32>,
    pub created_at:        DateTime<Utc>,
    pub updated_at:        DateTime<Utc>,
}

impl Webhook {
    /// Returns true if this webhook subscribes to the given event type.
    /// `"*"` is the catch-all marker; otherwise the type must appear in
    /// `events` exactly. Matching is case-sensitive (Kafka event names
    /// are canonical lowercase-dotted).
    pub fn matches(&self, event_type: &str) -> bool {
        if self.status != WebhookStatus::Active { return false; }
        self.events.iter().any(|e| e == "*" || e == event_type)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryAttempt {
    pub id:            Uuid,
    pub webhook_id:    Uuid,
    pub tenant_id:     Uuid,
    pub event_type:    String,
    pub payload:       serde_json::Value,
    pub attempt:       i32,
    /// 0 means the request never left this side (timeout/DNS/TLS).
    pub status_code:   i32,
    pub response_body: Option<String>,
    pub duration_ms:   i32,
    pub delivered_at:  DateTime<Utc>,
}
