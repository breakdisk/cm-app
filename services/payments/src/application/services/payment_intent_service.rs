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

/// Distinguishes "reject this webhook permanently" (bad signature, unknown
/// intent — NI should stop retrying) from "something transient broke after
/// the signature already verified" (DB/Kafka failure — NI should retry,
/// since the money was genuinely captured and must not be silently lost).
#[derive(Debug, thiserror::Error)]
pub enum WebhookError {
    #[error("webhook rejected: {0}")]
    Rejected(anyhow::Error),
    #[error("webhook processing failed: {0}")]
    Internal(anyhow::Error),
}

/// Marker for "the webhook's order/payment reference doesn't correspond to
/// any `payment_intent` row this service knows about." Returned (via the
/// ordinary `anyhow::Result` chain, not a signature change) from
/// `find_by_order_ref` so `handle_webhook` can downcast it out of the flat
/// `anyhow::Result<()>` that `apply_captured`/`apply_failed` still return,
/// and classify it as `WebhookError::Rejected` rather than `::Internal`.
///
/// This is safe specifically because this service has a single Postgres
/// primary with no read replica (see `infrastructure/db/payment_intent_repo.rs`):
/// `find_by_id` returning `None` here means the row genuinely does not
/// exist, not a stale/lagging read — a real transient DB error (timeout,
/// connection drop) on the SELECT itself surfaces as `Err` from `find_by_id`
/// and bypasses this marker entirely, so it is correctly classified as
/// `Internal` (retry) instead. An unknown intent id is therefore a permanent
/// condition: retrying the same webhook will never make the row appear.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
struct UnknownIntentError(String);

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
    ///
    /// Signature verification failures classify as `WebhookError::Rejected`
    /// unconditionally (permanent — NI should stop retrying). Failures from
    /// `apply_captured`/`apply_failed` — which run only after the signature
    /// already verified, meaning NI genuinely believes it captured real
    /// money — default to `WebhookError::Internal` (transient — NI should
    /// retry) EXCEPT the specific "unknown intent" case (see
    /// `UnknownIntentError`), which is also `Rejected` since retrying can't
    /// manufacture a row that will never exist.
    pub async fn handle_webhook(&self, headers: &reqwest::header::HeaderMap, raw_body: &[u8]) -> Result<(), WebhookError> {
        let event = self.gateway.verify_webhook(headers, raw_body).map_err(WebhookError::Rejected)?;
        let result = match event {
            WebhookEvent::Captured { gateway_order_ref, gateway_payment_ref } => {
                self.apply_captured(&gateway_order_ref, &gateway_payment_ref).await
            }
            WebhookEvent::Failed { gateway_order_ref } => {
                self.apply_failed(&gateway_order_ref, "gateway_declined").await
            }
        };
        result.map_err(|e| {
            if e.downcast_ref::<UnknownIntentError>().is_some() {
                WebhookError::Rejected(e)
            } else {
                WebhookError::Internal(e)
            }
        })
    }

    async fn find_by_order_ref(&self, gateway_order_ref: &str) -> anyhow::Result<PaymentIntent> {
        // gateway_order_ref is not separately indexed (it's 1:1 with the intent
        // we minted it for, always looked up right after creation in practice);
        // for the webhook path we instead re-derive by trying the payment ref
        // first, then fall back to a full scan-free path: NI's merchant_order_reference
        // IS our intent_id (see network_international.rs::create_session), so the
        // gateway_order_ref parameter here is actually the intent id round-tripped.
        //
        // Both failure branches below return `UnknownIntentError` (not a bare
        // `anyhow!(...)`) so `handle_webhook` can classify them as
        // `WebhookError::Rejected` — see that type's doc comment for why.
        let intent_id: Uuid = gateway_order_ref.parse().map_err(|_| {
            UnknownIntentError(format!("webhook order reference {gateway_order_ref:?} is not a valid intent id"))
        })?;
        let intent = self.repo.find_by_id(intent_id).await?
            .ok_or_else(|| UnknownIntentError(format!("no payment_intent found for id {intent_id}")))?;
        Ok(intent)
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
        let mut expired_count = 0;
        for mut intent in expired {
            if intent.expire().is_err() {
                continue; // raced with a webhook that captured it — leave it alone
            }
            if let Err(e) = self.repo.save(&intent).await {
                tracing::error!(intent_id = %intent.id, error = %e, "sweep_expired: failed to save expired intent — will retry next tick");
                continue; // don't publish an event for a state change that didn't persist
            }
            expired_count += 1;
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
        Ok(expired_count)
    }

    /// Refunds a captured intent. Two layers guard against ever calling the
    /// live gateway more than once for the same intent:
    ///
    /// 1. A local `status == Captured` check, rejecting the obviously-wrong
    ///    case (never captured, already refunded, ...) with zero repo/gateway
    ///    round-trips.
    /// 2. `repo.claim_for_refund` — an atomic `captured -> refunding` DB
    ///    claim. This, not step 1, is what actually serializes two genuinely
    ///    concurrent callers (the shipment-cancellation consumer and the
    ///    pending-refund retry sweep can both reach this method for the same
    ///    intent): both may pass the local check, but only one claim can win.
    ///    The loser bails without ever touching the gateway.
    ///
    /// `intent` is fetched BEFORE the claim and is deliberately never told
    /// about the claim's `refunding` status — it stays a `Captured` snapshot
    /// for the rest of this call. On gateway success, `intent.refund()`
    /// transitions that snapshot to `Refunded` (its own `Captured`-only
    /// guard is satisfied because the snapshot was never mutated). On
    /// gateway failure, that same still-`Captured` snapshot is saved back
    /// as-is — which is precisely the revert the claim needs: it overwrites
    /// the DB's `refunding` row back to `captured` so the pending-refund
    /// sweep retries it, rather than leaving it stuck.
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

        if !self.repo.claim_for_refund(intent_id).await? {
            anyhow::bail!("intent {intent_id} is already being refunded (lost the claim race)");
        }

        match self.gateway.refund(&gateway_payment_ref, intent.amount_cents).await {
            Ok(()) => {
                intent.refund().map_err(|e| anyhow::anyhow!("{e}"))?;
                self.repo.save(&intent).await?;
                Ok(())
            }
            Err(e) => {
                // Revert the claim: `intent` is still the pre-claim,
                // still-Captured snapshot (never mutated on this branch), so
                // saving it now writes 'captured' back over the DB's
                // 'refunding' row instead of leaving the intent stranded —
                // `sweep_pending_refunds` needs it back at `captured` (with
                // `refund_requested_at` still set) to retry it next tick.
                if let Err(save_err) = self.repo.save(&intent).await {
                    tracing::error!(
                        intent_id = %intent_id,
                        gateway_error = %e,
                        revert_error = %save_err,
                        "refund: gateway call failed AND reverting the 'refunding' claim also failed \
                         — intent is stuck in refunding until manually corrected",
                    );
                }
                Err(e)
            }
        }
    }

    /// Called by the periodic sweep in `bootstrap.rs`, alongside
    /// `sweep_expired`. Retries every `captured` intent with a recorded
    /// `refund_requested_at` — a refund `ShipmentCancelledConsumer` already
    /// asked for but never completed (the gateway call failed, or the
    /// process crashed between recording the obligation and calling the
    /// gateway). `refund()`'s own atomic claim already makes each retry safe
    /// against a concurrent attempt; this loop's job is only to not let one
    /// bad row abort the rest of the batch. Returns the count *actually
    /// refunded* in this pass, not the count found — mirrors `sweep_expired`.
    pub async fn sweep_pending_refunds(&self) -> anyhow::Result<usize> {
        let pending = self.repo.list_pending_refunds().await?;
        let mut refunded_count = 0;
        for intent in pending {
            match self.refund(intent.id).await {
                Ok(()) => refunded_count += 1,
                Err(e) => {
                    tracing::warn!(intent_id = %intent.id, error = %e, "sweep_pending_refunds: refund attempt failed — will retry next tick");
                    continue;
                }
            }
        }
        Ok(refunded_count)
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
        /// When set, `save()` returns an error for this one intent id (and
        /// leaves the stored row untouched) instead of persisting it — lets a
        /// test simulate a transient DB failure for exactly one row in a
        /// batch without a real database.
        fail_save_for: Mutex<Option<Uuid>>,
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

        fn set_fail_save_for(&self, id: Uuid) {
            *self.fail_save_for.lock().unwrap() = Some(id);
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
            if *self.fail_save_for.lock().unwrap() == Some(intent.id) {
                anyhow::bail!("simulated save failure for intent {}", intent.id);
            }
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

        async fn mark_refund_requested(&self, id: Uuid) -> anyhow::Result<()> {
            if let Some(intent) = self.intents.lock().unwrap().get_mut(&id) {
                // COALESCE-equivalent: don't reset an already-recorded timestamp.
                if intent.refund_requested_at.is_none() {
                    intent.refund_requested_at = Some(Utc::now());
                }
            }
            Ok(())
        }

        async fn claim_for_refund(&self, id: Uuid) -> anyhow::Result<bool> {
            let mut intents = self.intents.lock().unwrap();
            match intents.get_mut(&id) {
                Some(intent) if intent.status == PaymentIntentStatus::Captured => {
                    intent.status = PaymentIntentStatus::Refunding;
                    intent.updated_at = Utc::now();
                    Ok(true)
                }
                _ => Ok(false),
            }
        }

        async fn list_pending_refunds(&self) -> anyhow::Result<Vec<PaymentIntent>> {
            Ok(self.intents.lock().unwrap().values()
                .filter(|i| i.status == PaymentIntentStatus::Captured && i.refund_requested_at.is_some())
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

    /// A gateway that fails `refund` only for one specific
    /// `gateway_payment_ref`, succeeding for every other — lets
    /// `sweep_pending_refunds` tests seed two intents in the same batch with
    /// only one of them actually failing, using a single shared gateway
    /// (`PaymentIntentService` takes one `Arc<dyn PaymentGateway>`, so two
    /// intents processed by the same sweep call necessarily share one).
    struct SelectiveFailGateway {
        fail_ref: String,
        calls: Mutex<Vec<String>>,
    }

    impl SelectiveFailGateway {
        fn new(fail_ref: impl Into<String>) -> Self {
            Self { fail_ref: fail_ref.into(), calls: Mutex::new(Vec::new()) }
        }
    }

    #[async_trait]
    impl PaymentGateway for SelectiveFailGateway {
        async fn create_session(&self, _req: CreateSessionRequest<'_>) -> anyhow::Result<GatewaySession> {
            unreachable!("not exercised by sweep_pending_refunds tests")
        }

        fn verify_webhook(&self, _headers: &reqwest::header::HeaderMap, _raw_body: &[u8]) -> anyhow::Result<WebhookEvent> {
            unreachable!("not exercised by sweep_pending_refunds tests")
        }

        async fn refund(&self, gateway_payment_ref: &str, _amount_cents: i64) -> anyhow::Result<()> {
            self.calls.lock().unwrap().push(gateway_payment_ref.to_string());
            if gateway_payment_ref == self.fail_ref {
                anyhow::bail!("gateway refund failed for {gateway_payment_ref}");
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

    // Gap 3 (money-safety review): `PaymentIntent::fail()` used to fall
    // through its `_ => {}` catch-all for `Expired`, so a "declined" webhook
    // arriving after our own sweep had already expired the intent silently
    // re-transitioned Expired -> Failed — and `apply_failed` would then
    // publish a SECOND `payment.intent.failed` event for the same intent
    // (it doesn't check whether `fail()` actually changed anything before
    // publishing). `Expired` is now terminal, the same way `Captured` and
    // `Refunded` already were, so this webhook is rejected before it ever
    // reaches `repo.save`/`kafka.publish_event` — zero of either happen.
    //
    // This intentionally surfaces as `WebhookError::Internal` (5xx, NI
    // retries) rather than `::Rejected` — see that type's doc comment: this
    // service doesn't currently distinguish "permanently un-actionable
    // failure classification" from "transient infra failure" beyond the one
    // `UnknownIntentError` special case, and extending that is out of scope
    // here. NI's retry policy is bounded, and the alternative (silently
    // succeeding with a duplicate event) is the actual money-safety bug this
    // closes; order-intake's own consumer (`payment_consumer.rs::handle_failed`)
    // is separately idempotent via `shipment.can_cancel()`, so even a
    // hypothetical redelivery of the ORIGINAL (pre-expiry) failed event can
    // never cancel the same shipment twice.
    #[tokio::test]
    async fn handle_webhook_failed_on_an_already_expired_intent_is_rejected() {
        let repo = Arc::new(FakeRepo::default());
        let tenant_id = Uuid::new_v4();
        let mut intent = make_intent(tenant_id).with_gateway_order_ref("order-ref-4".into());
        intent.expire().unwrap();
        assert_eq!(intent.status, PaymentIntentStatus::Expired);
        let intent_id = intent.id;
        repo.seed(intent);
        let saves_before = repo.save_count();

        let gateway = Arc::new(FakeGateway::new().with_webhook(FakeWebhook::Failed {
            order_ref: intent_id.to_string(),
        }));
        let svc = service(repo.clone(), gateway);
        let headers = reqwest::header::HeaderMap::new();

        let err = svc.handle_webhook(&headers, b"{}").await
            .expect_err("fail() must now reject an already-Expired intent");
        assert!(
            matches!(err, WebhookError::Internal(_)),
            "not UnknownIntentError, so this classifies Internal (retry) — see the test doc comment above"
        );

        let stored = repo.get(intent_id);
        assert_eq!(stored.status, PaymentIntentStatus::Expired, "must remain Expired, not silently move to Failed");
        assert_eq!(repo.save_count(), saves_before, "rejected before any save — and therefore before any event publish");
    }

    // ── handle_webhook: error classification (Rejected vs Internal) ────────
    //
    // This is the crux of the fix: NI's retry policy depends on the HTTP
    // status the handler maps each variant to, so it matters which variant
    // each failure mode produces, not just that `handle_webhook` errors.

    #[tokio::test]
    async fn handle_webhook_with_a_bad_signature_is_rejected_not_internal() {
        // FakeGateway::verify_webhook errors when no webhook was configured —
        // stands in for a real signature-verification failure (bad/missing
        // HMAC). This must classify as Rejected: a bad signature is a
        // permanent condition, and 4xx tells NI to stop retrying.
        let repo = Arc::new(FakeRepo::default());
        let gateway = Arc::new(FakeGateway::new()); // no .with_webhook(...)
        let svc = service(repo, gateway);
        let headers = reqwest::header::HeaderMap::new();

        let err = svc.handle_webhook(&headers, b"{}").await
            .expect_err("unconfigured/unverifiable webhook must error");

        assert!(
            matches!(err, WebhookError::Rejected(_)),
            "signature verification failure must be Rejected (permanent), got {err:?}"
        );
    }

    #[tokio::test]
    async fn handle_webhook_for_an_unknown_intent_is_rejected_not_internal() {
        // Signature verifies fine, but the order_ref doesn't match any
        // payment_intent this service has ever seen (nothing was seeded).
        // Judgment call documented on `UnknownIntentError`: this service has
        // a single Postgres primary with no read replica, so `find_by_id`
        // returning `None` means the row genuinely doesn't exist, not a
        // lagging read — retrying the same webhook can never manufacture a
        // row that will never exist, so this must be Rejected (permanent),
        // not Internal (retry).
        let repo = Arc::new(FakeRepo::default());
        let unknown_intent_id = Uuid::new_v4();
        let gateway = Arc::new(FakeGateway::new().with_webhook(FakeWebhook::Captured {
            order_ref: unknown_intent_id.to_string(),
            payment_ref: "ni-txn-unknown".into(),
        }));
        let svc = service(repo, gateway);
        let headers = reqwest::header::HeaderMap::new();

        let err = svc.handle_webhook(&headers, b"{}").await
            .expect_err("webhook for an intent id we have no record of must error");

        assert!(
            matches!(err, WebhookError::Rejected(_)),
            "unknown intent must be Rejected (permanent), got {err:?}"
        );
    }

    #[tokio::test]
    async fn handle_webhook_when_the_post_verification_save_fails_is_internal_not_rejected() {
        // Signature verifies, the intent is found — but persisting the
        // Captured transition fails (simulated transient DB failure). This
        // is exactly the scenario the fix exists for: NI has already
        // genuinely captured the money, so this must be Internal (retry),
        // never Rejected (which would tell NI to stop retrying and let the
        // capture go unrecorded).
        let repo = Arc::new(FakeRepo::default());
        let tenant_id = Uuid::new_v4();
        let intent = make_intent(tenant_id).with_gateway_order_ref("order-ref-save-fail".into());
        let intent_id = intent.id;
        repo.seed(intent);
        repo.set_fail_save_for(intent_id);

        let gateway = Arc::new(FakeGateway::new().with_webhook(FakeWebhook::Captured {
            order_ref: intent_id.to_string(),
            payment_ref: "ni-txn-save-fail".into(),
        }));
        let svc = service(repo.clone(), gateway);
        let headers = reqwest::header::HeaderMap::new();

        let err = svc.handle_webhook(&headers, b"{}").await
            .expect_err("a save failure after signature verification must error");

        assert!(
            matches!(err, WebhookError::Internal(_)),
            "post-verification DB save failure must be Internal (retry), got {err:?}"
        );

        let stored = repo.get(intent_id);
        assert_eq!(
            stored.status, PaymentIntentStatus::Pending,
            "the failed save must not have persisted the Captured transition"
        );
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
        assert_eq!(count, 0, "count reflects how many intents were actually expired, not list_expired's length");

        let stored = repo.get(captured_id);
        assert_eq!(stored.status, PaymentIntentStatus::Captured, "must be left untouched");
    }

    #[tokio::test]
    async fn sweep_expired_continues_past_a_save_failure_for_one_intent() {
        // The crux of the fix: a repo.save() failure for one row must not
        // abort the loop and skip every OTHER stale intent in the same tick.
        let repo = Arc::new(FakeRepo::default());
        let tenant_id = Uuid::new_v4();

        let ok_intent = make_intent(tenant_id);
        let ok_id = ok_intent.id;
        repo.seed(ok_intent);

        let fail_intent = make_intent(tenant_id);
        let fail_id = fail_intent.id;
        repo.seed(fail_intent);

        repo.set_expired_override(vec![repo.get(ok_id), repo.get(fail_id)]);
        repo.set_fail_save_for(fail_id);

        let gateway = Arc::new(FakeGateway::new());
        let svc = service(repo.clone(), gateway);

        let count = svc.sweep_expired().await
            .expect("sweep must not abort the whole batch when one intent's save fails");
        assert_eq!(count, 1, "count must reflect only the successfully-expired intent, not both");

        let ok_stored = repo.get(ok_id);
        assert_eq!(ok_stored.status, PaymentIntentStatus::Expired, "the other intent must still be expired and saved");

        let fail_stored = repo.get(fail_id);
        assert_eq!(
            fail_stored.status, PaymentIntentStatus::Created,
            "a failed save must leave the persisted row untouched — expire() only mutated the in-memory copy, which was never written"
        );
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

    // Gap 1 + Gap 2 (money-safety review): before this fix, a gateway
    // failure just propagated the error and left the DB untouched — no
    // atomic claim existed to revert. Now `refund()` claims (captured ->
    // refunding) before calling the gateway, and reverts (refunding ->
    // captured) on failure, so `repo.save` IS called once here (the revert)
    // — this replaces the old "must not persist a state change" assertion,
    // which described the pre-claim behavior. `refund_requested_at` is
    // asserted to survive the round trip: it's what makes the intent
    // discoverable by `sweep_pending_refunds` afterwards (see the next test).
    #[tokio::test]
    async fn refund_leaves_the_intent_captured_when_the_gateway_call_fails() {
        let repo = Arc::new(FakeRepo::default());
        let tenant_id = Uuid::new_v4();
        let mut intent = make_intent(tenant_id);
        intent.capture("ni-txn-gateway-down".into()).unwrap();
        intent.refund_requested_at = Some(Utc::now()); // as ShipmentCancelledConsumer would have set via mark_refund_requested
        let intent_id = intent.id;
        repo.seed(intent);

        let gateway = Arc::new(FakeGateway::new().with_refund_failure());
        let svc = service(repo.clone(), gateway.clone());

        let saves_before = repo.save_count();
        let err = svc.refund(intent_id).await.expect_err("gateway failure must propagate as an error");
        assert!(err.to_string().contains("gateway refund failed"));

        assert_eq!(gateway.refund_calls(), 1);
        assert_eq!(
            repo.save_count(), saves_before + 1,
            "exactly one save: the revert of the 'refunding' claim back to 'captured'"
        );
        let stored = repo.get(intent_id);
        assert_eq!(stored.status, PaymentIntentStatus::Captured, "must remain captured, not stuck in refunding, not silently refunded");
        assert!(stored.refund_requested_at.is_some(), "the refund obligation must survive the revert so the sweep can retry it");
    }

    // ── refund: atomic claim (Gap 2) ────────────────────────────────────────

    #[tokio::test]
    async fn claim_for_refund_only_lets_one_caller_win_the_race() {
        // Unit-level proof of the primitive itself: two calls against the
        // same captured row — only the first may claim it.
        let repo = FakeRepo::default();
        let tenant_id = Uuid::new_v4();
        let mut intent = make_intent(tenant_id);
        intent.capture("ni-txn-claim-race".into()).unwrap();
        let intent_id = intent.id;
        repo.seed(intent);

        let first = repo.claim_for_refund(intent_id).await.unwrap();
        let second = repo.claim_for_refund(intent_id).await.unwrap();

        assert!(first, "first claim must win");
        assert!(!second, "second claim on an already-refunding row must lose");
    }

    #[tokio::test]
    async fn refund_does_not_call_the_gateway_when_another_caller_already_holds_the_claim() {
        // Simulates the real race Gap 2 closes: the cancellation consumer and
        // the pending-refund sweep can both reach `refund()` for the same
        // intent. Here "the other caller" is simulated by claiming the
        // intent directly against the repo before `svc.refund()` runs, as
        // the losing caller.
        let repo = Arc::new(FakeRepo::default());
        let tenant_id = Uuid::new_v4();
        let mut intent = make_intent(tenant_id);
        intent.capture("ni-txn-race".into()).unwrap();
        let intent_id = intent.id;
        repo.seed(intent);

        assert!(repo.claim_for_refund(intent_id).await.unwrap(), "setup: the other (winning) caller's claim must succeed");

        let gateway = Arc::new(FakeGateway::new());
        let svc = service(repo.clone(), gateway.clone());

        svc.refund(intent_id).await.expect_err("a caller that lost the claim race must not proceed");
        assert_eq!(gateway.refund_calls(), 0, "the gateway must never be called by the caller that lost the claim race");

        let stored = repo.get(intent_id);
        assert_eq!(stored.status, PaymentIntentStatus::Refunding, "still owned by the winning caller — untouched by the loser");
    }

    // ── sweep_pending_refunds ────────────────────────────────────────────────

    #[tokio::test]
    async fn sweep_pending_refunds_retries_a_previously_failed_refund_and_succeeds() {
        let repo = Arc::new(FakeRepo::default());
        let tenant_id = Uuid::new_v4();
        let mut intent = make_intent(tenant_id);
        intent.capture("ni-txn-retry-me".into()).unwrap();
        intent.refund_requested_at = Some(Utc::now());
        let intent_id = intent.id;
        repo.seed(intent);

        let gateway = Arc::new(FakeGateway::new()); // succeeds this time
        let svc = service(repo.clone(), gateway.clone());

        let count = svc.sweep_pending_refunds().await.expect("sweep must succeed");
        assert_eq!(count, 1);
        assert_eq!(gateway.refund_calls(), 1);

        let stored = repo.get(intent_id);
        assert_eq!(stored.status, PaymentIntentStatus::Refunded);
    }

    #[tokio::test]
    async fn sweep_pending_refunds_returns_the_actually_refunded_count_and_continues_past_a_failure() {
        // Seed two intents with an outstanding refund obligation: one whose
        // gateway call will succeed, one whose gateway call will keep
        // failing (a single shared FakeGateway can only be configured to
        // always-fail or always-succeed, so the "failing" row uses an
        // intent id the FakeGateway is never asked to know about — instead
        // we drive the failure via a per-intent gateway wrapper).
        let repo = Arc::new(FakeRepo::default());
        let tenant_id = Uuid::new_v4();

        let mut ok_intent = make_intent(tenant_id);
        ok_intent.capture("ni-txn-sweep-ok".into()).unwrap();
        ok_intent.refund_requested_at = Some(Utc::now());
        let ok_id = ok_intent.id;
        repo.seed(ok_intent);

        let mut fail_intent = make_intent(tenant_id);
        fail_intent.capture("ni-txn-sweep-fail".into()).unwrap();
        fail_intent.refund_requested_at = Some(Utc::now());
        let fail_id = fail_intent.id;
        repo.seed(fail_intent);

        let gateway = Arc::new(SelectiveFailGateway::new("ni-txn-sweep-fail"));
        let svc = PaymentIntentService::new(repo.clone(), gateway.clone(), test_kafka_producer());

        let count = svc.sweep_pending_refunds().await
            .expect("sweep must not abort the whole batch when one intent's refund fails");
        assert_eq!(count, 1, "count must reflect only the successfully-refunded intent");

        let ok_stored = repo.get(ok_id);
        assert_eq!(ok_stored.status, PaymentIntentStatus::Refunded);

        let fail_stored = repo.get(fail_id);
        assert_eq!(fail_stored.status, PaymentIntentStatus::Captured, "left retryable, not stuck in refunding");
        assert!(fail_stored.refund_requested_at.is_some(), "obligation preserved for the next sweep tick");
    }
}
