//! Port to `services/payments`' mesh-internal payment-intent endpoints —
//! OmniDeliv's prepaid-checkout foundation: authorize-then-capture-or-void.
//!
//! A trait, like `CourierDispatch`, so OmniDeliv depends on the *intent*
//! rather than on `reqwest`, `services/payments`' wire shapes, or the fact
//! that it is even a separate service — a product calling a platform
//! capability through an interface it owns, not the reverse.

use async_trait::async_trait;
use uuid::Uuid;

/// The result of opening an authorization hold — funds ring-fenced on the
/// customer's card, not yet taken.
pub struct AuthorizedIntent {
    pub intent_id: Uuid,
    /// The hosted-checkout page the customer must complete before the
    /// authorization actually lands (`payment.intent.authorized`).
    pub checkout_url: String,
}

#[async_trait]
pub trait OrderPayments: Send + Sync {
    /// Opens an authorization hold for `amount_cents` — see `AuthorizedIntent`.
    /// Purpose `"omnideliv_order"`, reference type `"order"`, so
    /// `services/payments`' own `payment.intent.*` consumers can tell an
    /// OmniDeliv order apart from an order-intake shipment on the same topics.
    async fn authorize(
        &self,
        tenant_id: Uuid,
        order_id: Uuid,
        amount_cents: i64,
        currency: &str,
        return_url: &str,
    ) -> anyhow::Result<AuthorizedIntent>;

    /// Captures funds previously ring-fenced by `authorize`.
    ///
    /// `amount_cents` of `None` takes the whole authorization — the delivery
    /// path, where a courier accepting the job commits the whole basket.
    ///
    /// `Some(n)` takes part of it: ADR-0017's acceptance barrier, where a
    /// foodcourt table authorized every stall and only some accepted. Taking
    /// the full amount would charge for food nobody is making; taking nothing
    /// would refuse the whole table because one stall was shut.
    async fn capture(&self, intent_id: Uuid, amount_cents: Option<i64>) -> anyhow::Result<()>;

    /// Releases an authorization hold that was never captured — called when
    /// no courier accepted the job within the no-courier timeout, so the
    /// customer is never charged for a delivery nobody made.
    async fn void(&self, intent_id: Uuid) -> anyhow::Result<()>;
}
