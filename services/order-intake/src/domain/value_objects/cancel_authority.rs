//! Who may act on a shipment by id.
//!
//! Row-level security does not do this. Migration 0001 forces a policy on
//! `current_setting('app.tenant_id')`, but nothing in order-intake ever sets it,
//! so `find_by_id` (`WHERE id = $1`) returned any tenant's row. A cancel then
//! reached payments' shipment-cancelled consumer, which refunded the captured
//! payment in full. Every user-facing by-id action is checked here instead.

use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Actor {
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    /// True when the caller may act on any shipment in their tenant. False when
    /// they may act only on shipments they booked. See `is_tenant_wide`.
    pub tenant_wide: bool,
}

/// Who is performing a by-id action.
///
/// `Unset` is the default, so a command deserialised from a request body, or
/// built by a caller that forgot to say who it is, is refused rather than
/// silently trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActingAs {
    #[default]
    Unset,
    /// A person with a token.
    User(Actor),
    /// A mesh-internal caller with no user, such as the payment-failure consumer.
    /// Trusted for the same reason `/v1/internal/*` is: Istio asserts the caller.
    System,
}

/// Merchants and customers can create shipments but not update them, and are
/// limited to the ones they booked. Everyone else reaches the whole tenant.
///
/// The rule lives in `logisticos_auth` because delivery-experience applies the
/// same one to tracking reads; this name is kept for the call sites here.
pub use logisticos_auth::rbac::shipments_tenant_wide as is_tenant_wide;

/// Tenant is always enforced, whatever the actor's breadth.
pub fn may_act_on(actor: &Actor, shipment_tenant: Uuid, shipment_owner: Uuid) -> bool {
    if actor.tenant_id != shipment_tenant {
        return false;
    }
    actor.tenant_wide || actor.user_id == shipment_owner
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn ids() -> (Uuid, Uuid, Uuid) {
        (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4())
    }

    /// The live bug: an operator in tenant A cancelling tenant B's shipment by
    /// uuid, which payments then refunded in full.
    #[test]
    fn another_tenant_is_refused_even_for_an_operator() {
        let (t1, t2, u) = ids();
        let op = Actor { tenant_id: t1, user_id: u, tenant_wide: true };
        assert!(!may_act_on(&op, t2, u));
    }

    #[test]
    fn a_customer_may_act_only_on_their_own() {
        let (t, me, other) = ids();
        let customer = Actor { tenant_id: t, user_id: me, tenant_wide: false };
        assert!(may_act_on(&customer, t, me));
        assert!(!may_act_on(&customer, t, other));
    }

    #[test]
    fn an_operator_may_act_on_anything_in_their_tenant() {
        let (t, op, owner) = ids();
        let actor = Actor { tenant_id: t, user_id: op, tenant_wide: true };
        assert!(may_act_on(&actor, t, owner));
    }

    /// Scope comes from permissions, not role strings. Merchants and customers
    /// can create but not update, so they are owner-scoped. Operators, drivers,
    /// partners and read-only users reach the whole tenant.
    #[test]
    fn owner_scope_is_derived_from_create_without_update() {
        assert!(!is_tenant_wide(true, false), "merchant / customer");
        assert!(is_tenant_wide(true, true), "admin / tenant_admin");
        assert!(is_tenant_wide(false, true), "dispatcher / hub_scanner");
        assert!(is_tenant_wide(false, false), "driver / partner / readonly");
    }

    /// A command that nobody stamped with an actor must not reach the data.
    #[test]
    fn unset_is_the_default() {
        assert_eq!(ActingAs::default(), ActingAs::Unset);
    }
}
