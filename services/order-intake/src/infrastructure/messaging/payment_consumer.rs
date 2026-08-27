//! Kafka consumer for `payment.intent.captured` / `payment.intent.failed`
//! (`purpose == "shipping_fee"` only — other purposes belong to other future
//! consumers on other services; `PaymentsClient::create_shipping_fee_intent`
//! is the only place in the platform that mints a `shipping_fee` intent, and
//! it always sets `reference_type = "shipment"` / `reference_id = shipment_id`).
//!
//! This is the consumer that closes the loop Task 18 opened: `create()` holds
//! a shipment's `AwbIssued`/`ShipmentCreated`/`ShipmentConfirmed` payloads in
//! `pending_dispatch_events` instead of publishing them, and opens a payment
//! intent. Without this consumer, an online-paid shipment sits in
//! `awaiting_payment` forever and never reaches dispatch.
//!
//! * On **captured**: republish the shipment's stored dispatch events
//!   unchanged and mark it `Paid`.
//! * On **failed** (declined webhook, or the payments-service sweep's expiry
//!   — both publish this same event, see `PaymentIntentFailed`'s doc comment
//!   in `libs/events`): cancel the shipment via the existing
//!   `ShipmentService::cancel()`, same as a merchant-initiated cancellation.
//!
//! Both handlers are idempotent against Kafka's at-least-once redelivery:
//! captured no-ops once `payment_status` is no longer `AwaitingPayment`;
//! failed no-ops once the shipment is no longer in a cancellable status.

use std::sync::Arc;

use anyhow::Context;
use logisticos_events::{
    consumer::KafkaConsumer,
    envelope::Event,
    payloads::{PaymentIntentCaptured, PaymentIntentFailed},
    topics,
};
use logisticos_types::ShipmentId;
use uuid::Uuid;

use crate::application::commands::CancelShipmentCommand;
use crate::application::services::shipment_service::ShipmentService;
use crate::domain::entities::shipment::PaymentRequirement;

/// Purpose tag `PaymentsClient::create_shipping_fee_intent` stamps on every
/// intent it opens — the only kind of payment intent order-intake ever
/// creates, and therefore the only kind this consumer acts on.
const SHIPPING_FEE_PURPOSE: &str = "shipping_fee";

pub struct PaymentConsumer {
    inner: KafkaConsumer,
    svc: Arc<ShipmentService>,
}

impl PaymentConsumer {
    pub fn new(brokers: &str, group_id: &str, svc: Arc<ShipmentService>) -> anyhow::Result<Self> {
        let inner = KafkaConsumer::new(
            brokers,
            &format!("{group_id}-payment"),
            &[topics::PAYMENT_INTENT_CAPTURED, topics::PAYMENT_INTENT_FAILED],
        )
        .context("Failed to create PaymentConsumer Kafka client")?;
        Ok(Self { inner, svc })
    }

    pub async fn run(self) {
        let svc = self.svc;

        let result = self.inner.run(move |topic, json| {
            let svc = Arc::clone(&svc);
            async move { handle(&topic, json, &svc).await }
        }).await;

        if let Err(e) = result {
            tracing::error!("PaymentConsumer loop exited with error: {e}");
        }
    }
}

/// Dispatches on topic, deserializes into the strongly-typed envelope for
/// that topic, and filters to `purpose == "shipping_fee"` before acting —
/// any other purpose (future consumers on other services) is a silent no-op
/// here, not an error.
pub async fn handle(topic: &str, json: serde_json::Value, svc: &ShipmentService) -> anyhow::Result<()> {
    match topic {
        topics::PAYMENT_INTENT_CAPTURED => {
            let evt: Event<PaymentIntentCaptured> = serde_json::from_value(json)
                .context("failed to deserialize payment.intent.captured event")?;
            if evt.data.purpose != SHIPPING_FEE_PURPOSE {
                return Ok(());
            }
            handle_captured(evt.data.reference_id, svc).await
        }
        topics::PAYMENT_INTENT_FAILED => {
            let evt: Event<PaymentIntentFailed> = serde_json::from_value(json)
                .context("failed to deserialize payment.intent.failed event")?;
            if evt.data.purpose != SHIPPING_FEE_PURPOSE {
                return Ok(());
            }
            handle_failed(evt.data.reference_id, &evt.data.reason, svc).await
        }
        _ => Ok(()),
    }
}

/// Republishes the three dispatch events a shipment held while awaiting
/// payment, and marks it `Paid`. Idempotent: a shipment that is not (or is
/// no longer) `AwaitingPayment` — including a replay after this handler
/// already ran — is a no-op.
pub async fn handle_captured(shipment_id: Uuid, svc: &ShipmentService) -> anyhow::Result<()> {
    let id = ShipmentId::from_uuid(shipment_id);
    let mut shipment = svc.repo.find_by_id(&id).await?
        .ok_or_else(|| anyhow::anyhow!("no shipment {shipment_id} for captured payment"))?;

    // Already fully processed: paid *and* the held events handed off. A
    // redelivered capture stops here.
    if shipment.payment_status == PaymentRequirement::Paid
        && shipment.pending_dispatch_events.is_none()
    {
        tracing::info!(
            shipment_id = %shipment_id,
            "payment.intent.captured — already processed, idempotent skip"
        );
        return Ok(());
    }

    // A shipment that is `Paid` but still holding its events got part-way
    // through a previous delivery: the money state was saved, then publishing
    // failed. Fall through and retry the publish rather than skipping — the
    // customer has paid and these three events are the only thing that will
    // ever get their shipment dispatched.
    if shipment.payment_status != PaymentRequirement::AwaitingPayment
        && shipment.payment_status != PaymentRequirement::Paid
    {
        tracing::info!(
            shipment_id = %shipment_id,
            payment_status = shipment.payment_status.as_str(),
            "payment.intent.captured — shipment not awaiting payment, idempotent skip"
        );
        return Ok(());
    }

    let events = shipment.pending_dispatch_events.clone().ok_or_else(|| {
        anyhow::anyhow!(
            "shipment {shipment_id} is awaiting_payment but has no pending_dispatch_events"
        )
    })?;

    // Persist `Paid` before publishing, so the money state is durable even if
    // publishing then fails. `pending_dispatch_events` is deliberately *not*
    // cleared here — it is the only record of what still needs to reach
    // dispatch, and clearing it before the events are actually out would lose
    // them with no way to recover, on the one path where the customer has
    // already been charged.
    if shipment.payment_status != PaymentRequirement::Paid {
        shipment.payment_status = PaymentRequirement::Paid;
        svc.repo.save(&shipment).await?;
    }

    let awb_key = shipment.awb.as_str().to_string();
    let shipment_key = shipment_id.to_string();
    let republish_targets: [(&str, &str, &str); 3] = [
        (topics::AWB_ISSUED, &awb_key, "awb_issued"),
        (topics::SHIPMENT_CREATED, &shipment_key, "shipment_created"),
        (topics::SHIPMENT_CONFIRMED, &shipment_key, "shipment_confirmed"),
    ];

    let mut failures = Vec::new();
    for (topic, key, data_key) in republish_targets {
        match events.get(data_key) {
            Some(payload) => {
                if let Err(e) = svc.publisher.publish(topic, key, &payload.to_string()).await {
                    tracing::error!(
                        shipment_id = %shipment_id, topic, error = %e,
                        "failed to republish held dispatch event after payment capture"
                    );
                    failures.push(topic);
                }
            }
            None => {
                // Nothing to publish and nothing a retry could fix — a missing
                // key is a malformed record, not a transient failure.
                tracing::error!(
                    shipment_id = %shipment_id, topic, data_key,
                    "pending_dispatch_events missing expected key — event not republished"
                );
            }
        }
    }

    if !failures.is_empty() {
        // Leave `pending_dispatch_events` in place and fail the message, so
        // Kafka redelivers and the block above retries the publish. The
        // `Paid` state is already saved, so nothing is charged twice.
        anyhow::bail!(
            "shipment {shipment_id}: {} of 3 held dispatch events failed to republish ({:?}) —              retained for redelivery",
            failures.len(),
            failures,
        );
    }

    // Every event is out. Only now is it safe to drop the held copy.
    shipment.pending_dispatch_events = None;
    svc.repo.save(&shipment).await?;

    Ok(())
}

/// Cancels a shipment whose payment failed or expired, via the same
/// `ShipmentService::cancel()` a merchant-initiated cancellation uses.
/// Idempotent: a shipment already past a cancellable status (already
/// cancelled by an earlier delivery of this same event, or otherwise
/// terminal) is a no-op rather than an error — this event can legitimately
/// be redelivered (Kafka at-least-once) or double-published (a declined
/// webhook racing the sweep's expiry), and a hard error here would leave a
/// poison-pill message blocking the consumer forever.
async fn handle_failed(shipment_id: Uuid, reason: &str, svc: &ShipmentService) -> anyhow::Result<()> {
    let id = ShipmentId::from_uuid(shipment_id);
    let shipment = svc.repo.find_by_id(&id).await?
        .ok_or_else(|| anyhow::anyhow!("no shipment {shipment_id} for failed payment"))?;

    if !shipment.can_cancel() {
        tracing::info!(
            shipment_id = %shipment_id,
            status = ?shipment.status,
            "payment.intent.failed — shipment no longer cancellable, idempotent skip"
        );
        return Ok(());
    }

    svc.cancel(CancelShipmentCommand { shipment_id, reason: reason.to_string() })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
}
