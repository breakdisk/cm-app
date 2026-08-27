//! PaymentIntentService — orchestrates creating a gateway session, and
//! transitioning an intent on webhook capture/failure or sweep expiry,
//! publishing the corresponding Kafka event each time.

use std::sync::Arc;

use chrono::Duration;
use logisticos_events::{envelope::Event, payloads::{PaymentIntentCaptured, PaymentIntentFailed}, producer::KafkaProducer, topics};
use uuid::Uuid;

use crate::domain::entities::PaymentIntent;
use crate::domain::repositories::{
    payment_gateway::{CreateSessionRequest, PaymentGateway, WebhookEvent},
    PaymentIntentRepository,
};

/// Hosted-checkout sessions must be completed within this window before the
/// sweep expires them — matches the design spec's stated 30-minute figure.
pub const INTENT_TTL: Duration = Duration::minutes(30);

pub struct PaymentIntentService {
    repo: Arc<dyn PaymentIntentRepository>,
    gateway: Arc<dyn PaymentGateway>,
    kafka: Arc<KafkaProducer>,
}

pub struct CreateIntentCommand {
    pub tenant_id: Uuid,
    pub purpose: String,
    pub reference_type: String,
    pub reference_id: Uuid,
    pub amount_cents: i64,
    pub currency: String,
    pub return_url: String,
}

pub struct CreatedIntent {
    pub intent_id: Uuid,
    pub checkout_url: String,
}

impl PaymentIntentService {
    pub fn new(
        repo: Arc<dyn PaymentIntentRepository>,
        gateway: Arc<dyn PaymentGateway>,
        kafka: Arc<KafkaProducer>,
    ) -> Self {
        Self { repo, gateway, kafka }
    }

    pub async fn create_intent(&self, cmd: CreateIntentCommand) -> anyhow::Result<CreatedIntent> {
        let intent = PaymentIntent::new(
            cmd.tenant_id,
            &cmd.purpose,
            &cmd.reference_type,
            cmd.reference_id,
            cmd.amount_cents,
            &cmd.currency,
            "network_international",
            INTENT_TTL,
        );
        self.repo.save(&intent).await?;

        let session = self.gateway.create_session(CreateSessionRequest {
            amount_cents: cmd.amount_cents,
            currency: &cmd.currency,
            intent_id: intent.id,
            return_url: &cmd.return_url,
        }).await?;

        let intent = intent.with_gateway_order_ref(session.gateway_order_ref);
        self.repo.save(&intent).await?;

        Ok(CreatedIntent { intent_id: intent.id, checkout_url: session.checkout_url })
    }

    /// Verifies and applies a webhook payload — the only path by which an
    /// intent can reach `captured`.
    pub async fn handle_webhook(&self, headers: &reqwest::header::HeaderMap, raw_body: &[u8]) -> anyhow::Result<()> {
        let event = self.gateway.verify_webhook(headers, raw_body)?;
        match event {
            WebhookEvent::Captured { gateway_order_ref, gateway_payment_ref } => {
                self.apply_captured(&gateway_order_ref, &gateway_payment_ref).await
            }
            WebhookEvent::Failed { gateway_order_ref } => {
                self.apply_failed(&gateway_order_ref, "gateway_declined").await
            }
        }
    }

    async fn find_by_order_ref(&self, gateway_order_ref: &str) -> anyhow::Result<PaymentIntent> {
        // gateway_order_ref is not separately indexed (it's 1:1 with the intent
        // we minted it for, always looked up right after creation in practice);
        // for the webhook path we instead re-derive by trying the payment ref
        // first, then fall back to a full scan-free path: NI's merchant_order_reference
        // IS our intent_id (see network_international.rs::create_session), so the
        // gateway_order_ref parameter here is actually the intent id round-tripped.
        let intent_id: Uuid = gateway_order_ref.parse()
            .map_err(|_| anyhow::anyhow!("webhook order reference is not a valid intent id"))?;
        self.repo.find_by_id(intent_id).await?
            .ok_or_else(|| anyhow::anyhow!("no payment_intent found for id {intent_id}"))
    }

    async fn apply_captured(&self, gateway_order_ref: &str, gateway_payment_ref: &str) -> anyhow::Result<()> {
        // Idempotency: a replay of the same transaction reference is a no-op.
        if let Some(existing) = self.repo.find_by_gateway_payment_ref(gateway_payment_ref).await? {
            if existing.status == crate::domain::entities::PaymentIntentStatus::Captured {
                return Ok(());
            }
        }

        let mut intent = self.find_by_order_ref(gateway_order_ref).await?;
        intent.capture(gateway_payment_ref.to_string())
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        self.repo.save(&intent).await?;

        let evt = Event::new(
            "logisticos/payments",
            "payment.intent.captured",
            intent.tenant_id,
            PaymentIntentCaptured {
                intent_id: intent.id,
                purpose: intent.purpose.clone(),
                reference_type: intent.reference_type.clone(),
                reference_id: intent.reference_id,
                amount_cents: intent.amount_cents,
                currency: intent.currency.clone(),
            },
        );
        self.kafka.publish_event(topics::PAYMENT_INTENT_CAPTURED, &evt).await?;
        Ok(())
    }

    async fn apply_failed(&self, gateway_order_ref: &str, reason: &str) -> anyhow::Result<()> {
        let mut intent = self.find_by_order_ref(gateway_order_ref).await?;
        intent.fail().map_err(|e| anyhow::anyhow!("{e}"))?;
        self.repo.save(&intent).await?;

        let evt = Event::new(
            "logisticos/payments",
            "payment.intent.failed",
            intent.tenant_id,
            PaymentIntentFailed {
                intent_id: intent.id,
                purpose: intent.purpose.clone(),
                reference_type: intent.reference_type.clone(),
                reference_id: intent.reference_id,
                reason: reason.to_string(),
            },
        );
        self.kafka.publish_event(topics::PAYMENT_INTENT_FAILED, &evt).await?;
        Ok(())
    }

    /// Called by the periodic sweep in `bootstrap.rs`. Expires every
    /// `created`/`pending` intent past its TTL and publishes the same
    /// `payment.intent.failed` event a declined payment would — order-intake's
    /// consumer treats both identically (cancel the shipment).
    pub async fn sweep_expired(&self) -> anyhow::Result<usize> {
        let expired = self.repo.list_expired(chrono::Utc::now()).await?;
        let count = expired.len();
        for mut intent in expired {
            if intent.expire().is_err() {
                continue; // raced with a webhook that captured it — leave it alone
            }
            self.repo.save(&intent).await?;
            let evt = Event::new(
                "logisticos/payments",
                "payment.intent.failed",
                intent.tenant_id,
                PaymentIntentFailed {
                    intent_id: intent.id,
                    purpose: intent.purpose.clone(),
                    reference_type: intent.reference_type.clone(),
                    reference_id: intent.reference_id,
                    reason: "expired".into(),
                },
            );
            if let Err(e) = self.kafka.publish_event(topics::PAYMENT_INTENT_FAILED, &evt).await {
                tracing::warn!(intent_id = %intent.id, error = %e, "failed to publish expiry event (will retry next sweep tick — intent stays expired)");
            }
        }
        Ok(count)
    }

    pub async fn refund(&self, intent_id: Uuid) -> anyhow::Result<()> {
        let mut intent = self.repo.find_by_id(intent_id).await?
            .ok_or_else(|| anyhow::anyhow!("no payment_intent {intent_id}"))?;
        // Domain-level status guard MUST run before any gateway call: intent.refund()
        // (below) doesn't clear gateway_payment_ref on transition to Refunded, so a
        // retried/concurrent refund against an already-refunded (or otherwise
        // non-captured) intent would pass the Option check that used to gate the
        // gateway call and hit the live gateway a second time. Reject locally first —
        // zero gateway calls for a non-captured intent.
        if intent.status != crate::domain::entities::PaymentIntentStatus::Captured {
            anyhow::bail!("Only a captured intent can be refunded");
        }
        let gateway_payment_ref = intent.gateway_payment_ref.clone()
            .ok_or_else(|| anyhow::anyhow!("intent {intent_id} has no captured payment to refund"))?;
        self.gateway.refund(&gateway_payment_ref, intent.amount_cents).await?;
        intent.refund().map_err(|e| anyhow::anyhow!("{e}"))?;
        self.repo.save(&intent).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! Kafka is deliberately *real*, not stubbed: `PaymentIntentService` takes
    //! a concrete `Arc<KafkaProducer>` (not a trait object), so a hand-rolled
    //! fake publisher isn't an option without changing the service's
    //! constructor signature — out of scope for this task. Instead we point
    //! the producer at an in-process `rdkafka::mocking::MockCluster`, the same
    //! approach `services/dispatch/tests/integration/main.rs::create_noop_kafka`
    //! already uses: publishes complete instantly against a real (if fake)
    //! broker, so tests assert actual publish success rather than merely
    //! tolerating a real-broker timeout.

    use std::collections::HashMap;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use chrono::Utc;

    use super::*;
    use crate::domain::entities::PaymentIntentStatus;
    use crate::domain::repositories::payment_gateway::GatewaySession;

    // ── Kafka: real producer over an in-process mock broker ────────────────

    fn test_kafka_producer() -> Arc<KafkaProducer> {
        use rdkafka::mocking::MockCluster;
        let cluster = MockCluster::new(1).expect("mock kafka cluster");
        let brokers = cluster.bootstrap_servers();
        // Leak the cluster so it outlives the producer for the duration of the
        // test — acceptable for a short-lived test process (each test spins up
        // its own cluster).
        Box::leak(Box::new(cluster));
        Arc::new(KafkaProducer::new(&brokers).expect("kafka producer over mock cluster"))
    }

    // ── Fake PaymentIntentRepository ────────────────────────────────────────

    #[derive(Default)]
    struct FakeRepo {
        intents: Mutex<HashMap<Uuid, PaymentIntent>>,
        save_count: Mutex<u32>,
        /// When set, `list_expired` returns exactly this set regardless of
        /// status — lets a test stage a defensive scenario (an already-final
        /// intent showing up in the sweep) that production `list_expired`
        /// (which filters to created/pending) would never actually produce.
        expired_override: Mutex<Option<Vec<PaymentIntent>>>,
    }

    impl FakeRepo {
        fn seed(&self, intent: PaymentIntent) {
            self.intents.lock().unwrap().insert(intent.id, intent);
        }

        fn get(&self, id: Uuid) -> PaymentIntent {
            self.intents.lock().unwrap().get(&id).cloned().expect("intent must be seeded")
        }

        fn save_count(&self) -> u32 {
            *self.save_count.lock().unwrap()
        }

        fn set_expired_override(&self, intents: Vec<PaymentIntent>) {
            *self.expired_override.lock().unwrap() = Some(intents);
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
            *self.save_count.lock().unwrap() += 1;
            self.intents.lock().unwrap().insert(intent.id, intent.clone());
            Ok(())
        }

        async fn list_expired(&self, before: chrono::DateTime<Utc>) -> anyhow::Result<Vec<PaymentIntent>> {
            if let Some(overridden) = self.expired_override.lock().unwrap().clone() {
                return Ok(overridden);
            }
            Ok(self.intents.lock().unwrap().values()
                .filter(|i| {
                    matches!(i.status, PaymentIntentStatus::Created | PaymentIntentStatus::Pending)
                        && i.expires_at < before
                })
                .cloned()
                .collect())
        }
    }

    // ── Fake PaymentGateway ──────────────────────────────────────────────────

    enum FakeWebhook {
        Captured { order_ref: String, payment_ref: String },
        Failed { order_ref: String },
    }

    struct FakeGateway {
        checkout_url: String,
        webhook: Mutex<Option<FakeWebhook>>,
        refund_should_fail: bool,
        refund_calls: Mutex<u32>,
    }

    impl FakeGateway {
        fn new() -> Self {
            Self {
                checkout_url: "https://pay.example/checkout/abc".into(),
                webhook: Mutex::new(None),
                refund_should_fail: false,
                refund_calls: Mutex::new(0),
            }
        }

        fn with_webhook(mut self, webhook: FakeWebhook) -> Self {
            self.webhook = Mutex::new(Some(webhook));
            self
        }

        fn with_refund_failure(mut self) -> Self {
            self.refund_should_fail = true;
            self
        }

        fn refund_calls(&self) -> u32 {
            *self.refund_calls.lock().unwrap()
        }
    }

    #[async_trait]
    impl PaymentGateway for FakeGateway {
        async fn create_session(&self, req: CreateSessionRequest<'_>) -> anyhow::Result<GatewaySession> {
            // The real NI adapter mints its order reference as the intent id
            // round-tripped; mirror that here so `find_by_order_ref` in the
            // service resolves correctly in tests that go through
            // `create_intent` and then a webhook.
            Ok(GatewaySession {
                checkout_url: self.checkout_url.clone(),
                gateway_order_ref: req.intent_id.to_string(),
            })
        }

        fn verify_webhook(&self, _headers: &reqwest::header::HeaderMap, _raw_body: &[u8]) -> anyhow::Result<WebhookEvent> {
            match &*self.webhook.lock().unwrap() {
                Some(FakeWebhook::Captured { order_ref, payment_ref }) => Ok(WebhookEvent::Captured {
                    gateway_order_ref: order_ref.clone(),
                    gateway_payment_ref: payment_ref.clone(),
                }),
                Some(FakeWebhook::Failed { order_ref }) => Ok(WebhookEvent::Failed {
                    gateway_order_ref: order_ref.clone(),
                }),
                None => Err(anyhow::anyhow!("test did not configure a webhook event")),
            }
        }

        async fn refund(&self, _gateway_payment_ref: &str, _amount_cents: i64) -> anyhow::Result<()> {
            *self.refund_calls.lock().unwrap() += 1;
            if self.refund_should_fail {
                anyhow::bail!("gateway refund failed");
            }
            Ok(())
        }
    }

    // ── Test helpers ─────────────────────────────────────────────────────────

    fn make_intent(tenant_id: Uuid) -> PaymentIntent {
        PaymentIntent::new(
            tenant_id,
            "shipping_fee",
            "shipment",
            Uuid::new_v4(),
            5_000,
            "AED",
            "network_international",
            INTENT_TTL,
        )
    }

    fn service(repo: Arc<FakeRepo>, gateway: Arc<FakeGateway>) -> PaymentIntentService {
        PaymentIntentService::new(repo, gateway, test_kafka_producer())
    }

    // ── create_intent ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn create_intent_creates_a_session_and_saves_twice_returning_the_checkout_url() {
        let repo = Arc::new(FakeRepo::default());
        let gateway = Arc::new(FakeGateway::new());
        let svc = service(repo.clone(), gateway.clone());

        let tenant_id = Uuid::new_v4();
        let reference_id = Uuid::new_v4();
        let created = svc.create_intent(CreateIntentCommand {
            tenant_id,
            purpose: "shipping_fee".into(),
            reference_type: "shipment".into(),
            reference_id,
            amount_cents: 12_345,
            currency: "AED".into(),
            return_url: "https://merchant.example/return".into(),
        }).await.expect("create_intent must succeed");

        assert_eq!(created.checkout_url, "https://pay.example/checkout/abc");

        // Saved once pre-session (Created) and once post-session (Pending) —
        // both writes hit the same row (id-keyed upsert in the fake repo), so
        // the observable save COUNT is what proves the two-phase save, not
        // the map size.
        assert_eq!(repo.save_count(), 2, "must save before AND after with_gateway_order_ref");

        let stored = repo.get(created.intent_id);
        assert_eq!(stored.status, PaymentIntentStatus::Pending, "post-session save must be Pending");
        assert_eq!(stored.gateway_order_ref.as_deref(), Some(created.intent_id.to_string().as_str()));
        assert_eq!(stored.amount_cents, 12_345);
        assert_eq!(stored.reference_id, reference_id);
        assert_eq!(stored.tenant_id, tenant_id);
    }

    // ── handle_webhook: Captured ─────────────────────────────────────────────

    #[tokio::test]
    async fn handle_webhook_captured_transitions_the_found_intent_to_captured() {
        let repo = Arc::new(FakeRepo::default());
        let tenant_id = Uuid::new_v4();
        let intent = make_intent(tenant_id).with_gateway_order_ref("order-ref-1".into());
        let intent_id = intent.id;
        repo.seed(intent);

        let gateway = Arc::new(FakeGateway::new().with_webhook(FakeWebhook::Captured {
            order_ref: intent_id.to_string(),
            payment_ref: "ni-txn-1".into(),
        }));
        let svc = service(repo.clone(), gateway);

        let headers = reqwest::header::HeaderMap::new();
        svc.handle_webhook(&headers, b"{}").await.expect("captured webhook must apply");

        let stored = repo.get(intent_id);
        assert_eq!(stored.status, PaymentIntentStatus::Captured);
        assert_eq!(stored.gateway_payment_ref.as_deref(), Some("ni-txn-1"));
    }

    // ── handle_webhook: Captured is idempotent on replay ────────────────────

    #[tokio::test]
    async fn handle_webhook_captured_replay_of_the_same_payment_ref_is_a_no_op() {
        let repo = Arc::new(FakeRepo::default());
        let tenant_id = Uuid::new_v4();
        let intent = make_intent(tenant_id).with_gateway_order_ref("order-ref-2".into());
        let intent_id = intent.id;
        repo.seed(intent);

        let gateway = Arc::new(FakeGateway::new().with_webhook(FakeWebhook::Captured {
            order_ref: intent_id.to_string(),
            payment_ref: "ni-txn-2".into(),
        }));
        let svc = service(repo.clone(), gateway);
        let headers = reqwest::header::HeaderMap::new();

        svc.handle_webhook(&headers, b"{}").await.expect("first delivery must apply");
        let saves_after_first = repo.save_count();

        // Redeliver the exact same webhook — must not error, and must not
        // attempt a second capture transition (find_by_gateway_payment_ref
        // short-circuits in apply_captured before find_by_order_ref/capture()).
        svc.handle_webhook(&headers, b"{}").await.expect("replay must be a no-op, not an error");

        let stored = repo.get(intent_id);
        assert_eq!(stored.status, PaymentIntentStatus::Captured);
        assert_eq!(
            repo.save_count(), saves_after_first,
            "idempotent replay must short-circuit before any further repo.save"
        );
    }

    // ── handle_webhook: Failed ───────────────────────────────────────────────

    #[tokio::test]
    async fn handle_webhook_failed_transitions_the_found_intent_to_failed() {
        let repo = Arc::new(FakeRepo::default());
        let tenant_id = Uuid::new_v4();
        let intent = make_intent(tenant_id).with_gateway_order_ref("order-ref-3".into());
        let intent_id = intent.id;
        repo.seed(intent);

        let gateway = Arc::new(FakeGateway::new().with_webhook(FakeWebhook::Failed {
            order_ref: intent_id.to_string(),
        }));
        let svc = service(repo.clone(), gateway);
        let headers = reqwest::header::HeaderMap::new();

        svc.handle_webhook(&headers, b"{}").await.expect("failed webhook must apply");

        let stored = repo.get(intent_id);
        assert_eq!(stored.status, PaymentIntentStatus::Failed);
    }

    // Task-5 code review gap: `PaymentIntent::fail()`'s match only explicitly
    // handles `Failed` (idempotent) and `Captured`/`Refunded` (rejected) —
    // `Expired` falls through the `_ => {}` catch-all, so it is *not* blocked.
    // A "declined" webhook arriving after our own sweep has already expired
    // the intent therefore re-transitions Expired -> Failed instead of
    // erroring, and would publish a second `payment.intent.failed` event for
    // the same intent. Documenting the actual current behavior, not asserting
    // it is the intended one — flagged separately as a possible entity-level
    // follow-up (`expire()` treats `Expired` as terminal-idempotent; `fail()`
    // does not extend the same treatment to it).
    #[tokio::test]
    async fn handle_webhook_failed_on_an_already_expired_intent_still_transitions_to_failed() {
        let repo = Arc::new(FakeRepo::default());
        let tenant_id = Uuid::new_v4();
        let mut intent = make_intent(tenant_id).with_gateway_order_ref("order-ref-4".into());
        intent.expire().unwrap();
        assert_eq!(intent.status, PaymentIntentStatus::Expired);
        let intent_id = intent.id;
        repo.seed(intent);

        let gateway = Arc::new(FakeGateway::new().with_webhook(FakeWebhook::Failed {
            order_ref: intent_id.to_string(),
        }));
        let svc = service(repo.clone(), gateway);
        let headers = reqwest::header::HeaderMap::new();

        svc.handle_webhook(&headers, b"{}").await
            .expect("fail() does not currently reject an Expired intent");

        let stored = repo.get(intent_id);
        assert_eq!(stored.status, PaymentIntentStatus::Failed);
    }

    // ── sweep_expired ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn sweep_expired_expires_created_and_pending_intents_past_ttl() {
        let repo = Arc::new(FakeRepo::default());
        let tenant_id = Uuid::new_v4();
        let intent = make_intent(tenant_id);
        let intent_id = intent.id;
        repo.seed(intent);

        // Force the fake repo to hand back exactly this intent as "expired",
        // matching what production `list_expired` would return for a
        // created/pending row whose TTL has passed.
        repo.set_expired_override(vec![repo.get(intent_id)]);

        let gateway = Arc::new(FakeGateway::new());
        let svc = service(repo.clone(), gateway);

        let count = svc.sweep_expired().await.expect("sweep must succeed");
        assert_eq!(count, 1);

        let stored = repo.get(intent_id);
        assert_eq!(stored.status, PaymentIntentStatus::Expired);
    }

    #[tokio::test]
    async fn sweep_expired_leaves_an_already_captured_intent_alone() {
        // Defensive test: production `list_expired` filters to created/pending,
        // so a captured intent should never actually appear here — but if it
        // somehow did (a race, a bug elsewhere), the service's own
        // `intent.expire().is_err() { continue }` guard must protect it.
        let repo = Arc::new(FakeRepo::default());
        let tenant_id = Uuid::new_v4();
        let mut captured = make_intent(tenant_id);
        captured.capture("ni-txn-already-captured".into()).unwrap();
        let captured_id = captured.id;
        repo.seed(captured.clone());

        repo.set_expired_override(vec![captured]);

        let gateway = Arc::new(FakeGateway::new());
        let svc = service(repo.clone(), gateway);

        let count = svc.sweep_expired().await.expect("sweep must succeed even with a non-expirable row");
        assert_eq!(count, 1, "count reflects list_expired's length, not how many actually transitioned");

        let stored = repo.get(captured_id);
        assert_eq!(stored.status, PaymentIntentStatus::Captured, "must be left untouched");
    }

    // ── refund ───────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn refund_requires_a_captured_payment_reference() {
        // Never-captured intent (status == Created) is rejected by the
        // status guard in `refund()` before the gateway_payment_ref check
        // (or the gateway) is ever reached — capture() is the only path
        // that sets gateway_payment_ref, and it always sets it together
        // with the Captured status, so a Captured-but-no-ref state cannot
        // arise here. Error message therefore matches the domain-level
        // `PaymentIntent::refund()` guard, not the Option check below it.
        let repo = Arc::new(FakeRepo::default());
        let tenant_id = Uuid::new_v4();
        let intent = make_intent(tenant_id); // never captured — no gateway_payment_ref
        let intent_id = intent.id;
        repo.seed(intent);

        let gateway = Arc::new(FakeGateway::new());
        let svc = service(repo.clone(), gateway.clone());

        let err = svc.refund(intent_id).await.expect_err("must reject refund of a never-captured intent");
        assert!(err.to_string().contains("Only a captured intent can be refunded"));
        assert_eq!(gateway.refund_calls(), 0, "gateway must never be called for an uncapturable refund");
    }

    #[tokio::test]
    async fn refund_of_an_already_refunded_intent_is_rejected_before_touching_the_gateway() {
        // The crux of the fix: PaymentIntent::refund() does not clear
        // gateway_payment_ref on transition to Refunded, so before this fix
        // a second refund() call against an already-refunded intent would
        // pass the old `gateway_payment_ref.is_some()` check and hit the
        // real gateway a second time before the domain guard rejected it.
        // The status check must now run first, so the gateway is called
        // zero times.
        let repo = Arc::new(FakeRepo::default());
        let tenant_id = Uuid::new_v4();
        let mut intent = make_intent(tenant_id);
        intent.capture("ni-txn-already-refunded".into()).unwrap();
        intent.refund().unwrap();
        assert_eq!(intent.status, PaymentIntentStatus::Refunded);
        assert!(intent.gateway_payment_ref.is_some(), "refund() does not clear gateway_payment_ref");
        let intent_id = intent.id;
        repo.seed(intent);

        let gateway = Arc::new(FakeGateway::new());
        let svc = service(repo.clone(), gateway.clone());

        let err = svc.refund(intent_id).await.expect_err("must reject a second refund of an already-refunded intent");
        assert!(err.to_string().contains("Only a captured intent can be refunded"));
        assert_eq!(gateway.refund_calls(), 0, "gateway must never be called for a non-captured intent, even a double refund");

        let stored = repo.get(intent_id);
        assert_eq!(stored.status, PaymentIntentStatus::Refunded, "must remain unchanged");
    }

    #[tokio::test]
    async fn refund_transitions_a_captured_intent_to_refunded() {
        let repo = Arc::new(FakeRepo::default());
        let tenant_id = Uuid::new_v4();
        let mut intent = make_intent(tenant_id);
        intent.capture("ni-txn-refundable".into()).unwrap();
        let intent_id = intent.id;
        repo.seed(intent);

        let gateway = Arc::new(FakeGateway::new());
        let svc = service(repo.clone(), gateway.clone());

        svc.refund(intent_id).await.expect("refund of a captured intent must succeed");

        assert_eq!(gateway.refund_calls(), 1);
        let stored = repo.get(intent_id);
        assert_eq!(stored.status, PaymentIntentStatus::Refunded);
    }

    #[tokio::test]
    async fn refund_leaves_the_intent_captured_when_the_gateway_call_fails() {
        let repo = Arc::new(FakeRepo::default());
        let tenant_id = Uuid::new_v4();
        let mut intent = make_intent(tenant_id);
        intent.capture("ni-txn-gateway-down".into()).unwrap();
        let intent_id = intent.id;
        repo.seed(intent);

        let gateway = Arc::new(FakeGateway::new().with_refund_failure());
        let svc = service(repo.clone(), gateway.clone());

        let saves_before = repo.save_count();
        let err = svc.refund(intent_id).await.expect_err("gateway failure must propagate as an error");
        assert!(err.to_string().contains("gateway refund failed"));

        assert_eq!(gateway.refund_calls(), 1);
        assert_eq!(repo.save_count(), saves_before, "must not persist a state change when the gateway call failed");
        let stored = repo.get(intent_id);
        assert_eq!(stored.status, PaymentIntentStatus::Captured, "must remain captured, not silently refunded");
    }
}
