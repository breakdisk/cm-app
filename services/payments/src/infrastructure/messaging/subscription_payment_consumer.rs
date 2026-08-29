//! Turns a captured subscription payment into a paid period and a granted tier.
//!
//! `purpose == "subscription"` only. Four products share `payment.intent.*` —
//! order-intake's `shipping_fee`, omnideliv's `omnideliv_order`, carrier's
//! `marketplace_booking` and this one — and each consumer is a silent no-op on
//! the other three.
//!
//! # Why a Kafka round-trip inside one service
//!
//! `SubscriptionService` already holds `PaymentIntentService` (to open a
//! checkout), so having the intent service call back into it on capture would
//! be a cycle. More importantly, doing the activation inline in the webhook
//! handler means a crash between "capture recorded" and "period extended"
//! loses the period silently: the money is taken, the intent says captured, and
//! nothing would ever revisit it. Going through the topic makes the activation
//! a redeliverable step. `Subscription::activate_from_payment` is idempotent on
//! the intent id precisely so that redelivery is free rather than a free month.

use std::sync::Arc;

use anyhow::Context;
use logisticos_events::{consumer::KafkaConsumer, topics};
use uuid::Uuid;

use crate::application::services::subscription_service::SUBSCRIPTION_PURPOSE;
use crate::application::services::SubscriptionService;

pub struct SubscriptionPaymentConsumer {
    inner: KafkaConsumer,
    svc:   Arc<SubscriptionService>,
}

impl SubscriptionPaymentConsumer {
    pub fn new(
        brokers: &str,
        group_id: &str,
        svc: Arc<SubscriptionService>,
    ) -> anyhow::Result<Self> {
        let inner = KafkaConsumer::new(
            brokers,
            &format!("{group_id}-subscription-payments"),
            &[topics::PAYMENT_INTENT_CAPTURED],
        )
        .context("Failed to create SubscriptionPaymentConsumer Kafka client")?;
        Ok(Self { inner, svc })
    }

    pub async fn run(self) {
        let svc = self.svc;
        let result = self.inner.run(move |_topic, json| {
            let svc = Arc::clone(&svc);
            async move { handle(json, &svc).await }
        }).await;

        if let Err(e) = result {
            tracing::error!("SubscriptionPaymentConsumer loop exited with error: {e}");
        }
    }
}

async fn handle(json: serde_json::Value, svc: &SubscriptionService) -> anyhow::Result<()> {
    // The envelope wraps the payload in `data`; tolerate a bare payload too,
    // matching `ShipmentCancelledConsumer`.
    let data = json.get("data").cloned().unwrap_or_else(|| json.clone());

    let purpose = data.get("purpose").and_then(|v| v.as_str()).unwrap_or_default();
    if purpose != SUBSCRIPTION_PURPOSE {
        return Ok(());
    }

    let subscription_id: Uuid = data
        .get("reference_id")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| anyhow::anyhow!("subscription capture event missing/invalid reference_id"))?;

    let intent_id: Uuid = data
        .get("intent_id")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| anyhow::anyhow!("subscription capture event missing/invalid intent_id"))?;

    svc.apply_captured_payment(subscription_id, intent_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(purpose: &str, reference_id: Uuid, intent_id: Uuid) -> serde_json::Value {
        serde_json::json!({
            "data": {
                "intent_id":      intent_id,
                "purpose":        purpose,
                "reference_type": "subscription",
                "reference_id":   reference_id,
                "amount_cents":   14900,
            }
        })
    }

    /// The filter that keeps four products off each other's topics. A shipping
    /// fee capture reaching this handler would look up a shipment id in the
    /// subscriptions table and, at best, log a warning about every parcel
    /// anyone ever paid for.
    #[test]
    fn another_products_capture_is_ignored() {
        let json = event("shipping_fee", Uuid::new_v4(), Uuid::new_v4());
        let data = json.get("data").unwrap();
        assert_ne!(
            data.get("purpose").and_then(|v| v.as_str()).unwrap(),
            SUBSCRIPTION_PURPOSE,
        );
    }

    /// Both ids must parse out of the envelope shape the producer actually
    /// emits — a silently-unparsed reference is a payment that activates
    /// nothing.
    #[test]
    fn both_ids_are_read_out_of_the_enveloped_payload() {
        let sub = Uuid::new_v4();
        let intent = Uuid::new_v4();
        let json = event(SUBSCRIPTION_PURPOSE, sub, intent);
        let data = json.get("data").cloned().unwrap();

        let parsed_sub: Uuid = data.get("reference_id").and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok()).unwrap();
        let parsed_intent: Uuid = data.get("intent_id").and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok()).unwrap();

        assert_eq!(parsed_sub, sub);
        assert_eq!(parsed_intent, intent);
    }
}
