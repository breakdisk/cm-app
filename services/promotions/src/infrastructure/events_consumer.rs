//! The three events promotions follows.
//!
//! - `shipment.created`: a move was booked (the loyalty count's denominator,
//!   and what makes an account no longer new for a referral).
//! - `delivery.completed`: a move was done. It counts toward a tier, and an
//!   invitee's first one pays their referrer.
//! - `shipment.cancelled`: the booking's code and credit go back.
//!
//! All three are idempotent, so a redelivery changes nothing. A failed handler
//! is retried in place with backoff, then logged and skipped.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use rdkafka::{
    config::ClientConfig,
    consumer::{CommitMode, Consumer, StreamConsumer},
    message::Message,
};
use serde_json::Value;
use tokio::sync::watch;
use uuid::Uuid;

use logisticos_events::topics;

use crate::application::Promotions;

const MAX_ATTEMPTS: u32 = 6;

fn uuid_at(v: &Value, key: &str) -> Option<Uuid> {
    v.get(key)?.as_str()?.parse().ok()
}

fn time_at(v: &Value, key: &str) -> Option<DateTime<Utc>> {
    v.get(key)?.as_str()?.parse().ok()
}

async fn handle(promotions: &Promotions, topic: &str, payload: &[u8]) -> anyhow::Result<()> {
    let envelope: Value = serde_json::from_slice(payload)?;
    let data = envelope.get("data").unwrap_or(&envelope);
    let at = time_at(&envelope, "time").unwrap_or_else(Utc::now);

    match topic {
        topics::SHIPMENT_CREATED => {
            let (Some(tenant), Some(account), Some(shipment)) =
                (uuid_at(&envelope, "tenant_id"), uuid_at(data, "merchant_id"), uuid_at(data, "shipment_id"))
            else {
                tracing::warn!("shipment.created without tenant, merchant or shipment — skipped");
                return Ok(());
            };
            promotions.record_booked(tenant, account, shipment, at).await?;
        }
        topics::DELIVERY_COMPLETED => {
            let Some(shipment) = uuid_at(data, "shipment_id") else {
                return Ok(());
            };
            let done_at = time_at(data, "completed_at").unwrap_or(at);
            promotions.record_completed(shipment, done_at, Utc::now()).await?;
        }
        topics::SHIPMENT_CANCELLED => {
            let Some(shipment) = uuid_at(data, "shipment_id") else {
                return Ok(());
            };
            promotions.release(shipment, Utc::now()).await?;
        }
        _ => {}
    }
    Ok(())
}

pub async fn start(
    brokers: &str,
    promotions: Arc<Promotions>,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", "promotions-events")
        // From the start of retention: bookings and completions already on the
        // topic seed the loyalty count, and a missed cancellation is a code the
        // customer never gets back.
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()?;
    consumer.subscribe(&[topics::SHIPMENT_CREATED, topics::DELIVERY_COMPLETED, topics::SHIPMENT_CANCELLED])?;

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
                        // Retried here, in place: leaving a message uncommitted
                        // does not bring it back until the next restart — the
                        // consumer's position has already moved past it.
                        let mut attempt = 0u32;
                        while let Err(e) = handle(&promotions, msg.topic(), payload).await {
                            attempt += 1;
                            if attempt >= MAX_ATTEMPTS {
                                tracing::error!(topic = msg.topic(), offset = msg.offset(), err = %e,
                                    "promotions event failed {MAX_ATTEMPTS} times — skipped");
                                break;
                            }
                            tracing::warn!(topic = msg.topic(), attempt, err = %e, "promotions event failed — retrying");
                            tokio::time::sleep(std::time::Duration::from_secs(2u64.pow(attempt.min(5)))).await;
                        }
                    }
                    consumer.commit_message(&msg, CommitMode::Async).ok();
                }
                Err(e) => {
                    tracing::error!(err = %e, "promotions events recv error");
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            }
        }
    }
    Ok(())
}
