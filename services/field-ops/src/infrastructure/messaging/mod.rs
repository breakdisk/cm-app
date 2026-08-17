//! Courier milestone events.
//!
//! field-ops publishes what happened to a courier. It does not know what the
//! consuming product does with it — `external_ref` is opaque, which is what
//! keeps this a platform tier rather than a LogisticOS or OmniDeliv service.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const TOPIC_COURIER: &str = "fieldops.courier";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum CourierEvent {
    Assigned  { tenant_id: Uuid, product: String, external_ref: Uuid, courier_id: Uuid, assignment_id: Uuid,
                /// The identity user behind the courier, so a consuming product
                /// can authorize that user against this job without asking us
                /// on every read. `courier_id` is this tier's own key and is a
                /// different uuid, so it cannot be compared to a JWT's subject.
                ///
                /// `Option` + `serde(default)`: messages published before this
                /// field existed are still inside the retention window, and a
                /// variant the consumer cannot deserialize fails *every*
                /// message on the partition, not just the new kind.
                #[serde(default)] courier_user_id: Option<Uuid> },
    Collected { tenant_id: Uuid, product: String, external_ref: Uuid, courier_id: Uuid, vendor_id: Uuid,
                device_timestamp: Option<chrono::DateTime<chrono::Utc>> },
    Delivered { tenant_id: Uuid, product: String, external_ref: Uuid, courier_id: Uuid,
                device_timestamp: Option<chrono::DateTime<chrono::Utc>> },
}

impl CourierEvent {
    /// Partition key. Keying by `external_ref` means every event for one job
    /// lands on one partition and therefore arrives in order — without that,
    /// `Delivered` can overtake `Collected` and the consumer's state machine
    /// correctly refuses it.
    ///
    /// Deliberately not the tenant id, which `publish_event` would use: that
    /// also gives ordering, but by serialising an entire tenant onto one
    /// partition, which costs all parallelism to buy a guarantee already had
    /// at job granularity.
    pub fn key(&self) -> Uuid {
        match self {
            CourierEvent::Assigned { external_ref, .. }
            | CourierEvent::Collected { external_ref, .. }
            | CourierEvent::Delivered { external_ref, .. } => *external_ref,
        }
    }
}

/// Publishing courier milestones. A trait so the dispatch service can be tested
/// without a broker, and so a deployment without Kafka can drop in a no-op.
#[async_trait::async_trait]
pub trait CourierEvents: Send + Sync {
    async fn publish(&self, event: &CourierEvent) -> anyhow::Result<()>;
}

pub struct KafkaCourierEvents {
    producer: Arc<logisticos_events::producer::KafkaProducer>,
}

impl KafkaCourierEvents {
    pub fn new(producer: Arc<logisticos_events::producer::KafkaProducer>) -> Self {
        Self { producer }
    }
}

#[async_trait::async_trait]
impl CourierEvents for KafkaCourierEvents {
    async fn publish(&self, event: &CourierEvent) -> anyhow::Result<()> {
        self.producer
            .publish_raw(TOPIC_COURIER, &event.key().to_string(), &serde_json::to_string(event)?)
            .await
    }
}

/// Drops every event. For a deployment with no broker, where losing milestones
/// is preferable to refusing claims — the claim itself is already committed.
pub struct NoopCourierEvents;

#[async_trait::async_trait]
impl CourierEvents for NoopCourierEvents {
    async fn publish(&self, _event: &CourierEvent) -> anyhow::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every variant keys on the job, not the courier or the tenant. If one
    /// variant keyed differently its events would land on another partition and
    /// could overtake the rest of the job's timeline.
    #[test]
    fn every_variant_keys_on_the_job() {
        let job = Uuid::new_v4();
        let tenant = Uuid::new_v4();
        let courier = Uuid::new_v4();

        let events = [
            CourierEvent::Assigned { tenant_id: tenant, product: "omnideliv".into(),
                                     external_ref: job, courier_id: courier, assignment_id: Uuid::new_v4(),
                                     courier_user_id: Some(Uuid::new_v4()) },
            CourierEvent::Collected { tenant_id: tenant, product: "omnideliv".into(),
                                      external_ref: job, courier_id: courier, vendor_id: Uuid::new_v4(),
                                      device_timestamp: None },
            CourierEvent::Delivered { tenant_id: tenant, product: "omnideliv".into(),
                                      external_ref: job, courier_id: courier, device_timestamp: None },
        ];

        for e in &events {
            assert_eq!(e.key(), job);
        }
    }

    /// The consumer matches on the tagged `event` field, so these names are a
    /// wire contract between two services.
    #[test]
    fn events_serialise_with_a_snake_case_tag() {
        let e = CourierEvent::Collected {
            tenant_id: Uuid::nil(), product: "omnideliv".into(), external_ref: Uuid::nil(),
            courier_id: Uuid::nil(), vendor_id: Uuid::nil(), device_timestamp: None,
        };
        let v = serde_json::to_value(&e).expect("serialise");
        assert_eq!(v["event"], "collected");
    }
}
