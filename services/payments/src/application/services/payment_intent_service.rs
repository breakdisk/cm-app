//! PaymentIntentService — orchestrates creating a gateway session, and
//! transitioning an intent on webhook capture/failure or sweep expiry,
//! publishing the corresponding Kafka event each time.

use std::sync::Arc;

use chrono::Duration;
use logisticos_events::{envelope::Event, payloads::{PaymentIntentAuthorized, PaymentIntentCaptured, PaymentIntentFailed}, producer::KafkaProducer, topics};
use uuid::Uuid;

use crate::domain::entities::{PaymentIntent, PaymentIntentStatus};
use crate::domain::repositories::{
    payment_gateway::{CreateSessionRequest, PaymentAction, PaymentGateway, WebhookEvent},
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
    /// `Sale` (immediate capture, the original behavior) or `Authorize`
    /// (ring-fence only — OmniDeliv's prepaid-checkout foundation).
    pub action: PaymentAction,
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
            action: cmd.action,
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
            WebhookEvent::Authorized { gateway_order_ref, gateway_payment_ref } => {
                self.apply_authorized(&gateway_order_ref, &gateway_payment_ref).await
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

    /// Resolves a webhook's `orderReference` to the intent it belongs to.
    ///
    /// Which value NI actually echoes back here is unverified against a live
    /// sandbox — it could be our own `merchant_order_reference` (our intent
    /// id, passed to `create_session`) or NI's own `reference` (stored on the
    /// intent as `gateway_order_ref` by `create_session` — see
    /// `network_international.rs`). Getting this wrong is the
    /// highest-consequence unknown in the integration: if only one
    /// convention is tried and NI uses the other, every capture webhook
    /// fails permanently and no payment is ever recorded, while the
    /// customer has been charged. So both are tried, in order:
    ///
    /// 1. Parse as a UUID and look up by intent id — the convention this
    ///    method used to assume unconditionally.
    /// 2. Fall back to `find_by_gateway_order_ref` — matches NI's own
    ///    reference, whether or not step 1's parse even succeeded (NI's
    ///    reference need not be UUID-shaped at all).
    ///
    /// Whichever path resolves is logged at INFO so the first real webhook
    /// against a live sandbox immediately reveals which convention NI uses.
    ///
    /// Both failure paths return `UnknownIntentError` (not a bare
    /// `anyhow!(...)`) so `handle_webhook` can classify "neither lookup
    /// resolved" as `WebhookError::Rejected` — see that type's doc comment
    /// for why. A genuine transient error from either repo call (a DB
    /// timeout, not "no such row") propagates via `?` unmodified and is
    /// therefore classified `Internal` instead, exactly as before.
    async fn find_by_order_ref(&self, gateway_order_ref: &str) -> anyhow::Result<PaymentIntent> {
        if let Ok(intent_id) = gateway_order_ref.parse::<Uuid>() {
            if let Some(intent) = self.repo.find_by_id(intent_id).await? {
                tracing::info!(
                    intent_id = %intent_id,
                    "find_by_order_ref: resolved via intent-id convention (webhook orderReference == our merchant_order_reference)",
                );
                return Ok(intent);
            }
        }

        if let Some(intent) = self.repo.find_by_gateway_order_ref(gateway_order_ref).await? {
            tracing::info!(
                intent_id = %intent.id,
                gateway_order_ref = %gateway_order_ref,
                "find_by_order_ref: resolved via gateway_order_ref fallback (webhook orderReference == NI's own reference)",
            );
            return Ok(intent);
        }

        Err(UnknownIntentError(format!(
            "webhook order reference {gateway_order_ref:?} matched neither an intent id nor a stored gateway_order_ref"
        )).into())
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

    /// Applies an `AUTHORISED` webhook — the AUTH counterpart of
    /// `apply_captured`. Funds are ring-fenced, not taken; publishes
    /// `payment.intent.authorized`, never `payment.intent.captured`. See
    /// `network_international.rs::parse_webhook_body` for why these are now
    /// two distinct `WebhookEvent` variants rather than one.
    async fn apply_authorized(&self, gateway_order_ref: &str, gateway_payment_ref: &str) -> anyhow::Result<()> {
        // Idempotency: a replay of the same payment reference that's already
        // Authorized — or has since moved further along to Captured via
        // `capture_intent` (which never changes gateway_payment_ref, see
        // `PaymentIntent::capture_authorized`) — is a no-op. Falling through
        // to `intent.authorize()` for an already-Captured intent would
        // otherwise error (Captured is not an authorizable source state),
        // which would misclassify a harmless out-of-order redelivery as a
        // permanent Rejected/Internal failure.
        if let Some(existing) = self.repo.find_by_gateway_payment_ref(gateway_payment_ref).await? {
            if matches!(existing.status, PaymentIntentStatus::Authorized | PaymentIntentStatus::Captured) {
                return Ok(());
            }
        }

        let mut intent = self.find_by_order_ref(gateway_order_ref).await?;
        intent.authorize(gateway_payment_ref.to_string())
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        self.repo.save(&intent).await?;

        let evt = Event::new(
            "logisticos/payments",
            "payment.intent.authorized",
            intent.tenant_id,
            PaymentIntentAuthorized {
                intent_id: intent.id,
                purpose: intent.purpose.clone(),
                reference_type: intent.reference_type.clone(),
                reference_id: intent.reference_id,
                amount_cents: intent.amount_cents,
                currency: intent.currency.clone(),
            },
        );
        self.kafka.publish_event(topics::PAYMENT_INTENT_AUTHORIZED, &evt).await?;
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
        // `Refunding` is allowed through as well as `Captured`: it may be an
        // abandoned claim from a process that died mid-refund. The atomic claim
        // below is the real guard -- it only succeeds for a `Captured` row or a
        // `Refunding` one whose lease has expired, so a refund still genuinely
        // in flight is still rejected.
        if !matches!(intent.status, PaymentIntentStatus::Captured | PaymentIntentStatus::Refunding) {
            anyhow::bail!("Only a captured intent can be refunded");
        }
        let gateway_payment_ref = intent.gateway_payment_ref.clone()
            .ok_or_else(|| anyhow::anyhow!("intent {intent_id} has no captured payment to refund"))?;

        if !self.repo.claim_for_refund(intent_id).await? {
            anyhow::bail!("intent {intent_id} is already being refunded (lost the claim race)");
        }

        // Winning the claim means this call now owns the refund, whether the
        // row was `Captured` or an abandoned `Refunding` we just reclaimed.
        // Normalise the snapshot so both outcomes below behave identically:
        // `intent.refund()` requires `Captured`, and the failure branch writes
        // this same snapshot back to release the claim.
        intent.status = PaymentIntentStatus::Captured;

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
                intent.updated_at = chrono::Utc::now();
                if let Err(save_err) = self.repo.save(&intent).await {
                    tracing::error!(
                        intent_id = %intent_id,
                        gateway_error = %e,
                        revert_error = %save_err,
                        "refund: gateway call failed AND reverting the 'refunding' claim also failed \
                         — the claim lease makes it reclaimable rather than stranded",
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

    /// Captures funds previously ring-fenced by `authorize()` — the "a
    /// courier accepted the order" half of OmniDeliv's prepaid checkout.
    ///
    /// Mirrors `refund()`'s structure: a cheap local status guard first
    /// (zero repo/gateway calls for an obviously-wrong intent), then
    /// `repo.claim_for_capture` — an atomic `authorized -> captured` DB
    /// claim — as the actual concurrency guard, exactly the way
    /// `claim_for_refund` is. Only the caller whose claim affects a row may
    /// call the gateway; the loser bails before ever touching it (see
    /// `capture_intent_does_not_call_the_gateway_when_another_caller_already_holds_the_claim`).
    ///
    /// Unlike `refund()`, there is no separate intermediate "claimed but not
    /// yet resolved" status here — the claim writes directly to the target
    /// terminal status (`captured`), and a definite gateway failure reverts
    /// it back to `authorized` (same revert shape as `refund()`'s failure
    /// branch) so the call is safely retryable. This is a deliberately
    /// narrower guarantee than `refund()`'s lease-based reclaim: a process
    /// that crashes AFTER winning the claim but BEFORE the gateway responds
    /// leaves the row at `captured` with no sweep to reconcile it (there is
    /// no `sweep_pending_captures` in this pass). That is a known, narrow
    /// gap — not a silently-accepted one — left for the caller
    /// (`POST /v1/internal/payments/intents/:id/capture`) and its own
    /// retry/alerting story to close, since nothing in this task's scope
    /// asked for a capture-retry sweep.
    pub async fn capture_intent(&self, intent_id: Uuid) -> anyhow::Result<()> {
        let mut intent = self.repo.find_by_id(intent_id).await?
            .ok_or_else(|| anyhow::anyhow!("no payment_intent {intent_id}"))?;
        if intent.status != PaymentIntentStatus::Authorized {
            anyhow::bail!("Only an authorized intent can be captured");
        }
        let gateway_order_ref = intent.gateway_order_ref.clone()
            .ok_or_else(|| anyhow::anyhow!("intent {intent_id} has no gateway order reference"))?;
        let gateway_payment_ref = intent.gateway_payment_ref.clone()
            .ok_or_else(|| anyhow::anyhow!("intent {intent_id} has no authorized payment to capture"))?;

        if !self.repo.claim_for_capture(intent_id).await? {
            anyhow::bail!("intent {intent_id} is already being captured or voided (lost the claim race)");
        }

        // `intent` is the pre-claim, still-`Authorized` snapshot (never
        // mutated on the failure branch below) — same trick `refund()` uses.
        match self.gateway.capture(&gateway_order_ref, &gateway_payment_ref, intent.amount_cents).await {
            Ok(capture_ref) => {
                tracing::info!(
                    intent_id = %intent_id,
                    capture_ref = %capture_ref,
                    "capture_intent: gateway capture succeeded",
                );
                intent.capture_authorized().map_err(|e| anyhow::anyhow!("{e}"))?;
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
            Err(e) => {
                // Revert the claim: write the still-`Authorized` snapshot
                // back over the DB's `captured` row so the intent is
                // retryable rather than stranded claiming money that was
                // never actually taken.
                intent.updated_at = chrono::Utc::now();
                if let Err(save_err) = self.repo.save(&intent).await {
                    tracing::error!(
                        intent_id = %intent_id,
                        gateway_error = %e,
                        revert_error = %save_err,
                        "capture_intent: gateway call failed AND reverting the claim also failed \
                         — the intent is now stranded at 'captured' in the DB despite the gateway \
                         call failing; requires manual investigation",
                    );
                }
                Err(e)
            }
        }
    }

    /// Releases an authorization hold that was never captured — the
    /// "no courier accepted this order" half of OmniDeliv's prepaid
    /// checkout. Structure mirrors `capture_intent` exactly (same claim
    /// mechanism, same revert-on-failure shape), with one addition specific
    /// to void: on a definite gateway failure, this logs at `tracing::error!`
    /// (not `warn!`) with an explicit callout that funds are STILL
    /// ring-fenced on the customer's card — this is the money-safety-critical
    /// direction (a failed capture just means "we didn't get paid"; a failed
    /// void means "the customer's money is still held and we don't yet have
    /// it back to them"). The claim is reverted back to `Authorized` (not
    /// left stuck), so the error is loud (logged + propagated to the caller,
    /// never swallowed) AND recoverable (a retry of this same call is safe
    /// and will attempt the gateway void again).
    ///
    /// See `NetworkInternationalGateway::void`'s doc comment: the wire-level
    /// endpoint this calls is NOT confirmed against NI's docs (only "void a
    /// capture" is confirmed; reversing an authorization that was never
    /// captured is a best-reading extrapolation). A failure here is
    /// therefore not necessarily "NI said no" — it may mean the endpoint
    /// itself is wrong, which is exactly why it must never fail silently.
    pub async fn void_intent(&self, intent_id: Uuid) -> anyhow::Result<()> {
        let mut intent = self.repo.find_by_id(intent_id).await?
            .ok_or_else(|| anyhow::anyhow!("no payment_intent {intent_id}"))?;
        if intent.status != PaymentIntentStatus::Authorized {
            anyhow::bail!("Only an authorized intent can be voided");
        }
        let gateway_order_ref = intent.gateway_order_ref.clone()
            .ok_or_else(|| anyhow::anyhow!("intent {intent_id} has no gateway order reference"))?;
        let gateway_payment_ref = intent.gateway_payment_ref.clone()
            .ok_or_else(|| anyhow::anyhow!("intent {intent_id} has no authorized payment to void"))?;

        if !self.repo.claim_for_void(intent_id).await? {
            anyhow::bail!("intent {intent_id} is already being captured or voided (lost the claim race)");
        }

        match self.gateway.void(&gateway_order_ref, &gateway_payment_ref).await {
            Ok(()) => {
                intent.void().map_err(|e| anyhow::anyhow!("{e}"))?;
                self.repo.save(&intent).await?;
                Ok(())
            }
            Err(e) => {
                intent.updated_at = chrono::Utc::now();
                if let Err(save_err) = self.repo.save(&intent).await {
                    tracing::error!(
                        intent_id = %intent_id,
                        gateway_error = %e,
                        revert_error = %save_err,
                        "void_intent: gateway void call failed AND reverting the claim also failed \
                         — the intent is now stranded at 'voided' in the DB despite the gateway call \
                         failing, and funds remain ring-fenced on the customer's card with NO \
                         automatic recovery path. Requires IMMEDIATE manual investigation: check the \
                         payment directly against NI and correct the DB row's status by hand if \
                         necessary.",
                    );
                    return Err(e);
                }
                tracing::error!(
                    intent_id = %intent_id,
                    gateway_error = %e,
                    "void_intent: gateway void call failed — funds remain ring-fenced (authorized, \
                     not released) on the customer's card. The claim was reverted, so the intent is \
                     back at 'authorized' and this call can be safely retried. NI's docs describe an \
                     unreleased authorization hold as eventually expiring on the issuing bank's own \
                     schedule, but that is not a substitute for an explicit retry or ops follow-up.",
                );
                Err(e)
            }
        }
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

    /// Must match the lease in `payment_intent_repo.rs`'s claim SQL.
    const LEASE_MINUTES: i64 = 15;
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

        async fn find_by_gateway_order_ref(&self, gateway_order_ref: &str) -> anyhow::Result<Option<PaymentIntent>> {
            Ok(self.intents.lock().unwrap().values()
                .find(|i| i.gateway_order_ref.as_deref() == Some(gateway_order_ref))
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
            // Mirrors the SQL: a `Captured` row, or a `Refunding` one whose
            // claim lease has expired (an abandoned claim).
            let lease_cutoff = Utc::now() - chrono::Duration::minutes(LEASE_MINUTES);
            match intents.get_mut(&id) {
                Some(intent)
                    if intent.status == PaymentIntentStatus::Captured
                        || (intent.status == PaymentIntentStatus::Refunding
                            && intent.updated_at < lease_cutoff) =>
                {
                    intent.status = PaymentIntentStatus::Refunding;
                    intent.updated_at = Utc::now();
                    Ok(true)
                }
                _ => Ok(false),
            }
        }

        async fn list_pending_refunds(&self) -> anyhow::Result<Vec<PaymentIntent>> {
            Ok(self.intents.lock().unwrap().values()
                // Mirrors the SQL: a refund is owed, and the row is either
                // captured or holding an expired claim lease.
                // Mirrors the SQL: a refund is owed, and the row is either
                // captured or holding an expired claim lease.
                .filter(|i| i.refund_requested_at.is_some()
                    && (i.status == PaymentIntentStatus::Captured
                        || (i.status == PaymentIntentStatus::Refunding
                            && i.updated_at < Utc::now() - chrono::Duration::minutes(LEASE_MINUTES))))
                .cloned()
                .collect())
        }

        async fn claim_for_capture(&self, id: Uuid) -> anyhow::Result<bool> {
            let mut intents = self.intents.lock().unwrap();
            match intents.get_mut(&id) {
                Some(intent) if intent.status == PaymentIntentStatus::Authorized => {
                    intent.status = PaymentIntentStatus::Captured;
                    intent.updated_at = Utc::now();
                    Ok(true)
                }
                _ => Ok(false),
            }
        }

        async fn claim_for_void(&self, id: Uuid) -> anyhow::Result<bool> {
            let mut intents = self.intents.lock().unwrap();
            match intents.get_mut(&id) {
                Some(intent) if intent.status == PaymentIntentStatus::Authorized => {
                    intent.status = PaymentIntentStatus::Voided;
                    intent.updated_at = Utc::now();
                    Ok(true)
                }
                _ => Ok(false),
            }
        }
    }

    // ── Fake PaymentGateway ──────────────────────────────────────────────────

    enum FakeWebhook {
        Captured { order_ref: String, payment_ref: String },
        Authorized { order_ref: String, payment_ref: String },
        Failed { order_ref: String },
    }

    struct FakeGateway {
        checkout_url: String,
        webhook: Mutex<Option<FakeWebhook>>,
        refund_should_fail: bool,
        refund_calls: Mutex<u32>,
        capture_should_fail: bool,
        capture_calls: Mutex<u32>,
        void_should_fail: bool,
        void_calls: Mutex<u32>,
    }

    impl FakeGateway {
        fn new() -> Self {
            Self {
                checkout_url: "https://pay.example/checkout/abc".into(),
                webhook: Mutex::new(None),
                refund_should_fail: false,
                refund_calls: Mutex::new(0),
                capture_should_fail: false,
                capture_calls: Mutex::new(0),
                void_should_fail: false,
                void_calls: Mutex::new(0),
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

        fn with_capture_failure(mut self) -> Self {
            self.capture_should_fail = true;
            self
        }

        fn with_void_failure(mut self) -> Self {
            self.void_should_fail = true;
            self
        }

        fn refund_calls(&self) -> u32 {
            *self.refund_calls.lock().unwrap()
        }

        fn capture_calls(&self) -> u32 {
            *self.capture_calls.lock().unwrap()
        }

        fn void_calls(&self) -> u32 {
            *self.void_calls.lock().unwrap()
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
                Some(FakeWebhook::Authorized { order_ref, payment_ref }) => Ok(WebhookEvent::Authorized {
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

        async fn capture(&self, _gateway_order_ref: &str, _gateway_payment_ref: &str, _amount_cents: i64) -> anyhow::Result<String> {
            *self.capture_calls.lock().unwrap() += 1;
            if self.capture_should_fail {
                anyhow::bail!("gateway capture failed");
            }
            Ok("ni-capture-ref".into())
        }

        async fn void(&self, _gateway_order_ref: &str, _gateway_payment_ref: &str) -> anyhow::Result<()> {
            *self.void_calls.lock().unwrap() += 1;
            if self.void_should_fail {
                anyhow::bail!("gateway void failed");
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

        async fn capture(&self, _gateway_order_ref: &str, _gateway_payment_ref: &str, _amount_cents: i64) -> anyhow::Result<String> {
            unreachable!("not exercised by sweep_pending_refunds tests")
        }

        async fn void(&self, _gateway_order_ref: &str, _gateway_payment_ref: &str) -> anyhow::Result<()> {
            unreachable!("not exercised by sweep_pending_refunds tests")
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
            action: PaymentAction::Sale,
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

    // ── find_by_order_ref: two-way lookup (highest-consequence unverified
    // assumption in the NI integration) ─────────────────────────────────────
    //
    // `find_by_order_ref` used to assume unconditionally that NI's webhook
    // `orderReference` echoes back our own `merchant_order_reference` (our
    // intent id, passed to `create_session`). That's never been verified
    // against a live NI sandbox. If NI instead echoes its own `reference`
    // (the value `create_session` stores as `gateway_order_ref` on the
    // intent), every capture webhook would fail permanently while the
    // customer had genuinely been charged. These three tests prove both
    // conventions now resolve, and that "matches neither" still correctly
    // classifies as Rejected, not Internal.

    #[tokio::test]
    async fn handle_webhook_resolves_when_order_ref_is_our_intent_id() {
        // The convention this code assumed before the fix — must keep
        // working. Stored gateway_order_ref deliberately differs from the
        // intent id so this only passes via the id lookup, not the fallback.
        let repo = Arc::new(FakeRepo::default());
        let tenant_id = Uuid::new_v4();
        let intent = make_intent(tenant_id).with_gateway_order_ref("ni-ref-not-the-intent-id".into());
        let intent_id = intent.id;
        repo.seed(intent);

        let gateway = Arc::new(FakeGateway::new().with_webhook(FakeWebhook::Captured {
            order_ref: intent_id.to_string(),
            payment_ref: "ni-txn-by-intent-id".into(),
        }));
        let svc = service(repo.clone(), gateway);
        let headers = reqwest::header::HeaderMap::new();

        svc.handle_webhook(&headers, b"{}").await.expect("must resolve via the intent-id convention");

        let stored = repo.get(intent_id);
        assert_eq!(stored.status, PaymentIntentStatus::Captured);
        assert_eq!(stored.gateway_payment_ref.as_deref(), Some("ni-txn-by-intent-id"));
    }

    #[tokio::test]
    async fn handle_webhook_resolves_when_order_ref_is_nis_own_reference_not_our_intent_id() {
        // THE test that proves the risk is closed: NI's webhook
        // `orderReference` is a non-UUID string matching only the stored
        // `gateway_order_ref` — never the intent's own UUID. Before this
        // fix, `gateway_order_ref.parse::<Uuid>()` would fail and the
        // webhook would be rejected as an unknown intent (`UnknownIntentError`)
        // even though the intent genuinely exists and was genuinely captured
        // — i.e. every real NI capture webhook would have failed and no
        // payment would ever have been recorded, silently, forever.
        let repo = Arc::new(FakeRepo::default());
        let tenant_id = Uuid::new_v4();
        let intent = make_intent(tenant_id).with_gateway_order_ref("ni-order-ref-xyz-789".into());
        let intent_id = intent.id;
        repo.seed(intent);

        let gateway = Arc::new(FakeGateway::new().with_webhook(FakeWebhook::Captured {
            order_ref: "ni-order-ref-xyz-789".into(), // NOT the intent id — NI's own reference
            payment_ref: "ni-txn-by-gateway-ref".into(),
        }));
        let svc = service(repo.clone(), gateway);
        let headers = reqwest::header::HeaderMap::new();

        svc.handle_webhook(&headers, b"{}").await
            .expect("must resolve via the gateway_order_ref fallback, not just the intent-id convention");

        let stored = repo.get(intent_id);
        assert_eq!(stored.status, PaymentIntentStatus::Captured);
        assert_eq!(stored.gateway_payment_ref.as_deref(), Some("ni-txn-by-gateway-ref"));
    }

    #[tokio::test]
    async fn handle_webhook_order_ref_matching_neither_convention_is_rejected_not_internal() {
        // A seeded intent exists, but the webhook's reference matches
        // neither its id nor its stored gateway_order_ref — must still be a
        // permanent Rejected (4xx, NI stops retrying), not Internal (5xx, NI
        // retries forever against a reference that will never resolve).
        let repo = Arc::new(FakeRepo::default());
        let tenant_id = Uuid::new_v4();
        let intent = make_intent(tenant_id).with_gateway_order_ref("ni-order-ref-real".into());
        repo.seed(intent);

        let gateway = Arc::new(FakeGateway::new().with_webhook(FakeWebhook::Captured {
            order_ref: "totally-unrelated-reference".into(),
            payment_ref: "ni-txn-orphan".into(),
        }));
        let svc = service(repo, gateway);
        let headers = reqwest::header::HeaderMap::new();

        let err = svc.handle_webhook(&headers, b"{}").await
            .expect_err("a reference matching neither convention must error");

        assert!(
            matches!(err, WebhookError::Rejected(_)),
            "must be Rejected (permanent), got {err:?}"
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

    /// A process that dies between claiming a refund and completing it must
    /// not strand the customer's money. The claim is a lease, so the retry
    /// sweep reclaims it; without that, the row sits in `refunding` forever,
    /// invisible to the sweep, with the customer still charged.
    #[tokio::test]
    async fn an_abandoned_refund_claim_is_reclaimed_once_its_lease_expires() {
        let repo = Arc::new(FakeRepo::default());
        let tenant_id = Uuid::new_v4();
        let mut intent = make_intent(tenant_id);
        intent.capture("ni-txn-abandoned".into()).unwrap();
        intent.status = PaymentIntentStatus::Refunding;
        intent.refund_requested_at = Some(Utc::now() - chrono::Duration::hours(1));
        intent.updated_at = Utc::now() - chrono::Duration::minutes(LEASE_MINUTES + 5);
        let intent_id = intent.id;
        repo.seed(intent);

        let gateway = Arc::new(FakeGateway::new());
        let svc = service(repo.clone(), gateway.clone());

        let refunded = svc.sweep_pending_refunds().await.expect("sweep must not error");

        assert_eq!(refunded, 1, "an abandoned claim must be retried, not stranded forever");
        assert_eq!(repo.get(intent_id).status, PaymentIntentStatus::Refunded);
        assert_eq!(gateway.refund_calls(), 1, "the reclaimed refund must actually reach the gateway");
    }

    /// The other half: a claim still inside its lease is a refund genuinely in
    /// flight, and must not be duplicated.
    #[tokio::test]
    async fn a_fresh_refund_claim_is_not_reclaimed() {
        let repo = Arc::new(FakeRepo::default());
        let tenant_id = Uuid::new_v4();
        let mut intent = make_intent(tenant_id);
        intent.capture("ni-txn-inflight".into()).unwrap();
        intent.status = PaymentIntentStatus::Refunding;
        intent.refund_requested_at = Some(Utc::now());
        intent.updated_at = Utc::now();
        let intent_id = intent.id;
        repo.seed(intent);

        let gateway = Arc::new(FakeGateway::new());
        let svc = service(repo.clone(), gateway.clone());

        let refunded = svc.sweep_pending_refunds().await.expect("sweep must not error");

        assert_eq!(refunded, 0, "a refund still in flight must not be retried");
        assert_eq!(gateway.refund_calls(), 0, "the gateway must not be called twice for one refund");
        assert_eq!(repo.get(intent_id).status, PaymentIntentStatus::Refunding);
    }


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

    // ── authorize-then-capture, with void ───────────────────────────────────
    //
    // OmniDeliv's prepaid-checkout foundation: ring-fence funds on order
    // placement (Authorize), then either capture once a courier accepts or
    // void if none does. The highest-risk item here is the CAPTURED/AUTHORISED
    // webhook split (previously conflated into one WebhookEvent::Captured) —
    // see the dedicated tests below and in network_international.rs.

    fn make_authorized_intent(tenant_id: Uuid) -> PaymentIntent {
        let mut intent = make_intent(tenant_id).with_gateway_order_ref("ord-auth-ref".into());
        intent.authorize("ni-auth-ref".into()).expect("authorize from Pending must succeed");
        intent
    }

    #[tokio::test]
    async fn handle_webhook_authorised_transitions_to_authorized_not_captured() {
        // THE regression test for the CAPTURED/AUTHORISED split at the
        // service layer (network_international.rs has the adapter-layer
        // proof). An AUTHORISED webhook must land the intent at Authorized
        // — money is only ring-fenced — never at Captured.
        let repo = Arc::new(FakeRepo::default());
        let tenant_id = Uuid::new_v4();
        let intent = make_intent(tenant_id).with_gateway_order_ref("order-ref-auth".into());
        let intent_id = intent.id;
        repo.seed(intent);

        let gateway = Arc::new(FakeGateway::new().with_webhook(FakeWebhook::Authorized {
            order_ref: intent_id.to_string(),
            payment_ref: "ni-auth-1".into(),
        }));
        let svc = service(repo.clone(), gateway);
        let headers = reqwest::header::HeaderMap::new();

        svc.handle_webhook(&headers, b"{}").await.expect("authorised webhook must apply");

        let stored = repo.get(intent_id);
        assert_eq!(stored.status, PaymentIntentStatus::Authorized, "must be Authorized, NOT Captured");
        assert_eq!(stored.gateway_payment_ref.as_deref(), Some("ni-auth-1"));
    }

    #[tokio::test]
    async fn handle_webhook_captured_still_transitions_to_captured_not_authorized() {
        // Regression guard on the split, from the other direction: a real
        // SALE-path CAPTURED webhook must be entirely unaffected by adding
        // the AUTHORISED branch.
        let repo = Arc::new(FakeRepo::default());
        let tenant_id = Uuid::new_v4();
        let intent = make_intent(tenant_id).with_gateway_order_ref("order-ref-cap".into());
        let intent_id = intent.id;
        repo.seed(intent);

        let gateway = Arc::new(FakeGateway::new().with_webhook(FakeWebhook::Captured {
            order_ref: intent_id.to_string(),
            payment_ref: "ni-cap-1".into(),
        }));
        let svc = service(repo.clone(), gateway);
        let headers = reqwest::header::HeaderMap::new();

        svc.handle_webhook(&headers, b"{}").await.expect("captured webhook must apply");

        let stored = repo.get(intent_id);
        assert_eq!(stored.status, PaymentIntentStatus::Captured, "must remain Captured, not Authorized");
    }

    #[tokio::test]
    async fn handle_webhook_authorised_replay_after_capture_intent_already_ran_is_a_no_op() {
        // Out-of-order redelivery: capture_intent already advanced the
        // intent to Captured (same gateway_payment_ref, since
        // capture_authorized() never changes it — see that method's doc
        // comment) by the time a late AUTHORISED webhook arrives. Must be a
        // silent no-op, not an error (falling through to intent.authorize()
        // on an already-Captured intent would otherwise reject it).
        let repo = Arc::new(FakeRepo::default());
        let tenant_id = Uuid::new_v4();
        let mut intent = make_intent(tenant_id).with_gateway_order_ref("order-ref-late".into());
        intent.authorize("ni-late-1".into()).unwrap();
        intent.capture_authorized().unwrap();
        assert_eq!(intent.status, PaymentIntentStatus::Captured);
        let intent_id = intent.id;
        repo.seed(intent);

        let gateway = Arc::new(FakeGateway::new().with_webhook(FakeWebhook::Authorized {
            order_ref: intent_id.to_string(),
            payment_ref: "ni-late-1".into(),
        }));
        let svc = service(repo.clone(), gateway);
        let headers = reqwest::header::HeaderMap::new();

        svc.handle_webhook(&headers, b"{}").await.expect("late redelivery must be a no-op, not an error");

        assert_eq!(repo.get(intent_id).status, PaymentIntentStatus::Captured, "must remain Captured");
    }

    // ── capture_intent ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn capture_intent_on_an_authorized_intent_captures_and_publishes_the_captured_event() {
        let repo = Arc::new(FakeRepo::default());
        let tenant_id = Uuid::new_v4();
        let intent = make_authorized_intent(tenant_id);
        let intent_id = intent.id;
        repo.seed(intent);

        let gateway = Arc::new(FakeGateway::new());
        let svc = service(repo.clone(), gateway.clone());

        svc.capture_intent(intent_id).await.expect("capture of an authorized intent must succeed");

        assert_eq!(gateway.capture_calls(), 1, "gateway must be called exactly once");
        let stored = repo.get(intent_id);
        assert_eq!(stored.status, PaymentIntentStatus::Captured);
        // The authorization's payment reference must survive capture unchanged.
        assert_eq!(stored.gateway_payment_ref.as_deref(), Some("ni-auth-ref"));
    }

    #[tokio::test]
    async fn capture_intent_on_a_non_authorized_intent_is_rejected_without_calling_the_gateway() {
        let repo = Arc::new(FakeRepo::default());
        let tenant_id = Uuid::new_v4();
        let intent = make_intent(tenant_id); // still Created — never authorized
        let intent_id = intent.id;
        repo.seed(intent);

        let gateway = Arc::new(FakeGateway::new());
        let svc = service(repo.clone(), gateway.clone());

        let err = svc.capture_intent(intent_id).await.expect_err("must reject a non-authorized intent");
        assert!(err.to_string().contains("Only an authorized intent can be captured"));
        assert_eq!(gateway.capture_calls(), 0, "gateway must never be called for a non-authorized intent");
    }

    #[tokio::test]
    async fn capture_intent_reverts_to_authorized_when_the_gateway_call_fails() {
        let repo = Arc::new(FakeRepo::default());
        let tenant_id = Uuid::new_v4();
        let intent = make_authorized_intent(tenant_id);
        let intent_id = intent.id;
        repo.seed(intent);

        let gateway = Arc::new(FakeGateway::new().with_capture_failure());
        let svc = service(repo.clone(), gateway.clone());

        let err = svc.capture_intent(intent_id).await.expect_err("gateway failure must propagate");
        assert!(err.to_string().contains("gateway capture failed"));

        let stored = repo.get(intent_id);
        assert_eq!(stored.status, PaymentIntentStatus::Authorized, "must revert to Authorized, not stay stuck at captured");
    }

    #[tokio::test]
    async fn concurrent_capture_the_second_caller_loses_the_claim_and_never_reaches_the_gateway() {
        let repo = Arc::new(FakeRepo::default());
        let tenant_id = Uuid::new_v4();
        let intent = make_authorized_intent(tenant_id);
        let intent_id = intent.id;
        repo.seed(intent);

        // Simulate the other (winning) caller claiming first, directly
        // against the repo — same technique
        // `refund_does_not_call_the_gateway_when_another_caller_already_holds_the_claim`
        // uses above.
        assert!(repo.claim_for_capture(intent_id).await.unwrap(), "setup: the winning claim must succeed");

        let gateway = Arc::new(FakeGateway::new());
        let svc = service(repo.clone(), gateway.clone());

        svc.capture_intent(intent_id).await.expect_err("the loser of the claim race must not proceed");
        assert_eq!(gateway.capture_calls(), 0, "gateway must never be called by the caller that lost the claim race");

        let stored = repo.get(intent_id);
        assert_eq!(stored.status, PaymentIntentStatus::Captured, "still owned by the winning (simulated) caller");
    }

    // ── void_intent ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn void_intent_on_an_authorized_intent_releases_the_hold() {
        let repo = Arc::new(FakeRepo::default());
        let tenant_id = Uuid::new_v4();
        let intent = make_authorized_intent(tenant_id);
        let intent_id = intent.id;
        repo.seed(intent);

        let gateway = Arc::new(FakeGateway::new());
        let svc = service(repo.clone(), gateway.clone());

        svc.void_intent(intent_id).await.expect("void of an authorized intent must succeed");

        assert_eq!(gateway.void_calls(), 1, "gateway must be called exactly once");
        assert_eq!(repo.get(intent_id).status, PaymentIntentStatus::Voided);
    }

    #[tokio::test]
    async fn void_intent_on_a_non_authorized_intent_is_rejected_without_calling_the_gateway() {
        let repo = Arc::new(FakeRepo::default());
        let tenant_id = Uuid::new_v4();
        let intent = make_intent(tenant_id); // still Created
        let intent_id = intent.id;
        repo.seed(intent);

        let gateway = Arc::new(FakeGateway::new());
        let svc = service(repo.clone(), gateway.clone());

        let err = svc.void_intent(intent_id).await.expect_err("must reject a non-authorized intent");
        assert!(err.to_string().contains("Only an authorized intent can be voided"));
        assert_eq!(gateway.void_calls(), 0);
    }

    // Money-safety regression: a failed void must be LOUD (the error
    // propagates, never swallowed) and RECOVERABLE (the claim reverts back
    // to Authorized so a retry is safe) — see `void_intent`'s doc comment.
    // An auth we failed to release is money still ring-fenced on a
    // customer's card.
    #[tokio::test]
    async fn void_intent_reverts_to_authorized_and_propagates_the_error_when_the_gateway_call_fails() {
        let repo = Arc::new(FakeRepo::default());
        let tenant_id = Uuid::new_v4();
        let intent = make_authorized_intent(tenant_id);
        let intent_id = intent.id;
        repo.seed(intent);

        let gateway = Arc::new(FakeGateway::new().with_void_failure());
        let svc = service(repo.clone(), gateway.clone());

        let err = svc.void_intent(intent_id).await.expect_err("gateway void failure must propagate, never be swallowed");
        assert!(err.to_string().contains("gateway void failed"));

        let stored = repo.get(intent_id);
        assert_eq!(
            stored.status, PaymentIntentStatus::Authorized,
            "must revert to Authorized (recoverable — a retry can safely call void_intent again), \
             not be left stuck claiming a void that never actually happened"
        );
    }

    #[tokio::test]
    async fn a_voided_intent_can_never_be_captured_or_refunded() {
        let repo = Arc::new(FakeRepo::default());
        let tenant_id = Uuid::new_v4();
        let intent = make_authorized_intent(tenant_id);
        let intent_id = intent.id;
        repo.seed(intent);

        let gateway = Arc::new(FakeGateway::new());
        let svc = service(repo.clone(), gateway.clone());

        svc.void_intent(intent_id).await.expect("void must succeed");
        assert_eq!(repo.get(intent_id).status, PaymentIntentStatus::Voided);

        let capture_err = svc.capture_intent(intent_id).await.expect_err("a voided intent must never be capturable");
        assert!(capture_err.to_string().contains("Only an authorized intent can be captured"));

        let refund_err = svc.refund(intent_id).await.expect_err("a voided intent must never be refundable");
        assert!(refund_err.to_string().contains("Only a captured intent can be refunded"));

        assert_eq!(gateway.capture_calls(), 0);
        assert_eq!(gateway.refund_calls(), 0);
        assert_eq!(repo.get(intent_id).status, PaymentIntentStatus::Voided, "must remain Voided throughout");
    }
}
