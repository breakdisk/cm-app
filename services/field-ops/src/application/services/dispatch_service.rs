use std::sync::Arc;

use uuid::Uuid;

use crate::domain::entities::{CourierAssignment, CourierLocation, ProductKey};
use crate::domain::repositories::CourierRepository;
use crate::domain::entities::CourierLedger;
use crate::infrastructure::db::{
    AssignmentRepository, ClaimOutcome, CourierLedgerRepository, LocationRepository,
};
use crate::infrastructure::messaging::{CourierEvent, CourierEvents};

pub struct DispatchService {
    couriers:    Arc<dyn CourierRepository>,
    assignments: Arc<dyn AssignmentRepository>,
    locations:   Arc<dyn LocationRepository>,
    ledgers:     Arc<dyn CourierLedgerRepository>,
    events:      Arc<dyn CourierEvents>,
    pay_bounds:  PayBounds,
}

/// What a product may declare a courier will earn.
///
/// A platform-tier guard, not a tariff: field-ops still never computes pay.
/// It only refuses to store a number that cannot be right.
#[derive(Debug, Clone, Copy)]
pub struct PayBounds {
    pub min_trip_cents: i64,
    pub max_trip_cents: i64,
    pub max_tip_cents:  i64,
}

impl Default for PayBounds {
    fn default() -> Self {
        Self { min_trip_cents: 2_000, max_trip_cents: 200_000, max_tip_cents: 500_000 }
    }
}

impl PayBounds {
    /// Check a declaration.
    ///
    /// Zero trip pay is allowed and unbounded below: a product that settles
    /// courier pay elsewhere declares nothing here, and forcing a floor on it
    /// would make field-ops credit money that product never intended to move.
    /// The floor applies only once a product has said it is paying.
    pub fn check(&self, trip_cents: i64, tip_cents: i64) -> Result<(), String> {
        if trip_cents < 0 || tip_cents < 0 {
            return Err("courier pay cannot be negative".into());
        }
        if trip_cents > 0 && trip_cents < self.min_trip_cents {
            return Err(format!(
                "trip pay {trip_cents} is below the {} floor — probably a units error",
                self.min_trip_cents
            ));
        }
        if trip_cents > self.max_trip_cents {
            return Err(format!("trip pay {trip_cents} exceeds the {} ceiling", self.max_trip_cents));
        }
        if tip_cents > self.max_tip_cents {
            return Err(format!("tip {tip_cents} exceeds the {} ceiling", self.max_tip_cents));
        }
        Ok(())
    }
}

impl DispatchService {
    pub fn new(
        couriers: Arc<dyn CourierRepository>,
        assignments: Arc<dyn AssignmentRepository>,
        locations: Arc<dyn LocationRepository>,
        ledgers: Arc<dyn CourierLedgerRepository>,
        events: Arc<dyn CourierEvents>,
        pay_bounds: PayBounds,
    ) -> Self {
        Self { couriers, assignments, locations, ledgers, events, pay_bounds }
    }

    /// Offer a job to the nearest dispatchable couriers. Offering is not
    /// claiming — several couriers may hold an offer for the same job; exactly
    /// one will win the claim.
    #[allow(clippy::too_many_arguments)]
    pub async fn offer_to_nearest(
        &self,
        tenant_id: Uuid,
        product: ProductKey,
        external_ref: Uuid,
        lat: f64,
        lng: f64,
        radius_km: f64,
        fanout: i64,
        trip_cents: i64,
        tip_cents: i64,
        cod_amount_cents: i64,
    ) -> anyhow::Result<Vec<CourierAssignment>> {
        // Checked before anything is offered or stored. Rejecting rather than
        // clamping is deliberate: clamping would credit the courier a different
        // number from the one the product recorded on its order, so the two
        // ledgers would disagree and the settlement identity would silently
        // stop holding. A refused offer leaves both sides consistent.
        if let Err(e) = self.pay_bounds.check(trip_cents, tip_cents) {
            anyhow::bail!("refusing the offer: {e}");
        }

        let candidates = self
            .couriers
            .find_available_near(tenant_id, lat, lng, radius_km, fanout)
            .await?;

        let mut offers = Vec::with_capacity(candidates.len());
        for c in candidates {
            // `product` is cloned per offer rather than copied: ProductKey owns
            // a String precisely so the set of products is not fixed at compile
            // time, and that ownership is worth one small allocation per offer
            // in a fan-out that is already doing a database write each turn.
            let a = CourierAssignment::offer_with_earnings(
                tenant_id, c.id, product.clone(), external_ref, trip_cents, tip_cents,
                cod_amount_cents);
            self.assignments.save(&a).await?;
            offers.push(a);
        }
        Ok(offers)
    }

    /// Record a cash handover, returning what is still outstanding.
    ///
    /// `None` when the user is not a courier. Remitting more than is held is
    /// allowed and lands the courier in credit — that is a real situation
    /// (an over-payment, or cash handed over before a delivery settles), and
    /// refusing it would leave the ledger unable to describe what happened.
    pub async fn remit_cash(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        amount_cents: i64,
        reference: Option<String>,
    ) -> anyhow::Result<Option<i64>> {
        let Some(courier) = self.couriers.find_by_user(tenant_id, user_id).await? else {
            return Ok(None);
        };
        let period = current_period();
        let mut ledger = match self.ledgers.find_open(tenant_id, courier.id, &period).await? {
            Some(l) => l,
            None => CourierLedger::open(tenant_id, courier.id, period),
        };

        ledger.record_cod_remitted(amount_cents, reference);
        self.ledgers.save(&ledger).await?;
        Ok(Some(ledger.cash_held_cents()))
    }

    /// This courier's ledger for the current period.
    ///
    /// `None` means they have not earned anything yet this period — a courier
    /// who has just started a shift, not an error. The caller renders a zero.
    pub async fn earnings_for_user(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> anyhow::Result<Option<CourierLedger>> {
        let Some(courier) = self.couriers.find_by_user(tenant_id, user_id).await? else {
            return Ok(None);
        };
        self.ledgers
            .find_open(tenant_id, courier.id, &current_period())
            .await
    }

    /// How many couriers could take a job here right now.
    ///
    /// The same query that backs dispatch, so the number a product plans
    /// against is the number it would actually get offered to — a count from a
    /// looser predicate would promise supply that dispatch then cannot find.
    pub async fn supply_near(
        &self,
        tenant_id: Uuid,
        lat: f64,
        lng: f64,
        radius_km: f64,
    ) -> anyhow::Result<usize> {
        // The limit is a ceiling on the answer, not a page size: a product
        // deciding whether to promise a delivery window does not care whether
        // there are 40 couriers or 400, and an unbounded scan on a busy tenant
        // would be a slow query on a read path an agent calls per run.
        const SUPPLY_CEILING: i64 = 50;
        Ok(self
            .couriers
            .find_available_near(tenant_id, lat, lng, radius_km, SUPPLY_CEILING)
            .await?
            .len())
    }

    /// The open offers waiting for one courier.
    ///
    /// `offer_to_nearest` returns ids to the *dispatching product*, not to the
    /// couriers it fanned out to, so without this a courier has no way to
    /// discover work and a driver app has nothing to render.
    pub async fn offers_for_user(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> anyhow::Result<Vec<CourierAssignment>> {
        let Some(courier) = self.couriers.find_by_user(tenant_id, user_id).await? else {
            // Not a courier in this tenant. An empty list, not an error: the
            // caller asked what work they have, and the answer is none.
            return Ok(Vec::new());
        };
        self.assignments
            .find_offered_for_courier(tenant_id, courier.id)
            .await
    }

    /// A courier accepts an offer. Returns `false` when another courier got
    /// there first — the caller should show "already taken", not an error.
    ///
    /// `user_id` is the authenticated caller, and the offer must have been made
    /// to *them*. Without that check any authenticated user in the tenant could
    /// claim any courier's offer just by naming its id — the ids are handed to
    /// the dispatching product, so they are not secret.
    pub async fn claim(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        assignment_id: Uuid,
    ) -> anyhow::Result<bool> {
        let Some(courier) = self.couriers.find_by_user(tenant_id, user_id).await? else {
            return Ok(false);
        };
        match self.assignments.find_by_id(tenant_id, assignment_id).await? {
            // Same answer as losing the race, deliberately: a caller probing
            // ids should not be able to tell "not yours" from "already taken".
            Some(a) if a.courier_id != courier.id => return Ok(false),
            None => return Ok(false),
            Some(_) => {}
        }

        match self.assignments.try_claim(tenant_id, assignment_id).await? {
            ClaimOutcome::Lost => Ok(false),
            ClaimOutcome::Won => {
                if let Some(a) = self.assignments.find_by_id(tenant_id, assignment_id).await? {
                    self.emit(CourierEvent::Assigned {
                        tenant_id,
                        product: a.product.as_str().to_string(),
                        external_ref: a.external_ref,
                        courier_id: a.courier_id,
                        assignment_id: a.id,
                    })
                    .await;
                }
                Ok(true)
            }
        }
    }

    /// A vendor's goods are in the bag.
    pub async fn mark_collected(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
        vendor_id: Uuid,
        device_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    ) -> anyhow::Result<bool> {
        let Some(a) = self.assignments.find_by_id(tenant_id, assignment_id).await? else {
            return Ok(false);
        };

        self.emit(CourierEvent::Collected {
            tenant_id,
            product: a.product.as_str().to_string(),
            external_ref: a.external_ref,
            courier_id: a.courier_id,
            vendor_id,
            device_timestamp,
        })
        .await;
        Ok(true)
    }

    /// The job is done. Completing the assignment frees the courier for the
    /// next one, which is why it is persisted rather than only published.
    pub async fn mark_delivered(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
        device_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    ) -> anyhow::Result<bool> {
        let Some(mut a) = self.assignments.find_by_id(tenant_id, assignment_id).await? else {
            return Ok(false);
        };

        a.complete();
        self.assignments.save(&a).await?;

        // Credit before publishing. A failed credit surfaces as an error the
        // caller retries; publishing first would tell OmniDeliv the job is done
        // while the courier is unpaid, and nothing downstream would notice.
        //
        // The COD debit rides in the same call for the same reason: the cash is
        // in the courier's hand the moment the door closes, and a delivery
        // recorded without it would show them in credit for money they are
        // holding — which is what a payout run would then hand them again.
        if a.trip_cents > 0 || a.tip_cents > 0 || a.cod_amount_cents > 0 {
            self.credit_courier(&a).await?;
        }

        self.emit(CourierEvent::Delivered {
            tenant_id,
            product: a.product.as_str().to_string(),
            external_ref: a.external_ref,
            courier_id: a.courier_id,
            device_timestamp,
        })
        .await;
        Ok(true)
    }

    /// Credit the courier for a completed job.
    ///
    /// The amounts come from the assignment, which is where the offering
    /// product declared them — field-ops never computes pay, because that would
    /// mean a platform tier knowing every product's tariff.
    async fn credit_courier(&self, a: &CourierAssignment) -> anyhow::Result<()> {
        let period = current_period();
        let mut ledger = match self
            .ledgers
            .find_open(a.tenant_id, a.courier_id, &period)
            .await?
        {
            Some(l) => l,
            None => CourierLedger::open(a.tenant_id, a.courier_id, period),
        };

        // Already credited — a retried delivery must not pay twice. Keyed on
        // the job rather than the assignment so a re-offer of the same job
        // cannot double-pay either.
        if ledger.entries.iter().any(|e| e.external_ref == Some(a.external_ref)) {
            return Ok(());
        }

        // Guarded rather than unconditional: a cash-only job with no trip pay
        // would otherwise append a zero-value earning, and a ledger full of
        // zero rows is harder to read than one without them.
        if a.trip_cents > 0 {
            ledger.credit_trip(a.trip_cents, 0, a.external_ref);
        }
        if a.tip_cents > 0 {
            ledger.credit_tip(a.tip_cents, a.external_ref);
        }
        // Negative. The courier now holds this much of the platform's money.
        if a.cod_amount_cents > 0 {
            ledger.record_cod_collected(a.cod_amount_cents, a.external_ref);
        }
        self.ledgers.save(&ledger).await
    }

    /// Fire-and-forget. The state change is already committed; failing it
    /// because the broker hiccupped would hand a claimed job to nobody. A
    /// missed event is recoverable by reconciliation — a lost claim is not.
    async fn emit(&self, event: CourierEvent) {
        if let Err(e) = self.events.publish(&event).await {
            tracing::error!(err = %e, ?event, "courier milestone publish failed");
        }
    }

    pub async fn record_position(
        &self,
        tenant_id: Uuid,
        courier_id: Uuid,
        lat: f64,
        lng: f64,
        device_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    ) -> anyhow::Result<()> {
        // The breadcrumb is the authoritative write; it must land first, and a
        // failure here fails the call. What follows is a cache refresh.
        let fix = CourierLocation::new(tenant_id, courier_id, lat, lng, device_timestamp);
        self.locations.record(&fix).await?;

        // Refresh the render cache on the courier row. This is NOT what supply
        // lookup reads — `find_available_near` joins courier_latest_locations,
        // because only the GiST index there can serve ST_DWithin. These columns
        // exist so a courier list renders without touching the time-series
        // table, and nothing dispatch-critical may depend on them.
        if let Some(mut c) = self.couriers.find_by_id(tenant_id, courier_id).await? {
            c.record_position(lat, lng);
            self.couriers.save(&c).await?;
        }
        Ok(())
    }
}

/// ISO week, matching OmniDeliv's vendor payout period so the two ledgers can
/// be reconciled against the same calendar.
/// The ledger period. Public so callers rendering an empty ledger label it
/// the same way the credit path would have.
pub fn current_period() -> String {
    use chrono::Datelike;
    let iso = chrono::Utc::now().iso_week();
    format!("{}-W{:02}", iso.year(), iso.week())
}

#[cfg(test)]
mod pay_bounds_tests {
    use super::PayBounds;

    fn bounds() -> PayBounds { PayBounds::default() }

    /// The bug this exists for: cents read as pesos. ₱58.00 declared as 58
    /// would pay a courier 58 centavos.
    #[test]
    fn a_units_error_is_refused() {
        assert!(bounds().check(58, 0).is_err());
        assert!(bounds().check(5_800, 0).is_ok());
    }

    /// The other direction: a multiplication that ran twice, or a fat finger.
    #[test]
    fn an_absurd_amount_is_refused() {
        assert!(bounds().check(5_800 * 5_800, 0).is_err());
        assert!(bounds().check(0, 900_000).is_err());
    }

    /// Zero is not "below the floor" — it means the product settles courier pay
    /// somewhere else, and forcing a floor there would have field-ops credit
    /// money nobody intended to move.
    #[test]
    fn declaring_no_pay_is_allowed() {
        assert!(bounds().check(0, 0).is_ok());
    }

    #[test]
    fn negative_pay_is_refused() {
        assert!(bounds().check(-1, 0).is_err());
        assert!(bounds().check(5_800, -1).is_err());
    }

    /// A generous tip on a cheap trip is ordinary and must pass — the ceiling
    /// is there for bugs, not for unusual customers.
    #[test]
    fn a_large_but_plausible_tip_passes() {
        assert!(bounds().check(5_800, 20_000).is_ok());
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Claim authorization
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod claim_authorization {
    use super::*;
    use crate::domain::entities::{AssignmentStatus, Courier, CourierLocation};
    use crate::domain::repositories::CourierRepository;
    use crate::infrastructure::db::{ClaimOutcome, LocationRepository};
    use std::sync::Mutex;

    const TENANT: Uuid = Uuid::from_u128(1);

    struct Couriers {
        /// user_id -> courier
        by_user: Vec<(Uuid, Courier)>,
    }

    #[async_trait::async_trait]
    impl CourierRepository for Couriers {
        async fn find_by_id(&self, _: Uuid, id: Uuid) -> anyhow::Result<Option<Courier>> {
            Ok(self.by_user.iter().find(|(_, c)| c.id == id).map(|(_, c)| c.clone()))
        }
        async fn find_by_user(&self, _: Uuid, user_id: Uuid) -> anyhow::Result<Option<Courier>> {
            Ok(self.by_user.iter().find(|(u, _)| *u == user_id).map(|(_, c)| c.clone()))
        }
        async fn save(&self, _: &Courier) -> anyhow::Result<()> { Ok(()) }
        async fn find_available_near(&self, _: Uuid, _: f64, _: f64, _: f64, _: i64)
            -> anyhow::Result<Vec<Courier>> { Ok(vec![]) }
    }

    #[derive(Default)]
    struct Assignments {
        rows:      Mutex<Vec<CourierAssignment>>,
        /// Every id try_claim was actually asked to swap. The assertion that
        /// matters: an unauthorized caller must not even reach the CAS.
        attempted: Mutex<Vec<Uuid>>,
    }

    #[async_trait::async_trait]
    impl AssignmentRepository for Assignments {
        async fn save(&self, _: &CourierAssignment) -> anyhow::Result<()> { Ok(()) }
        async fn try_claim(&self, _: Uuid, id: Uuid) -> anyhow::Result<ClaimOutcome> {
            self.attempted.lock().unwrap().push(id);
            let mut rows = self.rows.lock().unwrap();
            match rows.iter_mut().find(|a| a.id == id && a.status == AssignmentStatus::Offered) {
                Some(a) => { a.status = AssignmentStatus::Claimed; Ok(ClaimOutcome::Won) }
                None => Ok(ClaimOutcome::Lost),
            }
        }
        async fn find_by_id(&self, _: Uuid, id: Uuid) -> anyhow::Result<Option<CourierAssignment>> {
            Ok(self.rows.lock().unwrap().iter().find(|a| a.id == id).cloned())
        }
        async fn find_offered_for_courier(&self, _: Uuid, courier_id: Uuid)
            -> anyhow::Result<Vec<CourierAssignment>> {
            Ok(self.rows.lock().unwrap().iter()
                .filter(|a| a.courier_id == courier_id && a.status == AssignmentStatus::Offered)
                .cloned().collect())
        }
    }

    struct NoLocations;
    #[async_trait::async_trait]
    impl LocationRepository for NoLocations {
        async fn record(&self, _: &CourierLocation) -> anyhow::Result<()> { Ok(()) }
        async fn latest(&self, _: Uuid, _: Uuid) -> anyhow::Result<Option<CourierLocation>> { Ok(None) }
    }

    struct NoLedgers;
    #[async_trait::async_trait]
    impl crate::infrastructure::db::CourierLedgerRepository for NoLedgers {
        async fn find_open(&self, _: Uuid, _: Uuid, _: &str)
            -> anyhow::Result<Option<CourierLedger>> { Ok(None) }
        async fn save(&self, _: &CourierLedger) -> anyhow::Result<()> { Ok(()) }
    }

    fn courier() -> Courier {
        Courier::new(TENANT, Uuid::new_v4(), "A".into(), "B".into(), "+63".into())
    }

    /// (service, assignment_id, owner_user, other_user, assignments)
    fn fixture() -> (DispatchService, Uuid, Uuid, Uuid, Arc<Assignments>) {
        let owner_user = Uuid::new_v4();
        let other_user = Uuid::new_v4();
        let owner = courier();
        let other = courier();

        let a = CourierAssignment::offer_with_earnings(
            TENANT, owner.id, ProductKey::new("omnideliv".to_string()),
            Uuid::new_v4(), 3_500, 0, 0,
        );
        let id = a.id;

        let assignments = Arc::new(Assignments::default());
        assignments.rows.lock().unwrap().push(a);

        let svc = DispatchService::new(
            Arc::new(Couriers { by_user: vec![(owner_user, owner), (other_user, other)] }),
            assignments.clone(),
            Arc::new(NoLocations),
            Arc::new(NoLedgers),
            Arc::new(crate::infrastructure::messaging::NoopCourierEvents),
            PayBounds::default(),
        );
        (svc, id, owner_user, other_user, assignments)
    }

    #[tokio::test]
    async fn the_courier_the_offer_was_made_to_can_claim_it() {
        let (svc, id, owner, _, _) = fixture();
        assert!(svc.claim(TENANT, owner, id).await.unwrap());
    }

    /// The offer ids are handed to the dispatching product, so they are not
    /// secret. Another courier naming one must not be able to take the job.
    #[tokio::test]
    async fn another_courier_cannot_claim_someone_elses_offer() {
        let (svc, id, _, other, assignments) = fixture();

        assert!(!svc.claim(TENANT, other, id).await.unwrap());
        assert!(
            assignments.attempted.lock().unwrap().is_empty(),
            "an unauthorized claim must be refused before the CAS, not by it"
        );
    }

    /// Same answer as losing the race, on purpose: a caller probing ids should
    /// not be able to tell "not yours" from "already taken".
    #[tokio::test]
    async fn a_user_who_is_not_a_courier_is_refused() {
        let (svc, id, _, _, _) = fixture();
        assert!(!svc.claim(TENANT, Uuid::new_v4(), id).await.unwrap());
    }

    #[tokio::test]
    async fn offers_are_listed_only_for_the_courier_they_belong_to() {
        let (svc, id, owner, other, _) = fixture();

        let mine = svc.offers_for_user(TENANT, owner).await.unwrap();
        assert_eq!(mine.len(), 1);
        assert_eq!(mine[0].id, id);
        assert_eq!(mine[0].trip_cents, 3_500, "pay is on the list so the courier can decide");

        assert!(svc.offers_for_user(TENANT, other).await.unwrap().is_empty());
        assert!(svc.offers_for_user(TENANT, Uuid::new_v4()).await.unwrap().is_empty(),
                "a non-courier gets an empty list, not an error");
    }
}
