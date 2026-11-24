//! Projects which partner handles each shipment, so a partner token can be
//! scoped to its own work.
//!
//!   logisticos.carrier.allocated  → link (shipment, allocated carrier)
//!   logisticos.task.assigned      → link (shipment, the driver's carrier),
//!                                   only when the driver belongs to one
//!
//! A platform driver's task carries no carrier and links nothing. Links are
//! additive and idempotent: replays, and the pickup and delivery legs of one
//! shipment, collapse onto one row.
//!
//! Reads a minimal local view of each payload rather than the full event type,
//! like `status_consumer`, so it does not break when the producer's payload
//! grows.

use rdkafka::{
    config::ClientConfig,
    consumer::{CommitMode, Consumer, StreamConsumer},
    Message,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use logisticos_events::{topics, Event};

// `Event<T>` requires `T: Serialize`; these views are only ever deserialized.
#[derive(Serialize, Deserialize)]
struct CarrierAllocatedView {
    carrier_id:  Uuid,
    tenant_id:   Uuid,
    shipment_id: Uuid,
}

#[derive(Serialize, Deserialize)]
struct TaskAssignedView {
    shipment_id: Uuid,
    tenant_id:   Uuid,
    #[serde(default)]
    driver_carrier_id: Option<Uuid>,
}

/// What one message says: which shipment a carrier handles.
#[derive(Debug, PartialEq, Eq)]
struct Link {
    tenant_id:   Uuid,
    shipment_id: Uuid,
    /// `None` for a platform driver's task: it links nothing.
    carrier_id:  Option<Uuid>,
    source:      &'static str,
}

/// Read a message. `Ok(None)` for an unrelated topic; an error for a malformed
/// payload.
fn parse_link(topic: &str, payload: &[u8]) -> anyhow::Result<Option<Link>> {
    Ok(match topic {
        t if t == topics::CARRIER_ALLOCATED => {
            let d = serde_json::from_slice::<Event<CarrierAllocatedView>>(payload)?.data;
            Some(Link { tenant_id: d.tenant_id, shipment_id: d.shipment_id, carrier_id: Some(d.carrier_id), source: "carrier_allocated" })
        }
        t if t == topics::TASK_ASSIGNED => {
            let d = serde_json::from_slice::<Event<TaskAssignedView>>(payload)?.data;
            Some(Link { tenant_id: d.tenant_id, shipment_id: d.shipment_id, carrier_id: d.driver_carrier_id, source: "task_assigned" })
        }
        _ => None,
    })
}

/// Apply one message. Returns an error for a malformed payload; the caller logs
/// and skips it. An unrelated topic is not an error.
pub async fn handle_partner_link(pool: &PgPool, topic: &str, payload: &[u8]) -> anyhow::Result<()> {
    let Some(Link { tenant_id, shipment_id, carrier_id, source }) = parse_link(topic, payload)? else {
        return Ok(());
    };
    let Some(carrier_id) = carrier_id else { return Ok(()) };

    sqlx::query(
        "INSERT INTO order_intake.shipment_partner_links (tenant_id, shipment_id, carrier_id, source)          VALUES ($1, $2, $3, $4)          ON CONFLICT (shipment_id, carrier_id) DO NOTHING",
    )
    .bind(tenant_id)
    .bind(shipment_id)
    .bind(carrier_id)
    .bind(source)
    .execute(pool)
    .await?;
    Ok(())
}

/// Log that the consumer gave up on a message, with what an operator needs to
/// repair it: the shipment is invisible to its partner until the links
/// backfill is re-run. Best effort: a malformed payload has no identifiers.
fn log_gave_up(topic: &str, offset: i64, payload: &[u8], attempts: u32, err: &anyhow::Error) {
    let link = parse_link(topic, payload).ok().flatten();
    tracing::error!(
        topic,
        offset,
        attempts,
        tenant_id   = ?link.as_ref().map(|l| l.tenant_id),
        shipment_id = ?link.as_ref().map(|l| l.shipment_id),
        carrier_id  = ?link.as_ref().and_then(|l| l.carrier_id),
        err = %err,
        "partner link consumer: giving up; this shipment stays hidden from its partner.          Remedy: re-run scripts/backfill/partner-shipment-links.sh"
    );
}

pub async fn start_partner_link_consumer(
    brokers: &str,
    group_id: &str,
    pool: PgPool,
) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", format!("{}-partner-links", group_id))
        // A missed link hides a shipment from the partner that handles it.
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()?;

    consumer.subscribe(&[topics::CARRIER_ALLOCATED, topics::TASK_ASSIGNED])?;

    loop {
        match consumer.recv().await {
            Ok(msg) => {
                if let Some(payload) = msg.payload() {
                    // Retried in place: an uncommitted message is not redelivered
                    // until a restart. The handler is replay-safe.
                    let mut attempt = 0u32;
                    while let Err(e) = handle_partner_link(&pool, msg.topic(), payload).await {
                        attempt += 1;
                        if attempt >= 4 {
                            log_gave_up(msg.topic(), msg.offset(), payload, attempt, &e);
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(2u64.pow(attempt))).await;
                    }
                }
                consumer.commit_message(&msg, CommitMode::Async).ok();
            }
            Err(e) => {
                tracing::error!(err = %e, "partner link consumer: recv error");
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(topic_data: serde_json::Value, ty: &str, tenant: Uuid) -> Vec<u8> {
        serde_json::to_vec(&Event::new("test", ty, tenant, topic_data)).expect("payload")
    }

    #[test]
    fn give_up_context_names_tenant_shipment_and_carrier() {
        let (t, s, c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let p = payload(
            serde_json::json!({ "carrier_id": c, "tenant_id": t, "shipment_id": s }),
            "carrier.allocated",
            t,
        );
        let link = parse_link(topics::CARRIER_ALLOCATED, &p).expect("parses").expect("a link");
        assert_eq!(link, Link { tenant_id: t, shipment_id: s, carrier_id: Some(c), source: "carrier_allocated" });
    }

    #[test]
    fn malformed_payload_is_an_error_and_unrelated_topic_is_not() {
        assert!(parse_link(topics::CARRIER_ALLOCATED, b"not json").is_err());
        assert!(parse_link("some.other.topic", b"not json").expect("ignored").is_none());
        // The give-up logger must not panic on either.
        log_gave_up(topics::CARRIER_ALLOCATED, 7, b"not json", 4, &anyhow::anyhow!("boom"));
    }
}
