//! Consumes DELIVERY_FAILED events → resets the dispatch_queue row to 'pending'
//! so the shipment re-enters the queue for driver reassignment.
//!
//! This makes the re-queue path resilient to the business-logic ECA engine being
//! unavailable. Both paths (direct consumer here + ECA RescheduleDelivery action)
//! call reset_to_pending, which is idempotent — double-reset is a no-op because
//! the second UPDATE matches no row (already 'pending').
//!
//! Shipments that have exceeded MAX_AUTO_REQUEUE_ATTEMPTS are left in whatever
//! terminal state they are in and logged for manual ops intervention.

use logisticos_events::{envelope::Event, payloads::DeliveryFailed, topics};
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    config::ClientConfig,
    message::Message,
};
use std::sync::Arc;
use tokio::sync::watch;

use crate::infrastructure::db::dispatch_queue_repo::{DispatchQueueRepository, PgDispatchQueueRepository};
use sqlx::PgPool;

/// Shipments that have failed delivery this many times are parked for manual ops
/// review rather than being re-queued automatically. Configurable via
/// DELIVERY_FAILED_MAX_REQUEUE env var; default 3.
const DEFAULT_MAX_REQUEUE_ATTEMPTS: u32 = 3;

pub async fn start_delivery_failed_consumer(
    brokers: &str,
    group_id: &str,
    pool: PgPool,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let max_attempts: u32 = std::env::var("DELIVERY_FAILED_MAX_REQUEUE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_MAX_REQUEUE_ATTEMPTS);

    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", &format!("{}-delivery-failed", group_id))
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()?;

    consumer.subscribe(&[topics::DELIVERY_FAILED])?;
    let repo = Arc::new(PgDispatchQueueRepository::new(pool));

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow_and_update() {
                    tracing::info!("Delivery-failed consumer shutting down");
                    break;
                }
            }
            result = consumer.recv() => {
                match result {
                    Ok(msg) => {
                        if let Some(payload) = msg.payload() {
                            if let Err(e) = handle_delivery_failed(payload, &*repo, max_attempts).await {
                                tracing::warn!(err = %e, "delivery-failed consumer: handler error (skipping)");
                            }
                        }
                        consumer.commit_message(&msg, CommitMode::Async).ok();
                    }
                    Err(e) => {
                        tracing::error!(err = %e, "delivery-failed consumer: recv error");
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                }
            }
        }
    }

    Ok(())
}

async fn handle_delivery_failed(
    payload: &[u8],
    repo: &dyn DispatchQueueRepository,
    max_attempts: u32,
) -> anyhow::Result<()> {
    let event: Event<DeliveryFailed> = serde_json::from_slice(payload)?;
    let d = event.data;

    if d.attempt_number > max_attempts {
        tracing::warn!(
            shipment_id   = %d.shipment_id,
            attempt_number = d.attempt_number,
            max_attempts   = max_attempts,
            "Delivery failed beyond max requeue threshold — shipment requires manual ops intervention"
        );
        return Ok(());
    }

    // reset_to_pending is idempotent: the business-logic ECA engine may also call
    // the internal HTTP requeue endpoint for the same event. Second update is a no-op.
    repo.reset_to_pending(d.shipment_id).await?;

    tracing::info!(
        shipment_id    = %d.shipment_id,
        driver_id      = %d.driver_id,
        attempt_number = d.attempt_number,
        reason         = %d.reason,
        "Shipment requeued for dispatch retry after delivery failure"
    );

    Ok(())
}
