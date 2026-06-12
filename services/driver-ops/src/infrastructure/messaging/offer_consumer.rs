//! Consumes gig offer events from dispatch → FCM fan-out to candidate drivers.
//!
//! TASK_OFFER_CREATED → per-candidate `task_offer` data push (each candidate
//! sees their OWN payout — rates are per-driver). TASK_OFFER_CLOSED →
//! `offer_closed` push to every candidate except the winner so losing grab
//! cards flip to "Taken" / dismiss in real time.
//!
//! Pure fan-out: no DB writes here. The dispatch DB row is the source of
//! truth and the app's GET /v1/offers/open covers any missed push.

use std::sync::Arc;
use logisticos_events::{envelope::Event, payloads::{TaskOfferClosed, TaskOfferCreated}, topics};
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    config::ClientConfig,
    Message,
};
use tokio::sync::watch;
use crate::infrastructure::external::FcmClient;

pub async fn start_offer_consumer(
    brokers: &str,
    group_id: &str,
    fcm: Option<Arc<FcmClient>>,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", &format!("{}-offers", group_id))
        .set("auto.offset.reset", "latest")  // offers are 30s-TTL ephemera — replaying history is useless
        .set("enable.auto.commit", "false")
        .create()?;

    consumer.subscribe(&[topics::TASK_OFFER_CREATED, topics::TASK_OFFER_CLOSED])?;

    tracing::info!(
        "offer consumer: subscribed to {} + {}",
        topics::TASK_OFFER_CREATED, topics::TASK_OFFER_CLOSED
    );

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    tracing::info!("offer consumer: shutdown signal received");
                    break;
                }
            }
            result = consumer.recv() => {
                match result {
                    Ok(msg) => {
                        if let Some(payload) = msg.payload() {
                            let topic = msg.topic();
                            let res = if topic == topics::TASK_OFFER_CREATED {
                                handle_offer_created(payload, fcm.clone()).await
                            } else {
                                handle_offer_closed(payload, fcm.clone()).await
                            };
                            if let Err(e) = res {
                                tracing::warn!(err = %e, topic, "offer consumer: handler error (skipping)");
                            }
                        }
                        consumer.commit_message(&msg, CommitMode::Async).ok();
                    }
                    Err(e) => {
                        tracing::error!(err = %e, "offer consumer: recv error");
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                }
            }
        }
    }
    Ok(())
}

async fn handle_offer_created(payload: &[u8], fcm: Option<Arc<FcmClient>>) -> anyhow::Result<()> {
    let event: Event<TaskOfferCreated> = serde_json::from_slice(payload)?;
    let o = event.data;
    let Some(fcm) = fcm else { return Ok(()) };

    tracing::info!(
        offer_id = %o.offer_id, wave = o.wave, candidates = o.candidates.len(),
        "offer consumer: fanning out task_offer pushes"
    );

    let opt = |v: Option<f64>| v.map(|f| f.to_string()).unwrap_or_default();
    for candidate in &o.candidates {
        // FCM data maps are string→string; absent values serialize as "".
        let data = serde_json::json!({
            "type":              "task_offer",
            "title":             "New gig offer",
            "body":              format!("{} — {}", o.merchant_name, o.delivery_address),
            "offer_id":          o.offer_id.to_string(),
            "shipment_id":       o.shipment_id.to_string(),
            "wave":              o.wave.to_string(),
            "expires_at_ms":     o.expires_at.timestamp_millis().to_string(),
            // THIS candidate's contractual payout — never another driver's.
            "payout_cents":      candidate.payout_cents.map(|c| c.to_string()).unwrap_or_default(),
            "merchant_name":     o.merchant_name,
            "delivery_category": o.delivery_category,
            "weight_grams":      o.weight_grams.to_string(),
            "tracking_number":   o.tracking_number,
            "customer_name":     o.customer_name,
            "pickup_address":    o.pickup_address,
            "address":           o.delivery_address,
            "cod_amount_cents":  o.cod_amount_cents.map(|c| c.to_string()).unwrap_or_default(),
            "pickup_lat":        opt(o.pickup_lat),
            "pickup_lng":        opt(o.pickup_lng),
            "delivery_lat":      opt(o.delivery_lat),
            "delivery_lng":      opt(o.delivery_lng),
        });
        let fcm = Arc::clone(&fcm);
        let driver_id = candidate.driver_id;
        tokio::spawn(async move {
            fcm.notify_data(driver_id, &data).await;
        });
    }
    Ok(())
}

async fn handle_offer_closed(payload: &[u8], fcm: Option<Arc<FcmClient>>) -> anyhow::Result<()> {
    let event: Event<TaskOfferClosed> = serde_json::from_slice(payload)?;
    let o = event.data;
    let Some(fcm) = fcm else { return Ok(()) };

    for driver_id in o.candidate_driver_ids {
        // The winner gets task_assigned through the normal flow — only the
        // losers need their card flipped.
        if Some(driver_id) == o.claimed_by {
            continue;
        }
        let data = serde_json::json!({
            "type":     "offer_closed",
            "offer_id": o.offer_id.to_string(),
            "reason":   o.reason,
        });
        let fcm = Arc::clone(&fcm);
        tokio::spawn(async move {
            fcm.notify_data(driver_id, &data).await;
        });
    }
    Ok(())
}
