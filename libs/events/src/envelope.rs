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

    /// Like [`Event::new`] but with a caller-chosen id. For events that may be
    /// re-published (e.g. replay of an idempotent operation) and must carry the
    /// SAME id each time so consumers deduping on `id` see one event.
    pub fn with_id(id: Uuid, source: &str, event_type: &str, tenant_id: Uuid, data: T) -> Self {
        Self {
            id,
            source: source.to_owned(),
            event_type: event_type.to_owned(),
            time: Utc::now(),
            tenant_id,
            data,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_id_uses_the_given_id() {
        let id = Uuid::new_v4();
        let e = Event::with_id(id, "s", "t", Uuid::new_v4(), 1u8);
        assert_eq!(e.id, id);
        assert_ne!(Event::new("s", "t", Uuid::new_v4(), 1u8).id, id);
    }
}
