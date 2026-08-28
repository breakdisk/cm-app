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
}

impl PaymentIntentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Created   => "created",
            Self::Pending   => "pending",
            Self::Captured  => "captured",
            Self::Failed    => "failed",
            Self::Refunded  => "refunded",
            Self::Expired   => "expired",
            Self::Refunding => "refunding",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "created"   => Some(Self::Created),
            "pending"   => Some(Self::Pending),
            "captured"  => Some(Self::Captured),
            "failed"    => Some(Self::Failed),
            "refunded"  => Some(Self::Refunded),
            "expired"   => Some(Self::Expired),
            "refunding" => Some(Self::Refunding),
            _           => None,
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
    pub fn fail(&mut self) -> Result<(), &'static str> {
        match self.status {
            PaymentIntentStatus::Failed => return Ok(()), // idempotent
            PaymentIntentStatus::Captured | PaymentIntentStatus::Refunded | PaymentIntentStatus::Refunding => {
                return Err("Cannot fail an intent that already captured");
            }
            PaymentIntentStatus::Expired => {
                return Err("Cannot fail an intent that has already expired");
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
}
