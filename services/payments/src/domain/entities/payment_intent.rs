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
}

impl PaymentIntentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Created  => "created",
            Self::Pending  => "pending",
            Self::Captured => "captured",
            Self::Failed   => "failed",
            Self::Refunded => "refunded",
            Self::Expired  => "expired",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "created"  => Some(Self::Created),
            "pending"  => Some(Self::Pending),
            "captured" => Some(Self::Captured),
            "failed"   => Some(Self::Failed),
            "refunded" => Some(Self::Refunded),
            "expired"  => Some(Self::Expired),
            _          => None,
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

    pub fn fail(&mut self) -> Result<(), &'static str> {
        match self.status {
            PaymentIntentStatus::Failed => return Ok(()), // idempotent
            PaymentIntentStatus::Captured | PaymentIntentStatus::Refunded => {
                return Err("Cannot fail an intent that already captured");
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
}
