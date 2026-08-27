//! Kafka consumer for `logisticos.order.shipment.cancelled`.
//!
//! order-intake publishes this event for every cancellation reason —
//! merchant/customer-initiated cancel (`ShipmentService::cancel`) and the
//! payment-expiry sweep (`ShipmentService::sweep_expired_payments`) alike.
//! If the cancelled shipment had a captured `shipping_fee` payment intent,
//! refund it. A shipment that was never paid online (the common case — cash
//! at pickup) has no captured intent and this handler is a no-op.
//!
//! # Refund-failure handling
//!
//! A refund failure is logged and swallowed (the handler still returns
//! `Ok(())`, so the Kafka offset commits) rather than propagated to force
//! redelivery. Two things make that the right call here, not just the
//! easier one:
//!
//! 1. The shipment cancellation itself is already durable in order-intake
//!    regardless of what this consumer does — there is nothing to roll back.
//! 2. `KafkaConsumer::run` (`libs/events/src/consumer.rs`) commits via
//!    `commit_message` per successfully-handled message, not a running
//!    low-water mark. On a single partition, if this event fails and is left
//!    uncommitted, the *next* `shipment.cancelled` event that succeeds will
//!    commit an offset past this one — silently and permanently skipping
//!    redelivery of the failed refund. Leaving the offset uncommitted here
//!    does not purchase a reliable retry; it only guarantees this consumer
//!    re-attempts the very same failing gateway call on every process
//!    restart until it happens to be overtaken, which is worse (repeated
//!    calls against a possibly-permanently-broken reference) for no
//!    compensating benefit.
//!
//! `PaymentIntentService::refund` marks the intent `Refunded` locally only
//! *after* the gateway call succeeds, so a failed refund leaves the intent
//! `Captured` — the real backstop is a future reconciliation sweep (symmetric
//! to `sweep_expired`) that can find `Captured` intents on `Cancelled`
//! shipments and retry deliberately, not Kafka's at-least-once semantics.
//! The `error!` log below is what makes a failure visible until such a sweep
//! exists.

use std::sync::Arc;
use anyhow::Context;
use logisticos_events::{consumer::KafkaConsumer, topics};
use uuid::Uuid;

use crate::application::services::PaymentIntentService;
use crate::domain::repositories::PaymentIntentRepository;

pub struct ShipmentCancelledConsumer {
    inner: KafkaConsumer,
    intent_repo: Arc<dyn PaymentIntentRepository>,
    intent_service: Arc<PaymentIntentService>,
}

impl ShipmentCancelledConsumer {
    pub fn new(
        brokers: &str,
        group_id: &str,
        intent_repo: Arc<dyn PaymentIntentRepository>,
        intent_service: Arc<PaymentIntentService>,
    ) -> anyhow::Result<Self> {
        let inner = KafkaConsumer::new(
            brokers,
            &format!("{group_id}-shipment-cancelled"),
            &[topics::SHIPMENT_CANCELLED],
        )
        .context("Failed to create ShipmentCancelledConsumer Kafka client")?;
        Ok(Self { inner, intent_repo, intent_service })
    }

    pub async fn run(self) {
        let intent_repo = self.intent_repo;
        let intent_service = self.intent_service;

        let result = self.inner.run(move |_topic, json| {
            let intent_repo = Arc::clone(&intent_repo);
            let intent_service = Arc::clone(&intent_service);
            async move { handle(json, &*intent_repo, &intent_service).await }
        }).await;

        if let Err(e) = result {
            tracing::error!("ShipmentCancelledConsumer loop exited with error: {e}");
        }
    }
}

async fn handle(
    json: serde_json::Value,
    intent_repo: &dyn PaymentIntentRepository,
    intent_service: &PaymentIntentService,
) -> anyhow::Result<()> {
    let data = json.get("data").cloned().unwrap_or_else(|| json.clone());
    let shipment_id: Uuid = data
        .get("shipment_id")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| anyhow::anyhow!("shipment.cancelled event missing/invalid shipment_id"))?;

    let Some(intent) = intent_repo
        .find_captured_by_reference("shipping_fee", "shipment", shipment_id)
        .await?
    else {
        tracing::debug!(shipment_id = %shipment_id, "shipment cancelled — no captured shipping_fee intent, nothing to refund");
        return Ok(());
    };

    if let Err(e) = intent_service.refund(intent.id).await {
        tracing::error!(
            shipment_id = %shipment_id,
            intent_id = %intent.id,
            error = %e,
            "refund failed after shipment cancellation — needs manual follow-up",
        );
        // Deliberately Ok(()) here — see the module doc comment for why this
        // is not a shortcut: propagating would not give a reliable retry
        // given how offsets commit in this consumer group, and the intent
        // stays Captured for a future reconciliation pass to pick up.
    } else {
        tracing::info!(shipment_id = %shipment_id, intent_id = %intent.id, "refunded captured shipping_fee intent for cancelled shipment");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    //! Kafka is real (in-process `MockCluster`), not stubbed — mirrors
    //! `payment_intent_service.rs`'s own test module: `PaymentIntentService`
    //! takes a concrete `Arc<KafkaProducer>`, not a trait object, so a
    //! hand-rolled fake publisher isn't an option without changing that
    //! service's constructor signature (out of scope here).

    use std::collections::HashMap;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use chrono::Utc;
    use logisticos_events::producer::KafkaProducer;

    use super::*;
    use crate::application::services::payment_intent_service::INTENT_TTL;
    use crate::domain::entities::{PaymentIntent, PaymentIntentStatus};
    use crate::domain::repositories::payment_gateway::{CreateSessionRequest, GatewaySession, PaymentGateway, WebhookEvent};

    fn test_kafka_producer() -> Arc<KafkaProducer> {
        use rdkafka::mocking::MockCluster;
        let cluster = MockCluster::new(1).expect("mock kafka cluster");
        let brokers = cluster.bootstrap_servers();
        // Leak the cluster so it outlives the producer for the duration of the
        // test — acceptable for a short-lived test process.
        Box::leak(Box::new(cluster));
        Arc::new(KafkaProducer::new(&brokers).expect("kafka producer over mock cluster"))
    }

    // ── Fake PaymentIntentRepository ────────────────────────────────────────

    #[derive(Default)]
    struct FakeRepo {
        intents: Mutex<HashMap<Uuid, PaymentIntent>>,
    }

    impl FakeRepo {
        fn seed(&self, intent: PaymentIntent) {
            self.intents.lock().unwrap().insert(intent.id, intent);
        }
    }

    #[async_trait]
    impl PaymentIntentRepository for FakeRepo {
        async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<PaymentIntent>> {
            Ok(self.intents.lock().unwrap().get(&id).cloned())
        }

        async fn find_by_gateway_payment_ref(&self, gateway_payment_ref: &str) -> anyhow::Result<Option<PaymentIntent>> {
            Ok(self.intents.lock().unwrap().values()
                .find(|i| i.gateway_payment_ref.as_deref() == Some(gateway_payment_ref))
                .cloned())
        }

        async fn save(&self, intent: &PaymentIntent) -> anyhow::Result<()> {
            self.intents.lock().unwrap().insert(intent.id, intent.clone());
            Ok(())
        }

        async fn list_expired(&self, _before: chrono::DateTime<Utc>) -> anyhow::Result<Vec<PaymentIntent>> {
            Ok(Vec::new())
        }

        async fn find_captured_by_reference(
            &self,
            purpose: &str,
            reference_type: &str,
            reference_id: Uuid,
        ) -> anyhow::Result<Option<PaymentIntent>> {
            Ok(self.intents.lock().unwrap().values()
                .find(|i| {
                    i.purpose == purpose
                        && i.reference_type == reference_type
                        && i.reference_id == reference_id
                        && i.status == PaymentIntentStatus::Captured
                })
                .cloned())
        }
    }

    // ── Fake PaymentGateway ──────────────────────────────────────────────────

    #[derive(Default)]
    struct FakeGateway {
        refund_calls: Mutex<Vec<(String, i64)>>,
    }

    impl FakeGateway {
        fn refund_calls(&self) -> Vec<(String, i64)> {
            self.refund_calls.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl PaymentGateway for FakeGateway {
        async fn create_session(&self, _req: CreateSessionRequest<'_>) -> anyhow::Result<GatewaySession> {
            unreachable!("not exercised by these tests")
        }

        fn verify_webhook(&self, _headers: &reqwest::header::HeaderMap, _raw_body: &[u8]) -> anyhow::Result<WebhookEvent> {
            unreachable!("not exercised by these tests")
        }

        async fn refund(&self, gateway_payment_ref: &str, amount_cents: i64) -> anyhow::Result<()> {
            self.refund_calls.lock().unwrap().push((gateway_payment_ref.to_string(), amount_cents));
            Ok(())
        }
    }

    // ── Test helpers ─────────────────────────────────────────────────────────

    fn captured_intent(tenant_id: Uuid, reference_id: Uuid, amount_cents: i64) -> PaymentIntent {
        let mut intent = PaymentIntent::new(
            tenant_id,
            "shipping_fee",
            "shipment",
            reference_id,
            amount_cents,
            "AED",
            "network_international",
            INTENT_TTL,
        );
        intent.capture(format!("ni-txn-{}", intent.id)).unwrap();
        intent
    }

    fn cancelled_event(shipment_id: Uuid) -> serde_json::Value {
        serde_json::json!({
            "id": Uuid::new_v4(),
            "source": "logisticos/order-intake",
            "event_type": "shipment.cancelled",
            "time": Utc::now(),
            "tenant_id": Uuid::new_v4(),
            "data": { "shipment_id": shipment_id, "reason": "merchant_requested" },
        })
    }

    // ── 1: no captured intent → no refund ────────────────────────────────────

    #[tokio::test]
    async fn shipment_never_paid_online_triggers_no_refund() {
        let repo = Arc::new(FakeRepo::default());
        let gateway = Arc::new(FakeGateway::default());
        let svc = Arc::new(PaymentIntentService::new(
            repo.clone() as _,
            gateway.clone() as _,
            test_kafka_producer(),
        ));

        let shipment_id = Uuid::new_v4();
        // repo has no intent seeded for this shipment at all — cash-at-pickup case.
        handle(cancelled_event(shipment_id), &*repo, &svc).await.expect("no-op must succeed");

        assert_eq!(gateway.refund_calls(), Vec::new(), "gateway must not be called when there is nothing to refund");
    }

    // ── 2: captured intent → exactly one refund ──────────────────────────────

    #[tokio::test]
    async fn shipment_with_captured_intent_triggers_exactly_one_refund() {
        let repo = Arc::new(FakeRepo::default());
        let gateway = Arc::new(FakeGateway::default());
        let svc = Arc::new(PaymentIntentService::new(
            repo.clone() as _,
            gateway.clone() as _,
            test_kafka_producer(),
        ));

        let tenant_id = Uuid::new_v4();
        let shipment_id = Uuid::new_v4();
        let intent = captured_intent(tenant_id, shipment_id, 7_500);
        let expected_ref = intent.gateway_payment_ref.clone().unwrap();
        repo.seed(intent);

        handle(cancelled_event(shipment_id), &*repo, &svc).await.expect("refund path must succeed");

        let calls = gateway.refund_calls();
        assert_eq!(calls.len(), 1, "gateway refund must be called exactly once");
        assert_eq!(calls[0], (expected_ref, 7_500), "refund must target the captured intent's own reference and amount");
    }

    // ── 3: malformed event → error, not a silent no-op ───────────────────────

    #[tokio::test]
    async fn malformed_event_missing_shipment_id_errors() {
        let repo = Arc::new(FakeRepo::default());
        let gateway = Arc::new(FakeGateway::default());
        let svc = Arc::new(PaymentIntentService::new(
            repo.clone() as _,
            gateway.clone() as _,
            test_kafka_producer(),
        ));

        let malformed = serde_json::json!({
            "id": Uuid::new_v4(),
            "source": "logisticos/order-intake",
            "event_type": "shipment.cancelled",
            "time": Utc::now(),
            "tenant_id": Uuid::new_v4(),
            "data": { "reason": "merchant_requested" },
        });

        let err = handle(malformed, &*repo, &svc).await.expect_err("missing shipment_id must error, not silently no-op");
        assert!(err.to_string().contains("shipment_id"));
        assert_eq!(gateway.refund_calls(), Vec::new());
    }
}
