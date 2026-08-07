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
                tenant_id, c.id, product.clone(), external_ref, trip_cents, tip_cents);
            self.assignments.save(&a).await?;
            offers.push(a);
        }
        Ok(offers)
    }

    /// A courier accepts an offer. Returns `false` when another courier got
    /// there first — the caller should show "already taken", not an error.
    pub async fn claim(&self, tenant_id: Uuid, assignment_id: Uuid) -> anyhow::Result<bool> {
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
        if a.trip_cents > 0 || a.tip_cents > 0 {
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

        ledger.credit_trip(a.trip_cents, 0, a.external_ref);
        if a.tip_cents > 0 {
            ledger.credit_tip(a.tip_cents, a.external_ref);
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
fn current_period() -> String {
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
