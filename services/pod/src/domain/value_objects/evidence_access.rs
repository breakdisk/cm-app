//! Who may read a proof of delivery / pickup.
//!
//! The tenant boundary is enforced by the repositories. Inside a tenant, a proof
//! (recipient photo, signature, GPS) is readable by:
//!
//! - **operators** (admin, tenant_admin, dispatcher, ops, support): the whole tenant;
//! - **drivers / couriers**: proofs they captured (`driver_id` is set from
//!   `claims.user_id` at capture);
//! - **merchants and customers**: proofs of shipments they booked. order-intake
//!   stamps `shipment.merchant_id = claims.user_id` for every booker, customers
//!   included, so one id answers both.
//!
//! `readonly` and `finance` read tenant-wide: readonly already holds tenant-wide
//! `shipments:read`, and finance needs delivery evidence to settle COD disputes.
//!
//! Any other role (hub_scanner, partner without a driver role,
//! unknown) reads nothing. A caller holding several roles gets the union.
//!
//! "Not allowed" is always reported to callers as "not found", never "forbidden",
//! so proof and shipment ids cannot be probed.

use std::future::Future;

use logisticos_errors::AppResult;
use uuid::Uuid;

const OPERATOR_ROLES: &[&str] = &[
    "admin", "tenant_admin", "dispatcher", "ops", "support", "readonly", "finance",
];
const DRIVER_ROLES: &[&str] = &["driver", "courier"];
const OWNER_ROLES: &[&str] = &["merchant", "customer"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvidenceAccess {
    tenant_wide: bool,
    driver:      bool,
    owner:       bool,
}

impl EvidenceAccess {
    pub fn from_roles(roles: &[String]) -> Self {
        let has = |set: &[&str]| roles.iter().any(|r| set.contains(&r.as_str()));
        Self {
            tenant_wide: has(OPERATOR_ROLES),
            driver:      has(DRIVER_ROLES),
            owner:       has(OWNER_ROLES),
        }
    }

    /// May `caller` see a proof captured by `proof_driver` on a shipment whose
    /// booker is produced by `booker_of`?
    ///
    /// `booker_of` is only awaited when the answer depends on it (an owner-role
    /// caller who did not capture the proof): a driver or operator never costs an
    /// order-intake round trip. `Ok(None)` means order-intake says the shipment is
    /// not this tenant's. An `Err` (order-intake unreachable) propagates, so an
    /// unverifiable owner is refused with a 503, never treated as a pass.
    pub async fn may_view<F, Fut>(
        self,
        caller: Uuid,
        proof_driver: Uuid,
        booker_of: F,
    ) -> AppResult<bool>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = AppResult<Option<Uuid>>>,
    {
        if self.tenant_wide {
            return Ok(true);
        }
        if self.driver && caller == proof_driver {
            return Ok(true);
        }
        if self.owner {
            return Ok(booker_of().await? == Some(caller));
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use logisticos_errors::AppError;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn roles(r: &[&str]) -> EvidenceAccess {
        EvidenceAccess::from_roles(&r.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    /// Resolves the booker and counts how often order-intake was consulted.
    struct Booker { id: Option<Uuid>, calls: AtomicUsize }
    impl Booker {
        fn new(id: Option<Uuid>) -> Self { Self { id, calls: AtomicUsize::new(0) } }
        async fn lookup(&self) -> AppResult<Option<Uuid>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.id)
        }
    }

    async fn view(a: EvidenceAccess, caller: Uuid, driver: Uuid, b: &Booker) -> bool {
        a.may_view(caller, driver, || b.lookup()).await.expect("no outage")
    }

    #[tokio::test]
    async fn operators_read_the_whole_tenant_without_asking_order_intake() {
        for r in ["admin", "tenant_admin", "dispatcher", "ops", "support", "readonly", "finance"] {
            let b = Booker::new(Some(Uuid::new_v4()));
            assert!(view(roles(&[r]), Uuid::new_v4(), Uuid::new_v4(), &b).await, "{r}");
            assert_eq!(b.calls.load(Ordering::SeqCst), 0, "{r}");
        }
    }

    #[tokio::test]
    async fn a_driver_reads_only_what_they_captured() {
        let me = Uuid::new_v4();
        let b = Booker::new(Some(Uuid::new_v4()));
        for r in ["driver", "courier"] {
            assert!(view(roles(&[r]), me, me, &b).await, "own {r}");
            assert!(!view(roles(&[r]), me, Uuid::new_v4(), &b).await, "another's {r}");
        }
        assert_eq!(b.calls.load(Ordering::SeqCst), 0, "a driver never costs a lookup");
    }

    #[tokio::test]
    async fn a_driver_does_not_become_the_owner_by_being_the_booker() {
        // A driver-only token is not an owner role: even if ids coincide, the
        // booker lookup is never consulted.
        let me = Uuid::new_v4();
        let b = Booker::new(Some(me));
        assert!(!view(roles(&["driver"]), me, Uuid::new_v4(), &b).await);
        assert_eq!(b.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn merchants_and_customers_read_only_shipments_they_booked() {
        let me = Uuid::new_v4();
        for r in ["merchant", "customer"] {
            assert!(view(roles(&[r]), me, Uuid::new_v4(), &Booker::new(Some(me))).await, "own {r}");
            assert!(!view(roles(&[r]), me, Uuid::new_v4(), &Booker::new(Some(Uuid::new_v4()))).await, "other {r}");
            assert!(!view(roles(&[r]), me, Uuid::new_v4(), &Booker::new(None)).await, "foreign tenant {r}");
        }
    }

    #[tokio::test]
    async fn an_owner_token_cannot_read_by_capturing_driver_id() {
        // proof_driver == caller is the driver rule only.
        let me = Uuid::new_v4();
        assert!(!view(roles(&["merchant"]), me, me, &Booker::new(Some(Uuid::new_v4()))).await);
    }

    #[tokio::test]
    async fn unknown_and_non_evidence_roles_read_nothing() {
        let me = Uuid::new_v4();
        let b = Booker::new(Some(me));
        for r in ["hub_scanner", "partner", "store_manager", "bogus"] {
            assert!(!view(roles(&[r]), me, me, &b).await, "{r}");
        }
        assert!(!view(roles(&[]), me, me, &b).await, "no roles");
        assert_eq!(b.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn roles_combine_as_a_union() {
        let me = Uuid::new_v4();
        let b = Booker::new(Some(me));
        assert!(view(roles(&["driver", "merchant"]), me, Uuid::new_v4(), &b).await);
        assert!(view(roles(&["driver", "merchant"]), me, me, &Booker::new(None)).await);
    }

    #[tokio::test]
    async fn an_order_intake_outage_refuses_an_owner_instead_of_passing_them() {
        let me = Uuid::new_v4();
        let r = roles(&["merchant"])
            .may_view(me, Uuid::new_v4(), || async {
                Err::<Option<Uuid>, _>(AppError::ServiceUnavailable("down".into()))
            })
            .await;
        assert!(matches!(r, Err(AppError::ServiceUnavailable(_))));
    }
}
