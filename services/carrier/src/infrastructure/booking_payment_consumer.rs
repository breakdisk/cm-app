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
//! Both arms are idempotent against Kafka's at-least-once redelivery, but the
//! consumer loop commits every message whether or not the handler succeeded,
//! so a handler error is NOT retried by redelivery. Anything that must not be
//! lost (the late-hold void) retries inside the handler instead.

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

use crate::application::services::BookingPayments;
use crate::domain::entities::BookingStatus;
use crate::domain::repositories::MarketplaceRepository;

/// The tag `CarrierPaymentsClient::authorize` stamps on every intent it opens,
/// and the only kind this consumer acts on.
pub const MARKETPLACE_BOOKING_PURPOSE: &str = "marketplace_booking";

pub async fn start_booking_payment_consumer(
    brokers: &str,
    group_id: &str,
    repo: Arc<dyn MarketplaceRepository>,
    payments: Option<Arc<dyn BookingPayments>>,
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
                                    if let Err(e) = handle(&topic, json, &repo, payments.as_ref()).await {
                                        // The commit below is unconditional:
                                        // withholding it would block the
                                        // partition on a poison message and
                                        // every later booking with it. So an
                                        // error here is NOT redelivered -- it is
                                        // logged and dropped. Work that must not
                                        // be lost retries inside the handler.
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
    payments: Option<&Arc<dyn BookingPayments>>,
) -> anyhow::Result<()> {
    match topic {
        topics::PAYMENT_INTENT_AUTHORIZED => {
            let evt: Event<PaymentIntentAuthorized> = serde_json::from_value(json)
                .context("failed to deserialize payment.intent.authorized event")?;
            if evt.data.purpose != MARKETPLACE_BOOKING_PURPOSE {
                return Ok(());
            }
            handle_authorized(
                evt.data.reference_id, evt.data.intent_id, repo, payments, &LATE_HOLD_VOID_BACKOFF,
            ).await
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

/// Pauses between attempts to void a hold that landed on a booking that is no
/// longer pending: 3 attempts in all (immediately, after 200 ms, after 1 s).
pub(crate) const LATE_HOLD_VOID_BACKOFF: [std::time::Duration; 2] = [
    std::time::Duration::from_millis(200),
    std::time::Duration::from_secs(1),
];

/// Voids a late hold, retrying across `backoff` (attempts = `backoff.len() + 1`).
///
/// The consumer loop never redelivers a failed message, so this is the only
/// retry the hold gets. If every attempt fails the hold stays at the provider
/// against a cancelled booking and must be released by hand; that is logged
/// at `error` with the ids needed to do so.
async fn void_late_hold(
    payments: &Arc<dyn BookingPayments>,
    booking_id: Uuid,
    tenant_id: Uuid,
    intent_id: Uuid,
    backoff: &[std::time::Duration],
) -> anyhow::Result<()> {
    let mut attempt = 0usize;
    loop {
        match payments.void(intent_id).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                let Some(pause) = backoff.get(attempt) else {
                    tracing::error!(%booking_id, %tenant_id, %intent_id, attempts = attempt + 1, err = %e,
                        "late hold void failed after retries — MANUAL RELEASE REQUIRED");
                    return Err(e);
                };
                tracing::warn!(%booking_id, %intent_id, attempt = attempt + 1, err = %e,
                    "late hold void failed; retrying");
                tokio::time::sleep(*pause).await;
                attempt += 1;
            }
        }
    }
}

/// Funds are ring-fenced — the carrier may now see the booking.
pub(crate) async fn handle_authorized(
    booking_id: Uuid,
    intent_id: Uuid,
    repo: &Arc<dyn MarketplaceRepository>,
    payments: Option<&Arc<dyn BookingPayments>>,
    void_backoff: &[std::time::Duration],
) -> anyhow::Result<()> {
    // Compare-and-set with a short retry: a merchant cancel or a carrier
    // answer can land between our read and our write, and a blind save would
    // overwrite their status with the stale one we read.
    //
    // `voided_tenant` is set once the provider void has succeeded. The void
    // happens at most once per invocation: a later pass only re-reads and
    // retries the write, because the provider would reject a second void (the
    // intent is no longer Authorized) and we would then raise a false
    // manual-release alarm for a hold that is already released.
    let mut voided_tenant: Option<Uuid> = None;
    for _ in 0..3 {
        let Some(mut booking) = repo.find_booking_by_id(booking_id).await? else {
            // Not an error: an old partition replay, or a purged row. Logged so a
            // systematic mismatch is visible rather than silently dropped.
            tracing::warn!(%booking_id, "payment.intent.authorized for an unknown booking");
            return Ok(());
        };
        let expected_status  = booking.status.clone();
        let expected_payment = booking.payment_status;

        if booking.status != BookingStatus::Pending {
            // The merchant cancelled (or the carrier answered) while checkout
            // was still open, and the hold landed anyway. Marking it authorized
            // would ring-fence money against a booking nothing will ever
            // release, so void it instead. A booking whose payment is already
            // settled is a redelivery: nothing to do.
            if expected_payment != crate::domain::entities::BookingPaymentStatus::Pending {
                return Ok(());
            }
            let Some(payments) = payments else {
                anyhow::bail!(
                    "booking {booking_id} is {} but a hold {intent_id} landed and payments is not configured to void it",
                    booking.status.as_str()
                );
            };
            if voided_tenant.is_none() {
                void_late_hold(payments, booking_id, booking.tenant_id, intent_id, void_backoff).await?;
                voided_tenant = Some(booking.tenant_id);
            }
            booking.late_hold_voided(intent_id).map_err(|e| anyhow::anyhow!("{e}"))?;
            if repo.transition_booking(&booking, &expected_status, expected_payment).await? {
                tracing::warn!(%booking_id, %intent_id, status = booking.status.as_str(),
                    "hold landed on a booking that is no longer pending — voided");
                return Ok(());
            }
            continue;
        }

        // Idempotent, and specifically does not re-stamp `payment_authorized_at`
        // on a redelivery — that timestamp is the response-window clock, and
        // restamping it would push the carrier's deadline out every time Kafka
        // replayed the message.
        booking.payment_authorized(intent_id).map_err(|e| anyhow::anyhow!("{e}"))?;
        if repo.transition_booking(&booking, &expected_status, expected_payment).await? {
            tracing::info!(%booking_id, %intent_id, carrier_id = %booking.carrier_id,
                "booking funded — now offered to the carrier");
            return Ok(());
        }
    }
    if let Some(tenant_id) = voided_tenant {
        // The money is released; only our record of it is missing. Distinct
        // from the manual-release alarm, which means the hold is still held.
        tracing::error!(%booking_id, %tenant_id, %intent_id,
            "late hold voided at provider but booking not recorded — RECONCILE BOOKING");
        anyhow::bail!(
            "late hold {intent_id} was voided but booking {booking_id} could not be updated after 3 tries; reconcile the booking"
        );
    }
    anyhow::bail!("booking {booking_id} kept changing underneath payment.intent.authorized; giving up after 3 tries")
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
