//! Consumes `payment.intent.authorized` / `payment.intent.failed` for
//! marketplace bookings.
//!
//! `purpose == "marketplace_booking"` only. Three products share these two
//! topics — order-intake's `shipping_fee`, omnideliv's `omnideliv_order` and
//! this one — and each consumer is a silent no-op on the other two.
//!
//! This is the deferred half of `MarketplaceService::create_booking`'s `Online`
//! branch. That branch persists the booking and opens a hold, and shows the
//! carrier nothing: an unfunded booking would have a carrier hold a truck for a
//! job that may never be paid for.
//!
//! * On **authorized**: mark the hold real. The booking becomes visible to the
//!   carrier through `MarketplaceBooking::is_offered_to_carrier`, and its
//!   response window starts running from this moment rather than from when the
//!   merchant first hit Book.
//! * On **failed** (declined, or `services/payments`' own sweep expiring an
//!   uncompleted checkout session — both publish this identical event): cancel
//!   the booking. No carrier was ever shown it, so there is nothing to undo on
//!   that side, and the merchant was never charged.
//!
//! Both arms are idempotent against Kafka's at-least-once redelivery.

use std::sync::Arc;

use anyhow::Context;
use logisticos_events::{
    envelope::Event,
    payloads::{PaymentIntentAuthorized, PaymentIntentFailed},
    topics,
};
use rdkafka::{
    config::ClientConfig,
    consumer::{CommitMode, Consumer, StreamConsumer},
    message::Message,
};
use tokio::sync::watch;
use uuid::Uuid;

use crate::domain::repositories::MarketplaceRepository;

/// The tag `CarrierPaymentsClient::authorize` stamps on every intent it opens,
/// and the only kind this consumer acts on.
pub const MARKETPLACE_BOOKING_PURPOSE: &str = "marketplace_booking";

pub async fn start_booking_payment_consumer(
    brokers: &str,
    group_id: &str,
    repo: Arc<dyn MarketplaceRepository>,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", format!("{group_id}-booking-payments"))
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()?;

    consumer.subscribe(&[topics::PAYMENT_INTENT_AUTHORIZED, topics::PAYMENT_INTENT_FAILED])?;

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow_and_update() {
                    tracing::info!("Booking-payment consumer shutting down");
                    break;
                }
            }
            result = consumer.recv() => {
                match result {
                    Ok(msg) => {
                        let topic = msg.topic().to_string();
                        if let Some(payload) = msg.payload() {
                            match serde_json::from_slice::<serde_json::Value>(payload) {
                                Ok(json) => {
                                    if let Err(e) = handle(&topic, json, &repo).await {
                                        // Withholding the commit would block the
                                        // partition on a poison message, and both
                                        // arms are idempotent, so a retry costs
                                        // nothing while a block costs every later
                                        // booking. Logged loudly instead.
                                        tracing::error!(err = %e, %topic,
                                            "booking-payment consumer: handler error (skipping)");
                                    }
                                }
                                Err(e) => tracing::warn!(err = %e, %topic, "unparseable payment event"),
                            }
                        }
                        consumer.commit_message(&msg, CommitMode::Async).ok();
                    }
                    Err(e) => tracing::warn!(err = %e, "booking-payment consumer: recv error"),
                }
            }
        }
    }
    Ok(())
}

/// Dispatches on topic and filters to this service's purpose before acting.
pub async fn handle(
    topic: &str,
    json: serde_json::Value,
    repo: &Arc<dyn MarketplaceRepository>,
) -> anyhow::Result<()> {
    match topic {
        topics::PAYMENT_INTENT_AUTHORIZED => {
            let evt: Event<PaymentIntentAuthorized> = serde_json::from_value(json)
                .context("failed to deserialize payment.intent.authorized event")?;
            if evt.data.purpose != MARKETPLACE_BOOKING_PURPOSE {
                return Ok(());
            }
            handle_authorized(evt.data.reference_id, evt.data.intent_id, repo).await
        }
        topics::PAYMENT_INTENT_FAILED => {
            let evt: Event<PaymentIntentFailed> = serde_json::from_value(json)
                .context("failed to deserialize payment.intent.failed event")?;
            if evt.data.purpose != MARKETPLACE_BOOKING_PURPOSE {
                return Ok(());
            }
            handle_failed(evt.data.reference_id, &evt.data.reason, repo).await
        }
        _ => Ok(()),
    }
}

/// Funds are ring-fenced — the carrier may now see the booking.
async fn handle_authorized(
    booking_id: Uuid,
    intent_id: Uuid,
    repo: &Arc<dyn MarketplaceRepository>,
) -> anyhow::Result<()> {
    let Some(mut booking) = repo.find_booking_by_id(booking_id).await? else {
        // Not an error: an old partition replay, or a purged row. Logged so a
        // systematic mismatch is visible rather than silently dropped.
        tracing::warn!(%booking_id, "payment.intent.authorized for an unknown booking");
        return Ok(());
    };

    // Idempotent, and specifically does not re-stamp `payment_authorized_at`
    // on a redelivery — that timestamp is the response-window clock, and
    // restamping it would push the carrier's deadline out every time Kafka
    // replayed the message.
    booking.payment_authorized(intent_id).map_err(|e| anyhow::anyhow!("{e}"))?;
    repo.save_booking(&booking).await?;

    tracing::info!(%booking_id, %intent_id, carrier_id = %booking.carrier_id,
        "booking funded — now offered to the carrier");
    Ok(())
}

/// The hold never landed, or expired unused — cancel the booking. Nothing to
/// void: there is no hold, which is exactly what "failed" means here.
async fn handle_failed(
    booking_id: Uuid,
    reason: &str,
    repo: &Arc<dyn MarketplaceRepository>,
) -> anyhow::Result<()> {
    let Some(mut booking) = repo.find_booking_by_id(booking_id).await? else {
        tracing::warn!(%booking_id, "payment.intent.failed for an unknown booking");
        return Ok(());
    };

    // A booking a carrier already answered is not this event's to cancel — a
    // late or duplicate failure for an intent that was captured in the meantime
    // must not reverse an accepted job.
    if booking.payment_status.is_terminal() {
        return Ok(());
    }

    booking.payment_failed().map_err(|e| anyhow::anyhow!("{e}"))?;
    // Infallible from Pending, and a booking whose payment never landed cannot
    // have left Pending — the carrier was never shown it.
    let _ = booking.expire();
    repo.save_booking(&booking).await?;

    tracing::warn!(%booking_id, %reason, "booking payment failed — booking cancelled");
    Ok(())
}
