//! Consumes `shipment.cancelled` → gives a cancelled booking's code back.
//!
//! A code is spent when a booking is created, so an abandoned checkout, a
//! payment that fails, or a customer cancelling would otherwise cost them the
//! month's one code for a move that never happened. The ledger row is kept —
//! it is the record that the code was used and returned — and stops counting.

use std::sync::Arc;

use rdkafka::{
    config::ClientConfig,
    consumer::{CommitMode, Consumer, StreamConsumer},
    message::Message,
};
use tokio::sync::watch;
use uuid::Uuid;

use crate::application::Promotions;

#[derive(Debug, serde::Deserialize)]
struct Cancelled {
    shipment_id: Uuid,
}

#[derive(Debug, serde::Deserialize)]
struct Envelope {
    data: Cancelled,
}

pub async fn start(
    brokers: &str,
    promotions: Arc<Promotions>,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", "promotions-shipment-cancelled")
        // A cancellation missed is a code the customer never gets back.
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()?;
    consumer.subscribe(&[logisticos_events::topics::SHIPMENT_CANCELLED])?;

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow_and_update() {
                    break;
                }
            }
            result = consumer.recv() => match result {
                Ok(msg) => {
                    if let Some(payload) = msg.payload() {
                        match serde_json::from_slice::<Envelope>(payload) {
                            Ok(env) => {
                                if let Err(e) = promotions.release(env.data.shipment_id, chrono::Utc::now()).await {
                                    // Not committed: redelivered. A release is idempotent.
                                    tracing::error!(shipment_id = %env.data.shipment_id, err = %e, "release failed");
                                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                                    continue;
                                }
                            }
                            Err(e) => tracing::warn!(err = %e, "shipment.cancelled payload not understood — skipped"),
                        }
                    }
                    consumer.commit_message(&msg, CommitMode::Async).ok();
                }
                Err(e) => {
                    tracing::error!(err = %e, "shipment.cancelled recv error");
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            }
        }
    }
    Ok(())
}
