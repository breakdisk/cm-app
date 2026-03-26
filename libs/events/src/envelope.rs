use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// CloudEvents-compatible event envelope.
/// Every domain event produced by any service uses this wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event<T: Serialize> {
    /// Unique event ID — used for idempotency checks by consumers.
    pub id: Uuid,
    /// Originating service, e.g. "logisticos/order-intake"
    pub source: String,
    /// Dot-separated event type, e.g. "shipment.created"
    pub event_type: String,
    pub time: DateTime<Utc>,
    pub tenant_id: Uuid,
    pub data: T,
}

impl<T: Serialize> Event<T> {
    pub fn new(source: &str, event_type: &str, tenant_id: Uuid, data: T) -> Self {
        Self {
            id: Uuid::new_v4(),
            source: source.to_owned(),
            event_type: event_type.to_owned(),
            time: Utc::now(),
            tenant_id,
            data,
        }
    }
}
