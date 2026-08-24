//! Keeps each courier's compliance verdict on their row.
//!
//! The compliance service is the authority on whether a field worker may be
//! assigned work. This consumer copies its verdict onto `field_ops.couriers` so
//! the supply query can filter on it in SQL, and so the ops roster can *show*
//! it — see migration 0009 for why a column rather than the Redis cache the
//! sibling dispatch tier uses.
//!
//! WHAT THE GATE DOES NOT DO, deliberately.
//! It withholds *new offers*. It does not reach into work already in flight: a
//! courier blocked mid-job still completes it, and a courier who was offered a
//! job moments before the verdict arrived can still claim it. Both are the
//! right call — the alternative is a parcel stranded in a stranger's bag
//! because a document expired between pickup and doorstep, and a courier who
//! did the work not being credited for it. Compliance is a rule about who may
//! be *given* work, not a kill switch on custody.
//!
//! If a courier must be stopped immediately, that is what ops suspension
//! (`is_active`) is for; it is a different lever on purpose.

use std::sync::Arc;

use rdkafka::{
    config::ClientConfig,
    consumer::{CommitMode, Consumer, StreamConsumer},
    message::Message,
};
use tokio::sync::watch;
use uuid::Uuid;

use crate::domain::repositories::CourierRepository;

pub const TOPIC_COMPLIANCE: &str = "compliance";

/// Compliance's `ComplianceStatusChangedPayload`, narrowed to the fields this
/// tier acts on.
///
/// `is_assignable` is read and stored verbatim, never re-derived from
/// `new_status`. Which statuses are assignable is compliance's rule — `expired`
/// is one of them, because there is a grace period — and a second copy of that
/// rule here is how two services start disagreeing about who may work.
#[derive(Debug, serde::Deserialize)]
struct StatusChangedPayload {
    entity_id:     Uuid,
    entity_type:   String,
    new_status:    String,
    is_assignable: bool,
}

/// The `Event<T>` envelope, narrowed.
///
/// `tenant_id` is taken from the envelope: the update must be tenant-scoped,
/// and this schema has no row-level security to fall back on (migration 0001) —
/// the `WHERE tenant_id` in the repository is the whole isolation story.
#[derive(Debug, serde::Deserialize)]
struct ComplianceEvent {
    event_type: String,
    tenant_id:  Uuid,
    data:       StatusChangedPayload,
}

/// Does this event concern a courier this tier should act on?
///
/// Compliance has one entity type for people who carry things — its
/// `entity_kind_for` maps both the `driver` and `courier` roles to `"driver"` —
/// so `driver` is the type to match, not `courier`. `customer` events on the
/// same topic are KYC and none of this tier's business.
///
/// A free function so the rule is unit-testable without a broker.
fn concerns_a_courier(event_type: &str, entity_type: &str) -> bool {
    event_type == "compliance.status_changed" && entity_type == "driver"
}

pub async fn start_compliance_consumer(
    brokers:  &str,
    group_id: &str,
    couriers: Arc<dyn CourierRepository>,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", group_id)
        .set("enable.auto.commit", "false")
        // `earliest`, matching compliance's own consumer. A courier blocked
        // while this service was down must still be blocked when it comes back;
        // `latest` would silently skip every verdict issued during the outage
        // and leave the row saying they are fine.
        .set("auto.offset.reset", "earliest")
        .create()?;

    consumer.subscribe(&[TOPIC_COMPLIANCE])?;
    tracing::info!(topic = TOPIC_COMPLIANCE, %group_id, "compliance consumer subscribed");

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow_and_update() {
                    tracing::info!("compliance consumer shutting down");
                    break;
                }
            }
            result = consumer.recv() => {
                match result {
                    Err(e) => tracing::warn!("compliance Kafka recv error: {e}"),
                    Ok(msg) => {
                        match msg.payload_view::<str>() {
                            None           => tracing::warn!("compliance event has no payload — skipping"),
                            Some(Err(e))   => tracing::warn!("compliance event payload is not UTF-8: {e}"),
                            Some(Ok(body)) => handle(body, couriers.as_ref()).await,
                        }
                        if let Err(e) = consumer.commit_message(&msg, CommitMode::Async) {
                            tracing::error!("failed to commit compliance offset: {e}");
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

async fn handle(body: &str, couriers: &dyn CourierRepository) {
    let event: ComplianceEvent = match serde_json::from_str(body) {
        Ok(e) => e,
        Err(e) => {
            // Not worth a warning: compliance publishes four event types on
            // this topic and three of them have payloads this struct cannot
            // read. Logging those at warn would cry wolf on healthy traffic.
            tracing::debug!("compliance event this tier does not read: {e}");
            return;
        }
    };

    if !concerns_a_courier(&event.event_type, &event.data.entity_type) {
        return;
    }

    match couriers
        .update_compliance(
            event.tenant_id,
            event.data.entity_id,
            &event.data.new_status,
            event.data.is_assignable,
        )
        .await
    {
        // Expected and not an error: compliance publishes for driver-ops
        // drivers on this same topic, and most of them have no courier row.
        Ok(false) => tracing::debug!(
            entity_id = %event.data.entity_id,
            "compliance verdict for a user who is not a courier in this tenant",
        ),
        Ok(true) => tracing::info!(
            entity_id     = %event.data.entity_id,
            status        = %event.data.new_status,
            is_assignable = event.data.is_assignable,
            "courier compliance status updated",
        ),
        Err(e) => tracing::error!(
            entity_id = %event.data.entity_id,
            "failed to record courier compliance status: {e}",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_driver_status_change_concerns_this_tier() {
        assert!(concerns_a_courier("compliance.status_changed", "driver"));
    }

    /// The trap this rule exists to avoid. Compliance calls them drivers; this
    /// schema and OmniDeliv call them couriers. Matching on `"courier"` would
    /// compile, read correctly, pass review, and silently never fire.
    #[test]
    fn compliance_says_driver_even_for_a_courier() {
        assert!(
            !concerns_a_courier("compliance.status_changed", "courier"),
            "compliance never emits entity_type=courier; matching it would be a dead branch",
        );
    }

    #[test]
    fn customer_kyc_is_not_this_tiers_business() {
        assert!(!concerns_a_courier("compliance.status_changed", "customer"));
    }

    /// Three other event types share this topic. Acting on them would apply a
    /// document review's payload as though it were an assignability verdict.
    #[test]
    fn other_compliance_events_on_the_same_topic_are_ignored() {
        for other in [
            "compliance.document_reviewed",
            "compliance.expiry_warning",
            "compliance.driver_reinstated",
        ] {
            assert!(!concerns_a_courier(other, "driver"), "{other} must not be acted on");
        }
    }

    /// Pins the envelope this tier has to read. `tenant_id` lives on the
    /// envelope rather than inside `data` — reading it from the wrong level
    /// would scope every update to a nil tenant and match no rows, which looks
    /// identical to no events arriving at all.
    #[test]
    fn a_real_compliance_envelope_deserialises() {
        let body = r#"{
            "id": "11111111-1111-1111-1111-111111111111",
            "source": "logisticos/compliance",
            "event_type": "compliance.status_changed",
            "time": "2026-08-24T00:00:00Z",
            "tenant_id": "22222222-2222-2222-2222-222222222222",
            "data": {
                "entity_type": "driver",
                "entity_id": "33333333-3333-3333-3333-333333333333",
                "old_status": "under_review",
                "new_status": "rejected",
                "is_assignable": false
            }
        }"#;

        let e: ComplianceEvent = serde_json::from_str(body).expect("deserialise");
        assert_eq!(e.tenant_id.to_string(), "22222222-2222-2222-2222-222222222222");
        assert_eq!(e.data.new_status, "rejected");
        assert!(!e.data.is_assignable);
        assert!(concerns_a_courier(&e.event_type, &e.data.entity_type));
    }

    /// `is_assignable` is compliance's answer and is carried verbatim.
    /// `expired` is assignable there — a deliberate grace period — so a tier
    /// that re-derived "expired means blocked" would refuse work compliance
    /// permits, and would do it without anyone changing the compliance rule.
    #[test]
    fn an_expired_but_assignable_verdict_is_carried_through_unchanged() {
        let body = r#"{
            "id": "11111111-1111-1111-1111-111111111111",
            "source": "logisticos/compliance",
            "event_type": "compliance.status_changed",
            "time": "2026-08-24T00:00:00Z",
            "tenant_id": "22222222-2222-2222-2222-222222222222",
            "data": {
                "entity_type": "driver",
                "entity_id": "33333333-3333-3333-3333-333333333333",
                "old_status": "compliant",
                "new_status": "expired",
                "is_assignable": true
            }
        }"#;

        let e: ComplianceEvent = serde_json::from_str(body).expect("deserialise");
        assert_eq!(e.data.new_status, "expired");
        assert!(e.data.is_assignable, "compliance permits expired within grace");
    }
}
