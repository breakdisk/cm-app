//! Consumes SHIPMENT_CANCELLED (order-intake) → takes the shipment out of
//! dispatch: the queue row, its open offers, a whole-home move's reservation,
//! and any route and assignment carrying it.
//!
//! Before this, dispatch never heard of a cancellation: a cancelled shipment
//! could still be swept to a driver or grabbed from an offer, and a reserved
//! home move was activated for its lead regardless.

use logisticos_events::{envelope::Event, topics};
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    config::ClientConfig,
    message::Message,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::watch;
use uuid::Uuid;

use crate::application::services::OfferService;
use crate::infrastructure::db::dispatch_queue_repo::PgDispatchQueueRepository;

/// The part of order-intake's `shipment.cancelled` dispatch reads. The
/// tenant comes from the envelope.
#[derive(Debug, Serialize, Deserialize)]
struct ShipmentCancelledData {
    shipment_id: Uuid,
}

pub async fn start_shipment_cancelled_consumer(
    brokers: &str,
    group_id: &str,
    pool: PgPool,
    offer_service: Arc<OfferService>,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", format!("{}-shipment-cancelled", group_id))
        // A missed cancellation sends a driver to a job that isn't there.
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()?;

    consumer.subscribe(&[topics::SHIPMENT_CANCELLED])?;
    let repo = PgDispatchQueueRepository::new(pool);

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow_and_update() {
                    tracing::info!("Shipment-cancelled consumer shutting down");
                    break;
                }
            }
            result = consumer.recv() => {
                match result {
                    Ok(msg) => {
                        if let Some(payload) = msg.payload() {
                            // Retried in place: the position has already moved
                            // past an uncommitted message. The handler is
                            // replay-safe.
                            let mut attempt = 0u32;
                            while let Err(e) = handle_shipment_cancelled(payload, &repo, &offer_service).await {
                                attempt += 1;
                                if attempt >= 6 {
                                    tracing::error!(offset = msg.offset(), err = %e,
                                        "shipment-cancelled consumer: failed 6 times — cancel the dispatch from the ops console");
                                    break;
                                }
                                tracing::warn!(attempt, err = %e, "shipment-cancelled consumer: handler error — retrying");
                                tokio::time::sleep(std::time::Duration::from_secs(2u64.pow(attempt.min(5)))).await;
                            }
                        }
                        consumer.commit_message(&msg, CommitMode::Async).ok();
                    }
                    Err(e) => {
                        tracing::error!(err = %e, "shipment-cancelled consumer: recv error");
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                }
            }
        }
    }

    Ok(())
}

async fn handle_shipment_cancelled(
    payload: &[u8],
    repo: &PgDispatchQueueRepository,
    offer_service: &OfferService,
) -> anyhow::Result<()> {
    let event: Event<ShipmentCancelledData> = serde_json::from_slice(payload)?;
    let (tenant_id, shipment_id) = (event.tenant_id, event.data.shipment_id);
    if tenant_id.is_nil() {
        // Events from before order-intake stamped the tenant; nothing here
        // can be scoped to them.
        tracing::warn!(shipment_id = %shipment_id, "shipment.cancelled without a tenant — skipped");
        return Ok(());
    }

    let done = repo.cancel_for_shipment(tenant_id, shipment_id).await?;
    offer_service.announce_cancelled(tenant_id, shipment_id, &done.offers).await;
    tracing::info!(
        shipment_id = %shipment_id,
        offers_closed = done.offers.len(),
        drivers_released = done.released_drivers.len(),
        reservation_released = done.reservation_released,
        "Cancelled shipment taken out of dispatch"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_order_intakes_cancellation() {
        // As order-intake publishes it: extra fields ride along unread.
        let json = r#"{
            "id":"00000000-0000-0000-0000-000000000099",
            "source":"logisticos/order-intake",
            "event_type":"shipment.cancelled",
            "tenant_id":"00000000-0000-0000-0000-000000000001",
            "time":"2026-09-19T08:00:00Z",
            "data":{"shipment_id":"00000000-0000-0000-0000-0000000000aa","reason":"changed my mind",
                    "retention_bps":2000,"cancellation_tier":"within_24h","survey_retained_cents":4500}
        }"#;
        let event: Event<ShipmentCancelledData> = serde_json::from_str(json).expect("parses");
        assert_eq!(event.data.shipment_id, Uuid::parse_str("00000000-0000-0000-0000-0000000000aa").unwrap());
        assert!(!event.tenant_id.is_nil());
    }
}
