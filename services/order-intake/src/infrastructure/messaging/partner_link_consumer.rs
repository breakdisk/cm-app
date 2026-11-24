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

/// Apply one message. Returns an error for a malformed payload; the caller logs
/// and skips it. An unrelated topic is not an error.
pub async fn handle_partner_link(pool: &PgPool, topic: &str, payload: &[u8]) -> anyhow::Result<()> {
    let (tenant_id, shipment_id, carrier_id, source) = match topic {
        t if t == topics::CARRIER_ALLOCATED => {
            let d = serde_json::from_slice::<Event<CarrierAllocatedView>>(payload)?.data;
            (d.tenant_id, d.shipment_id, Some(d.carrier_id), "carrier_allocated")
        }
        t if t == topics::TASK_ASSIGNED => {
            let d = serde_json::from_slice::<Event<TaskAssignedView>>(payload)?.data;
            (d.tenant_id, d.shipment_id, d.driver_carrier_id, "task_assigned")
        }
        _ => return Ok(()),
    };

    let Some(carrier_id) = carrier_id else { return Ok(()) };

    sqlx::query(
        "INSERT INTO order_intake.shipment_partner_links (tenant_id, shipment_id, carrier_id, source) \
         VALUES ($1, $2, $3, $4) \
         ON CONFLICT (shipment_id, carrier_id) DO NOTHING",
    )
    .bind(tenant_id)
    .bind(shipment_id)
    .bind(carrier_id)
    .bind(source)
    .execute(pool)
    .await?;
    Ok(())
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
                            tracing::error!(
                                topic = msg.topic(), offset = msg.offset(), err = %e,
                                "partner link consumer: giving up after 4 attempts; run the partner-shipment-links backfill"
                            );
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
