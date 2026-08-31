//! PaymentIntent — a gateway-agnostic record of "charge this much, for this
//! reason, tied to this thing." One row exists per attempted online charge,
//! regardless of which gateway or which product surface initiated it.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentIntentStatus {
    Created,
    Pending,
    Captured,
    Failed,
    Refunded,
    Expired,
    /// Atomically-claimed intermediate status between `Captured` and
    /// `Refunded`: the DB-level `UPDATE ... WHERE status = 'captured'` claim
    /// in `PaymentIntentRepository::claim_for_refund` is what actually
    /// serializes concurrent refund attempts (the cancellation consumer and
    /// the pending-refund sweep can genuinely race for the same intent) —
    /// only the caller whose claim affects a row may call the gateway. On
    /// gateway success it advances to `Refunded`; on gateway failure it is
    /// reverted to `Captured` (never left stuck here) so the sweep retries it.
    Refunding,
    /// Funds are ring-fenced on the customer's card (an NI `AUTH` order, or
    /// an `AUTHORISED` webhook) but not yet taken. Reached only via
    /// `authorize()`, never via `capture()`/`with_gateway_order_ref` — the
    /// immediate-SALE path never visits this state. From here the intent
    /// either advances to `Captured` (`capture_authorized()`) or terminates
    /// at `Voided` (`void()`). This is OmniDeliv's prepaid-checkout
    /// foundation: ring-fence on order placement, capture once a courier
    /// accepts, void if none does — so the customer is never charged for an
    /// order nobody fulfilled.
    Authorized,
    /// Terminal: an authorization hold that was released without ever being
    /// captured (`void()`, from `Authorized` only). Like `Refunded`, a
    /// `Voided` intent can never be captured or refunded again — see the
    /// guards on `capture_authorized()` and `refund()`.
    Voided,
}

impl PaymentIntentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Created    => "created",
            Self::Pending    => "pending",
            Self::Captured   => "captured",
            Self::Failed     => "failed",
            Self::Refunded   => "refunded",
            Self::Expired    => "expired",
            Self::Refunding  => "refunding",
            Self::Authorized => "authorized",
            Self::Voided     => "voided",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "created"    => Some(Self::Created),
            "pending"    => Some(Self::Pending),
            "captured"   => Some(Self::Captured),
            "failed"     => Some(Self::Failed),
            "refunded"   => Some(Self::Refunded),
            "expired"    => Some(Self::Expired),
            "refunding"  => Some(Self::Refunding),
            "authorized" => Some(Self::Authorized),
            "voided"     => Some(Self::Voided),
            _            => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentIntent {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub purpose: String,
    pub reference_type: String,
    pub reference_id: Uuid,
    pub amount_cents: i64,
    pub currency: String,
    pub status: PaymentIntentStatus,
    pub gateway: String,
    pub gateway_order_ref: Option<String>,
    pub gateway_payment_ref: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    /// When a refund became owed for this intent (set durably, before the
    /// gateway call is attempted, by `PaymentIntentRepository::mark_refund_requested`
    /// — see that method's doc comment). `None` for an intent nothing has
    /// ever asked to refund. Read by `PaymentIntentService::sweep_pending_refunds`
    /// to find `Captured` intents with an outstanding, unfulfilled obligation.
    pub refund_requested_at: Option<DateTime<Utc>>,
    /// What was actually taken, when that differs from what was ring-fenced.
    ///
    /// `amount_cents` above stays the AUTHORIZED amount and is never
    /// rewritten — `refund()` and every reconciliation read it, and an
    /// authorized figure that shifted after the fact would make a hold
    /// impossible to match against its own capture.
    ///
    /// `None` on a `Captured` intent means captured in full (including every
    /// intent captured before partial capture existed). Read it as
    /// `amount_cents` — see `captured_or_full()`.
    pub captured_amount_cents: Option<i64>,
}

impl PaymentIntent {
    /// Eight arguments, because a payment intent has eight required fields at
    /// construction time. Grouping them into a builder or a params struct would
    /// move the arity rather than remove it, and would let a caller construct a
    /// half-populated intent — the opposite of what this type is for.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tenant_id: Uuid,
        purpose: impl Into<String>,
        reference_type: impl Into<String>,
        reference_id: Uuid,
        amount_cents: i64,
        currency: impl Into<String>,
        gateway: impl Into<String>,
        ttl: chrono::Duration,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            purpose: purpose.into(),
            reference_type: reference_type.into(),
            reference_id,
            amount_cents,
            currency: currency.into(),
            status: PaymentIntentStatus::Created,
            gateway: gateway.into(),
            gateway_order_ref: None,
            gateway_payment_ref: None,
            created_at: now,
            updated_at: now,
            expires_at: now + ttl,
            refund_requested_at: None,
            captured_amount_cents: None,
        }
    }

    /// Attach the gateway's own session/order reference once the hosted
    /// checkout session has been created.
    pub fn with_gateway_order_ref(mut self, gateway_order_ref: String) -> Self {
        self.gateway_order_ref = Some(gateway_order_ref);
        self.status = PaymentIntentStatus::Pending;
        self.updated_at = Utc::now();
        self
    }

    /// Business rule: only a `Created`/`Pending` intent can be captured, and
    /// capture is idempotent — replaying the same `gateway_payment_ref` on an
    /// already-`Captured` intent is a no-op, not an error, since a webhook can
    /// legitimately be delivered more than once.
    pub fn capture(&mut self, gateway_payment_ref: String) -> Result<(), &'static str> {
        match self.status {
            PaymentIntentStatus::Captured => {
                if self.gateway_payment_ref.as_deref() == Some(gateway_payment_ref.as_str()) {
                    return Ok(()); // idempotent replay
                }
                return Err("Intent already captured under a different gateway reference");
            }
            PaymentIntentStatus::Created | PaymentIntentStatus::Pending => {}
            _ => return Err("Intent is not in a capturable state"),
        }
        self.status = PaymentIntentStatus::Captured;
        self.gateway_payment_ref = Some(gateway_payment_ref);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// `Expired` is terminal, the same way `Captured`/`Refunded` already
    /// are: a late "declined" webhook arriving after our own sweep has
    /// already expired the intent must NOT be allowed to re-transition it to
    /// `Failed`. Before this guard, that catch-all `_ => {}` silently let
    /// `Expired -> Failed` through, which meant a second
    /// `payment.intent.failed` event could be published for an intent
    /// order-intake had already reacted to (see
    /// `handle_webhook_failed_on_an_already_expired_intent_is_rejected` in
    /// `payment_intent_service.rs` for the scenario this closes, and that
    /// test's doc comment for why rejecting — rather than a silent idempotent
    /// no-op — is the right shape here: a no-op would still let the caller,
    /// `PaymentIntentService::apply_failed`, walk on to unconditionally
    /// publish a duplicate event, since that method doesn't currently check
    /// whether `fail()` actually changed anything before publishing).
    ///
    /// `Authorized` gets the same treatment as `Captured`/`Refunded`/
    /// `Refunding`, for the identical money-safety reason: it means real
    /// funds are genuinely ring-fenced on the customer's card. A stale or
    /// out-of-order "declined" webhook must not be allowed to silently
    /// relabel that as `Failed` — a downstream consumer treating `Failed` as
    /// "nothing to collect, safe to cancel" would be wrong while the hold is
    /// still live. `Voided` is likewise blocked as a plain terminal state,
    /// the same way `Expired` already is.
    pub fn fail(&mut self) -> Result<(), &'static str> {
        match self.status {
            PaymentIntentStatus::Failed => return Ok(()), // idempotent
            PaymentIntentStatus::Captured
            | PaymentIntentStatus::Refunded
            | PaymentIntentStatus::Refunding
            | PaymentIntentStatus::Authorized => {
                return Err("Cannot fail an intent that already captured or authorized");
            }
            PaymentIntentStatus::Expired => {
                return Err("Cannot fail an intent that has already expired");
            }
            PaymentIntentStatus::Voided => {
                return Err("Cannot fail an intent that has already been voided");
            }
            _ => {}
        }
        self.status = PaymentIntentStatus::Failed;
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn expire(&mut self) -> Result<(), &'static str> {
        match self.status {
            PaymentIntentStatus::Created | PaymentIntentStatus::Pending => {
                self.status = PaymentIntentStatus::Expired;
                self.updated_at = Utc::now();
                Ok(())
            }
            PaymentIntentStatus::Expired => Ok(()), // idempotent
            _ => Err("Cannot expire an intent that already reached a final state"),
        }
    }

    pub fn refund(&mut self) -> Result<(), &'static str> {
        if self.status != PaymentIntentStatus::Captured {
            return Err("Only a captured intent can be refunded");
        }
        self.status = PaymentIntentStatus::Refunded;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// The `AUTH` counterpart of `capture()`: places a hold instead of
    /// taking the money immediately. Business rule mirrors `capture()`
    /// exactly — only a `Created`/`Pending` intent can be authorized, and
    /// authorization is idempotent for the same reason: an `AUTHORISED`
    /// webhook can legitimately be delivered more than once.
    pub fn authorize(&mut self, gateway_payment_ref: String) -> Result<(), &'static str> {
        match self.status {
            PaymentIntentStatus::Authorized => {
                if self.gateway_payment_ref.as_deref() == Some(gateway_payment_ref.as_str()) {
                    return Ok(()); // idempotent replay
                }
                return Err("Intent already authorized under a different gateway reference");
            }
            PaymentIntentStatus::Created | PaymentIntentStatus::Pending => {}
            _ => return Err("Intent is not in an authorizable state"),
        }
        self.status = PaymentIntentStatus::Authorized;
        self.gateway_payment_ref = Some(gateway_payment_ref);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Captures funds previously ring-fenced by `authorize()`. Kept as a
    /// SIBLING to `capture()` rather than folded into it, deliberately:
    /// `capture()` is the immediate-SALE path (`Created`/`Pending` ->
    /// `Captured` directly — `Authorized` is never visited) and takes the
    /// gateway's payment reference as an argument because that reference is
    /// new information at that point. Here the payment reference was
    /// already recorded by `authorize()` and does not change on capture —
    /// it must keep identifying the same underlying NI payment so a later
    /// `refund()` still resolves correctly. Merging the two into one method
    /// would also erase a real distinction `PaymentIntentService::capture_intent`
    /// needs to preserve: a single unified "capture" that accepted both
    /// `Created`/`Pending` and `Authorized` as valid source states would
    /// silently allow "capturing" money that was never actually authorized
    /// or sold, which is exactly the ambiguity the caller must be able to
    /// rule out before ever calling the gateway. Idempotent for the same
    /// replay reason as `capture()`.
    pub fn capture_authorized(&mut self, amount_cents: i64) -> Result<(), &'static str> {
        match self.status {
            // Idempotent replay. Deliberately does NOT re-record the amount: a
            // redelivered capture carrying a different figure must not rewrite
            // what was actually taken the first time.
            PaymentIntentStatus::Captured => return Ok(()),
            PaymentIntentStatus::Authorized => {}
            _ => return Err("Only an authorized intent can be captured via capture_authorized"),
        }
        if amount_cents <= 0 {
            // A zero capture is a void wearing a capture's name. The caller has
            // to say which it meant, because they are not the same event
            // downstream and they are not the same thing to a customer.
            return Err("A capture must take a positive amount; use void() to release a hold");
        }
        if amount_cents > self.amount_cents {
            return Err("Cannot capture more than was authorized");
        }
        self.captured_amount_cents = Some(amount_cents);
        self.status = PaymentIntentStatus::Captured;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// What was taken, for an intent that was captured.
    ///
    /// Folds the pre-partial-capture `None` into the full amount so callers
    /// never have to remember that a null means "all of it".
    pub fn captured_or_full(&self) -> i64 {
        self.captured_amount_cents.unwrap_or(self.amount_cents)
    }

    /// What is left ring-fenced but not taken, after a partial capture.
    ///
    /// The gateway does not necessarily release this on its own — see the
    /// unverified `void` note in `network_international.rs`. Reconciliation
    /// needs to be able to ask.
    pub fn uncaptured_remainder_cents(&self) -> i64 {
        match self.status {
            PaymentIntentStatus::Captured => self.amount_cents - self.captured_or_full(),
            _ => 0,
        }
    }

    /// Releases a hold that was never captured — the no-courier path in
    /// OmniDeliv's prepaid checkout. Terminal: a `Voided` intent can never
    /// be captured (`capture_authorized()`) or refunded (`refund()`, which
    /// only accepts `Captured`) again. Idempotent on replay, matching every
    /// other terminal transition in this file.
    pub fn void(&mut self) -> Result<(), &'static str> {
        match self.status {
            PaymentIntentStatus::Voided => return Ok(()), // idempotent replay
            PaymentIntentStatus::Authorized => {}
            _ => return Err("Only an authorized intent can be voided"),
        }
        self.status = PaymentIntentStatus::Voided;
        self.updated_at = Utc::now();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_intent() -> PaymentIntent {
        PaymentIntent::new(
            Uuid::new_v4(), "shipping_fee", "shipment", Uuid::new_v4(),
            5_000, "AED", "network_international", chrono::Duration::minutes(30),
        )
    }

    #[test]
    fn new_intent_starts_created() {
        let intent = make_intent();
        assert_eq!(intent.status, PaymentIntentStatus::Created);
        assert!(intent.gateway_payment_ref.is_none());
    }

    #[test]
    fn capture_transitions_to_captured_and_stores_the_reference() {
        let mut intent = make_intent();
        intent.capture("ni-txn-123".into()).unwrap();
        assert_eq!(intent.status, PaymentIntentStatus::Captured);
        assert_eq!(intent.gateway_payment_ref.as_deref(), Some("ni-txn-123"));
    }

    #[test]
    fn capture_is_idempotent_on_replay_of_the_same_reference() {
        let mut intent = make_intent();
        intent.capture("ni-txn-123".into()).unwrap();
        intent.capture("ni-txn-123".into()).unwrap();
        assert_eq!(intent.status, PaymentIntentStatus::Captured);
    }

    #[test]
    fn capture_rejects_a_conflicting_second_reference() {
        let mut intent = make_intent();
        intent.capture("ni-txn-123".into()).unwrap();
        assert!(intent.capture("ni-txn-999".into()).is_err());
    }

    #[test]
    fn fail_after_captured_is_rejected() {
        let mut intent = make_intent();
        intent.capture("ni-txn-123".into()).unwrap();
        assert!(intent.fail().is_err());
    }

    #[test]
    fn expire_only_applies_to_created_or_pending() {
        let mut intent = make_intent();
        intent.capture("ni-txn-123".into()).unwrap();
        assert!(intent.expire().is_err());
    }

    #[test]
    fn refund_requires_captured_state() {
        let mut intent = make_intent();
        assert!(intent.refund().is_err());
        intent.capture("ni-txn-123".into()).unwrap();
        intent.refund().unwrap();
        assert_eq!(intent.status, PaymentIntentStatus::Refunded);
    }

    #[test]
    fn fail_on_an_expired_intent_is_rejected_not_a_silent_transition() {
        // Gap 3: Expired must be terminal, the same way Captured/Refunded
        // already are — the old catch-all let a late "declined" webhook
        // re-transition an already-expired intent to Failed.
        let mut intent = make_intent();
        intent.expire().unwrap();
        assert_eq!(intent.status, PaymentIntentStatus::Expired);
        assert!(intent.fail().is_err());
        assert_eq!(intent.status, PaymentIntentStatus::Expired, "must remain Expired, not silently move to Failed");
    }

    #[test]
    fn fail_on_a_refunding_intent_is_rejected() {
        // A refund claim is exclusive — an intent mid-refund must not be
        // pulled sideways into Failed by an unrelated declined-webhook replay.
        let mut intent = make_intent();
        intent.status = PaymentIntentStatus::Refunding;
        assert!(intent.fail().is_err());
    }

    #[test]
    fn refund_status_round_trips_through_as_str_and_parse() {
        assert_eq!(PaymentIntentStatus::Refunding.as_str(), "refunding");
        assert_eq!(PaymentIntentStatus::parse("refunding"), Some(PaymentIntentStatus::Refunding));
    }

    // ── authorize / capture_authorized / void ───────────────────────────────

    #[test]
    fn authorized_and_voided_round_trip_through_as_str_and_parse() {
        assert_eq!(PaymentIntentStatus::Authorized.as_str(), "authorized");
        assert_eq!(PaymentIntentStatus::parse("authorized"), Some(PaymentIntentStatus::Authorized));
        assert_eq!(PaymentIntentStatus::Voided.as_str(), "voided");
        assert_eq!(PaymentIntentStatus::parse("voided"), Some(PaymentIntentStatus::Voided));
    }

    #[test]
    fn authorize_transitions_to_authorized_and_stores_the_reference() {
        let mut intent = make_intent();
        intent.authorize("ni-auth-123".into()).unwrap();
        assert_eq!(intent.status, PaymentIntentStatus::Authorized);
        assert_eq!(intent.gateway_payment_ref.as_deref(), Some("ni-auth-123"));
    }

    #[test]
    fn authorize_is_idempotent_on_replay_of_the_same_reference() {
        let mut intent = make_intent();
        intent.authorize("ni-auth-123".into()).unwrap();
        intent.authorize("ni-auth-123".into()).unwrap();
        assert_eq!(intent.status, PaymentIntentStatus::Authorized);
    }

    #[test]
    fn authorize_rejects_a_conflicting_second_reference() {
        let mut intent = make_intent();
        intent.authorize("ni-auth-123".into()).unwrap();
        assert!(intent.authorize("ni-auth-999".into()).is_err());
    }

    #[test]
    fn capture_authorized_transitions_an_authorized_intent_to_captured() {
        let mut intent = make_intent();
        intent.authorize("ni-auth-123".into()).unwrap();
        let amt = intent.amount_cents;
        intent.capture_authorized(amt).unwrap();
        assert_eq!(intent.status, PaymentIntentStatus::Captured);
        // The authorization's own payment reference is preserved unchanged —
        // capture_authorized() must not overwrite it, since refund() later
        // needs it to identify the same underlying NI payment.
        assert_eq!(intent.gateway_payment_ref.as_deref(), Some("ni-auth-123"));
    }

    #[test]
    fn capture_authorized_is_idempotent_on_replay() {
        let mut intent = make_intent();
        intent.authorize("ni-auth-123".into()).unwrap();
        let amt = intent.amount_cents;
        intent.capture_authorized(amt).unwrap();
        let amt = intent.amount_cents;
        intent.capture_authorized(amt).unwrap();
        assert_eq!(intent.status, PaymentIntentStatus::Captured);
    }

    #[test]
    fn capture_authorized_rejects_an_intent_that_was_never_authorized() {
        let mut intent = make_intent(); // still Created
        assert!(intent.capture_authorized(1).is_err());
    }

    #[test]
    fn void_transitions_an_authorized_intent_to_voided() {
        let mut intent = make_intent();
        intent.authorize("ni-auth-123".into()).unwrap();
        intent.void().unwrap();
        assert_eq!(intent.status, PaymentIntentStatus::Voided);
    }

    #[test]
    fn void_is_idempotent_on_replay() {
        let mut intent = make_intent();
        intent.authorize("ni-auth-123".into()).unwrap();
        intent.void().unwrap();
        intent.void().unwrap();
        assert_eq!(intent.status, PaymentIntentStatus::Voided);
    }

    #[test]
    fn void_rejects_an_intent_that_was_never_authorized() {
        let mut intent = make_intent(); // still Created
        assert!(intent.void().is_err());
    }

    #[test]
    fn a_voided_intent_can_never_be_captured() {
        let mut intent = make_intent();
        intent.authorize("ni-auth-123".into()).unwrap();
        intent.void().unwrap();
        assert!(intent.capture_authorized(1).is_err());
        assert_eq!(intent.status, PaymentIntentStatus::Voided, "must remain Voided");
    }

    #[test]
    fn refund_rejects_an_authorized_intent() {
        // Only Captured is refundable — an authorization hold with no
        // capture yet must not be refundable via the existing refund() path.
        let mut intent = make_intent();
        intent.authorize("ni-auth-123".into()).unwrap();
        assert!(intent.refund().is_err());
    }

    #[test]
    fn refund_rejects_a_voided_intent() {
        let mut intent = make_intent();
        intent.authorize("ni-auth-123".into()).unwrap();
        intent.void().unwrap();
        assert!(intent.refund().is_err());
    }

    #[test]
    fn fail_after_authorized_is_rejected() {
        // Real funds are ring-fenced once Authorized — the same money-safety
        // reasoning as fail_after_captured_is_rejected above: a stale
        // "declined" webhook must not silently relabel a live hold as Failed.
        let mut intent = make_intent();
        intent.authorize("ni-auth-123".into()).unwrap();
        assert!(intent.fail().is_err());
        assert_eq!(intent.status, PaymentIntentStatus::Authorized);
    }

    #[test]
    fn fail_after_voided_is_rejected() {
        let mut intent = make_intent();
        intent.authorize("ni-auth-123".into()).unwrap();
        intent.void().unwrap();
        assert!(intent.fail().is_err());
        assert_eq!(intent.status, PaymentIntentStatus::Voided);
    }
}
