//! Courier milestone events.
//!
//! field-ops publishes what happened to a courier. It does not know what the
//! consuming product does with it — `external_ref` is opaque, which is what
//! keeps this a platform tier rather than a LogisticOS or OmniDeliv service.

pub mod compliance_consumer;

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const TOPIC_COURIER: &str = "fieldops.courier";

/// The topic the compliance service consumes registrations on.
///
/// Owned by compliance, not by this tier: it subscribes to `driver` looking for
/// `driver.registered`, and until now **nothing in the platform ever published
/// it**. That consumer has been running against a topic with no producer since
/// the service shipped, which is why no field worker has ever had a compliance
/// profile created for them.
pub const TOPIC_DRIVER: &str = "driver";

/// Wire contract with the compliance service's `driver.registered` consumer.
///
/// `tenant_id` is duplicated inside the payload even though the envelope
/// already carries it. That is compliance's shape, and its struct declares the
/// field without `#[serde(default)]` — omitting it here would fail
/// deserialisation on their side, which surfaces as a warning log on their
/// consumer and total silence on ours.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverRegisteredPayload {
    /// The identity user. Compliance stores this as the profile's `entity_id`,
    /// and ADR-0015 makes it equal to `courier.id`.
    pub driver_id:    Uuid,
    pub tenant_id:    Uuid,
    pub jurisdiction: String,
}

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
    /// The courier is at a stop. Published, never persisted: it changes no
    /// assignment state, and a milestone that only informs does not need a row.
    ///
    /// `stop_ref` is opaque. The offering product sets it — OmniDeliv uses the
    /// vendor id for a pickup and the order id for the dropoff — and this tier
    /// never resolves it, exactly as it never resolves `external_ref`.
    Arrived   { tenant_id: Uuid, product: String, external_ref: Uuid, courier_id: Uuid,
                stop_ref: Uuid,
                device_timestamp: Option<chrono::DateTime<chrono::Utc>> },
    Collected { tenant_id: Uuid, product: String, external_ref: Uuid, courier_id: Uuid, vendor_id: Uuid,
                device_timestamp: Option<chrono::DateTime<chrono::Utc>> },
    Delivered { tenant_id: Uuid, product: String, external_ref: Uuid, courier_id: Uuid,
                device_timestamp: Option<chrono::DateTime<chrono::Utc>> },
    /// A courier reported that a delivery could not be completed.
    ///
    /// Unlike the other four this one is persisted as well as published - see
    /// migration 0010. The event exists so the offering product can flag its
    /// own order: OmniDeliv needs to know an order needs a human, and D1 says
    /// the refund decision is made out of band rather than here.
    ///
    /// Carries no money and implies no state change. A consumer that treats
    /// this as a terminal status is wrong: the assignment is still `claimed`.
    ExceptionRaised { tenant_id: Uuid, product: String, external_ref: Uuid, courier_id: Uuid,
                      exception_id: Uuid, reason: String,
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
            | CourierEvent::Arrived { external_ref, .. }
            | CourierEvent::Collected { external_ref, .. }
            | CourierEvent::Delivered { external_ref, .. }
            | CourierEvent::ExceptionRaised { external_ref, .. } => *external_ref,
        }
    }
}

/// Publishing courier milestones. A trait so the dispatch service can be tested
/// without a broker, and so a deployment without Kafka can drop in a no-op.
#[async_trait::async_trait]
pub trait CourierEvents: Send + Sync {
    async fn publish(&self, event: &CourierEvent) -> anyhow::Result<()>;

    /// Announce a newly registered courier so compliance opens a profile.
    ///
    /// A separate method rather than a `CourierEvent` variant because it is a
    /// different contract in every respect: a different topic, owned by another
    /// service, and the full `Event<T>` envelope rather than the bare tagged
    /// enum the milestone stream uses. Folding it into `CourierEvent` would
    /// mean one of the two shapes had to bend.
    async fn publish_registered(
        &self,
        tenant_id:    Uuid,
        user_id:      Uuid,
        jurisdiction: &str,
    ) -> anyhow::Result<()>;
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

    async fn publish_registered(
        &self,
        tenant_id:    Uuid,
        user_id:      Uuid,
        jurisdiction: &str,
    ) -> anyhow::Result<()> {
        let event = logisticos_events::envelope::Event::new(
            "logisticos/field-ops",
            "driver.registered",
            tenant_id,
            DriverRegisteredPayload {
                driver_id: user_id,
                tenant_id,
                jurisdiction: jurisdiction.to_owned(),
            },
        );
        self.producer.publish_event(TOPIC_DRIVER, &event).await
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

    async fn publish_registered(&self, _: Uuid, _: Uuid, _: &str) -> anyhow::Result<()> {
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

    /// Field names compliance's `DriverRegisteredPayload` declares. It has no
    /// `#[serde(default)]` on any of them, so a rename here does not fail a
    /// build — it makes their consumer log a deserialisation warning and create
    /// no profile, which looks exactly like no courier ever registering.
    #[test]
    fn a_registration_payload_matches_the_shape_compliance_declares() {
        let v = serde_json::to_value(DriverRegisteredPayload {
            driver_id:    Uuid::nil(),
            tenant_id:    Uuid::nil(),
            jurisdiction: "PH".into(),
        })
        .expect("serialise");

        assert!(v.get("driver_id").is_some(),   "compliance reads data.driver_id");
        assert!(v.get("tenant_id").is_some(),   "compliance reads data.tenant_id");
        assert!(v.get("jurisdiction").is_some(), "compliance reads data.jurisdiction");
    }

    /// The event type string compliance matches on verbatim. Anything else and
    /// it takes the `Unrecognised driver event type` branch and drops the
    /// message.
    #[test]
    fn the_registration_envelope_is_typed_driver_registered() {
        let e = logisticos_events::envelope::Event::new(
            "logisticos/field-ops",
            "driver.registered",
            Uuid::nil(),
            DriverRegisteredPayload {
                driver_id: Uuid::nil(), tenant_id: Uuid::nil(), jurisdiction: "PH".into(),
            },
        );
        let v = serde_json::to_value(&e).expect("serialise");
        assert_eq!(v["event_type"], "driver.registered");
        assert_eq!(v["data"]["jurisdiction"], "PH");
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
