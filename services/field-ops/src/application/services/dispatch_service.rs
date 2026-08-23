use std::sync::Arc;

use uuid::Uuid;

use crate::domain::entities::{
    AssignmentStatus, Courier, CourierAssignment, CourierLocation, ProductKey,
};
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

/// The outcome of one payout run, in enough detail to reconcile.
#[derive(Debug, Default, Clone)]
pub struct PayoutRun {
    pub period: String,
    pub batch:  String,
    pub paid:   Vec<(Uuid, i64)>,
    pub paid_cents: i64,
    /// Owed money but still holding ours — paid next run, once they remit.
    pub skipped_holding_cash: Vec<(Uuid, i64)>,
    /// Zero or negative balance. Nothing to pay.
    pub skipped_nothing_owed: Vec<Uuid>,
    /// The ledger write failed. These are unpaid and must be retried.
    pub failed: Vec<Uuid>,
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

/// What happened to a remittance.
///
/// A refusal is a normal outcome, not an error: a courier tapping "remit" twice
/// on a flaky connection is the expected way to reach it. It is modelled
/// explicitly so the caller cannot mistake a refusal for a recorded handover —
/// the previous signature returned a bare `i64` and had no way to say "no".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemitOutcome {
    /// Recorded. Carries the cash still outstanding afterwards.
    Recorded { cash_still_held_cents: i64 },
    /// Refused: more than this courier is holding. Carries what they do hold.
    ExceedsCashHeld { cash_held_cents: i64 },
}

/// Who is asking where a courier is.
///
/// A caller identity, not a permission check: the handler translates the JWT
/// into one of these and this layer decides what it may see. Keeping the
/// decision here rather than in the handler makes it unit-testable without
/// minting tokens.
#[derive(Debug, Clone, Copy)]
pub enum PositionReader {
    /// A courier, by `user_id`. May read only an assignment addressed to them.
    Courier(Uuid),
    /// A product service holding `field-ops:read-position`. May read any
    /// assignment in its own tenant — it needs this to render customer
    /// tracking and has no courier identity of its own.
    Service,
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
        // Opaque: stored on each assignment and never inspected here. Cloned per
        // offer because the fan-out gives every candidate their own row, and a
        // courier reading their offer must see the same card as the rest.
        offer_card: Option<serde_json::Value>,
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
            let a = CourierAssignment::offer_with_card(
                tenant_id, c.id, product.clone(), external_ref, trip_cents, tip_cents,
                cod_amount_cents, offer_card.clone());
            self.assignments.save(&a).await?;
            offers.push(a);
        }
        Ok(offers)
    }

    /// A courier signs up.
    ///
    /// Registers as **offline**, not available: a new courier must go on duty
    /// deliberately, and starting them available would put someone who has just
    /// tapped "sign up" into the next proximity search.
    ///
    /// Idempotent on the user — signing up twice returns the existing profile
    /// rather than creating a second one, which the `id = user_id` invariant
    /// (ADR-0015) would reject anyway. Better to return their profile than an
    /// error they cannot act on.
    pub async fn register_courier(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        first_name: String,
        last_name: String,
        phone: String,
    ) -> anyhow::Result<Courier> {
        if let Some(existing) = self.couriers.find_by_user(tenant_id, user_id).await? {
            return Ok(existing);
        }
        let mut c = Courier::new(tenant_id, user_id, first_name, last_name, phone);
        // The collapse from ADR-0015: one identity for a field worker. Set
        // explicitly rather than relying on the constructor, because this is
        // the invariant `drivers_id_is_user_id` enforces on the sibling table.
        c.id = user_id;
        self.couriers.save(&c).await?;
        Ok(c)
    }

    /// Pay every courier who is owed money for a period.
    ///
    /// Two rules decide who gets paid, and both exist because getting them
    /// wrong hands out real money:
    ///
    /// 1. **Only a positive balance.** A negative one means the courier owes
    ///    us, and "paying" it would be a second transfer in the wrong
    ///    direction.
    /// 2. **Never while cash is outstanding.** A courier can be in credit
    ///    overall and still be holding our cash — earn 5000, collect 3000,
    ///    balance 2000. Paying that 2000 before the 3000 comes back means the
    ///    platform is down 3000 with nothing to reconcile against, and the
    ///    courier has been handed money they were already holding.
    ///
    /// Skipped couriers are returned rather than silently omitted: a payout run
    /// that quietly pays fewer people than expected is indistinguishable from
    /// one that worked.
    pub async fn run_payout(&self, period: &str, batch: &str) -> anyhow::Result<PayoutRun> {
        let mut run = PayoutRun { period: period.to_string(), batch: batch.to_string(), ..Default::default() };

        for mut ledger in self.ledgers.find_all_open(period).await? {
            let held = ledger.cash_held_cents();
            if held > 0 {
                run.skipped_holding_cash.push((ledger.courier_id, held));
                continue;
            }
            if ledger.balance_cents <= 0 {
                run.skipped_nothing_owed.push(ledger.courier_id);
                continue;
            }

            let amount = ledger.balance_cents;
            ledger.record_payout(amount, Some(batch.to_string()));

            // A failed save must not be reported as paid. Continue rather than
            // abort so one bad row does not stop everyone else being paid.
            if let Err(e) = self.ledgers.save(&ledger).await {
                tracing::error!(err = %e, courier_id = %ledger.courier_id, "payout write failed");
                run.failed.push(ledger.courier_id);
                continue;
            }

            run.paid_cents += amount;
            run.paid.push((ledger.courier_id, amount));
        }

        Ok(run)
    }

    /// Record a cash handover, returning what is still outstanding.
    ///
    /// `None` when the user is not a courier. Remitting more than is held is
    /// allowed and lands the courier in credit — that is a real situation
    /// (an over-payment, or cash handed over before a delivery settles), and
    /// refusing it would leave the ledger unable to describe what happened.
    /// Record a courier handing collected cash back to the platform.
    ///
    /// `Ok(None)` means the token does not belong to a courier. The outcome
    /// distinguishes a recorded remittance from one refused for exceeding the
    /// cash actually held — see [`RemitOutcome`].
    pub async fn remit_cash(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        amount_cents: i64,
        reference: Option<String>,
    ) -> anyhow::Result<Option<RemitOutcome>> {
        let Some(courier) = self.couriers.find_by_user(tenant_id, user_id).await? else {
            return Ok(None);
        };
        let period = current_period();
        let mut ledger = match self.ledgers.find_open(tenant_id, courier.id, &period).await? {
            Some(l) => l,
            None => CourierLedger::open(tenant_id, courier.id, period),
        };

        // A courier can only hand back cash they are actually holding.
        //
        // Without this check the endpoint mints balance out of nothing:
        // remitting money that was never collected credits the ledger, the
        // balance goes positive, and the next payout run pays it out. It is a
        // self-serve withdrawal, and the only thing standing between it and
        // real money is that nobody had tried.
        //
        // Not hypothetical — found by probing the running service. A 100-cent
        // remittance against a fully-settled ledger was accepted and credited,
        // and a test script had already walked 38900 out of the dev tenant this
        // way by remitting twice against a single collection.
        let held = ledger.cash_held_cents();
        if amount_cents > held {
            return Ok(Some(RemitOutcome::ExceedsCashHeld { cash_held_cents: held }));
        }

        ledger.record_cod_remitted(amount_cents, reference);
        self.ledgers.save(&ledger).await?;
        Ok(Some(RemitOutcome::Recorded {
            cash_still_held_cents: ledger.cash_held_cents(),
        }))
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

    /// The ops roster: every courier in the tenant.
    pub async fn list_couriers(
        &self,
        tenant_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<Courier>> {
        self.couriers.list_for_tenant(tenant_id, limit.clamp(1, 200), offset.max(0)).await
    }

    /// Suspend or reinstate a courier. Ops' lever, not the courier's.
    ///
    /// Deliberately separate from [`set_availability`]: `is_dispatchable`
    /// requires **both** flags, so a suspended courier can flip themselves on
    /// duty all day and still never be offered a job — and reinstating them
    /// does not silently clock them on.
    ///
    /// Returns `false` when no such courier exists **in this tenant**. This
    /// service has no row-level security — the tenant bound into each query is
    /// the whole of it — so a foreign id must read as absent rather than
    /// forbidden, matching every other route here.
    pub async fn set_courier_active(
        &self,
        tenant_id: Uuid,
        courier_id: Uuid,
        active: bool,
    ) -> anyhow::Result<bool> {
        let Some(mut courier) = self.couriers.find_by_id(tenant_id, courier_id).await? else {
            return Ok(false);
        };
        courier.is_active = active;
        courier.updated_at = chrono::Utc::now();
        self.couriers.save(&courier).await?;
        Ok(true)
    }

    /// A courier starts or ends their shift.
    ///
    /// This is the write that `find_available_near` reads. Without it a courier
    /// stays on the `offline` that `register` gave them, the proximity search
    /// skips them forever, and every order fans out to nobody — which is
    /// exactly what the platform did until this existed.
    ///
    /// Only the flag moves. Nothing here touches a live assignment: going off
    /// duty mid-job must not abandon the parcel a courier is carrying, and the
    /// supply query asks the `claimed` unique index directly rather than
    /// trusting this column, so a courier holding a job is excluded whatever it
    /// says.
    ///
    /// A deactivated courier can still flip their own flag and still will not
    /// be dispatched — `is_dispatchable` requires `is_active`, which belongs to
    /// whoever deactivated them, not to the courier.
    ///
    /// Returns `false` when the caller holds no courier row in this tenant, so
    /// the route can answer 404 without confirming whether an id exists — the
    /// same shape as `claim` and `offers_for_user`.
    pub async fn set_availability(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        available: bool,
    ) -> anyhow::Result<bool> {
        let Some(mut courier) = self.couriers.find_by_user(tenant_id, user_id).await? else {
            return Ok(false);
        };

        if available {
            courier.go_available();
        } else {
            courier.go_offline();
        }
        self.couriers.save(&courier).await?;
        Ok(true)
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
                    // Retire the same job's offers to everyone else. Best
                    // effort: the claim has already succeeded and the courier
                    // is on their way, so a failure here must not turn a won
                    // job into an error. The cost of missing it is a stale row
                    // in somebody's inbox, which is what this fixes rather than
                    // something it may create.
                    if let Err(e) = self
                        .assignments
                        .expire_other_offers(tenant_id, &a.product, a.external_ref, a.id)
                        .await
                    {
                        tracing::warn!(err = %e, job = %a.external_ref,
                            "could not expire the losing offers for this job");
                    }

                    self.emit(CourierEvent::Assigned {
                        tenant_id,
                        product: a.product.as_str().to_string(),
                        external_ref: a.external_ref,
                        courier_id: a.courier_id,
                        assignment_id: a.id,
                        // The caller *is* the courier — the ownership check
                        // above refused anyone else before the CAS ran.
                        courier_user_id: Some(user_id),
                    })
                    .await;
                }
                Ok(true)
            }
        }
    }

    /// The courier is at a stop.
    ///
    /// Published, never persisted: it changes no assignment state, and a
    /// milestone that only informs does not need a row. Gated on a live claim
    /// like `mark_collected` — being *offered* a job is not carrying it.
    pub async fn mark_arrived(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        assignment_id: Uuid,
        stop_ref: Uuid,
        device_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    ) -> anyhow::Result<bool> {
        let Some(a) = self.assignment_for_courier(tenant_id, user_id, assignment_id).await? else {
            return Ok(false);
        };
        if a.status != AssignmentStatus::Claimed {
            return Ok(false);
        }

        self.emit(CourierEvent::Arrived {
            tenant_id,
            product: a.product.as_str().to_string(),
            external_ref: a.external_ref,
            courier_id: a.courier_id,
            stop_ref,
            device_timestamp,
        })
        .await;
        Ok(true)
    }

    /// A vendor's goods are in the bag.
    ///
    /// `user_id` is the authenticated caller and the assignment must be
    /// **theirs**. Assignment ids are handed to the dispatching product, so
    /// they are not secret; without this check any authenticated user in the
    /// tenant could report milestones against another courier's job. `claim`
    /// has had this since it was hardened — these two had not.
    pub async fn mark_collected(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        assignment_id: Uuid,
        vendor_id: Uuid,
        device_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    ) -> anyhow::Result<bool> {
        let Some(a) = self.assignment_for_courier(tenant_id, user_id, assignment_id).await? else {
            return Ok(false);
        };
        // Being offered a job is not carrying it. `offer_to_nearest` addresses
        // one job to several couriers and only the winner's row is claimed;
        // the losers keep a readable assignment id and must not be able to
        // report milestones against it.
        if a.status != AssignmentStatus::Claimed {
            return Ok(false);
        }

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
    ///
    /// The most consequential call on this service: it completes the
    /// assignment, credits the courier and debits the cash they are holding. So
    /// it is gated twice — `assignment_for_courier` proves the caller is the
    /// courier this job is addressed to, and the status check below proves they
    /// actually claimed it.
    pub async fn mark_delivered(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        assignment_id: Uuid,
        device_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    ) -> anyhow::Result<bool> {
        let Some(mut a) = self.assignment_for_courier(tenant_id, user_id, assignment_id).await? else {
            return Ok(false);
        };

        // An at-least-once client retrying a delivery whose response was lost
        // has not made an error: the milestone already landed. Accept it
        // without re-completing, re-crediting or re-publishing — so the
        // duplicate stops here rather than relying on the consumer to absorb
        // it, and an offline queue is not told to park a milestone that
        // succeeded.
        if a.status == AssignmentStatus::Completed {
            return Ok(true);
        }
        // Being offered a job is not carrying it. `offer_to_nearest` addresses
        // one job to several couriers and only the winner's row is claimed; the
        // losers keep a readable assignment id, and without this they could
        // complete and be paid for a job they never took.
        if a.status != AssignmentStatus::Claimed {
            return Ok(false);
        }

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

    /// The assignment, if it is *addressed to* this courier — in any status.
    ///
    /// `None` covers all three refusals — not a courier, no such assignment,
    /// someone else's assignment — because the handler turns every one of them
    /// into the same 404. Distinguishing them would let a caller probe which
    /// assignment ids exist.
    ///
    /// Deliberately says nothing about status, and the name says so. Being
    /// *offered* a job is not holding it: `offer_to_nearest` addresses one job
    /// to `fanout` couriers and only the winner's row is ever claimed, so a
    /// check on identity alone passes for all of them. Each caller states the
    /// status it requires.
    async fn assignment_for_courier(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        assignment_id: Uuid,
    ) -> anyhow::Result<Option<CourierAssignment>> {
        let Some(courier) = self.couriers.find_by_user(tenant_id, user_id).await? else {
            return Ok(None);
        };
        let Some(a) = self.assignments.find_by_id(tenant_id, assignment_id).await? else {
            return Ok(None);
        };
        if a.courier_id != courier.id {
            return Ok(None);
        }
        Ok(Some(a))
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
        //
        // Asked of the *store*, not of `ledger.entries`. The ledger in hand is
        // only the current period's, and `current_period()` is the ISO week: an
        // offline queue retrying a lost response across the Sunday→Monday
        // boundary gets a fresh, empty ledger, and a guard that scanned only it
        // would find nothing and pay a second time — crediting the trip again
        // and re-debiting cash the courier already handed over.
        if self
            .ledgers
            .entry_exists_for_job(a.tenant_id, a.courier_id, a.external_ref)
            .await?
        {
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

    /// `position_for_assignment`, gated on who is asking.
    ///
    /// The unguarded version below is capability-based: any valid tenant JWT
    /// plus the assignment UUID reads a live courier position. That was safe
    /// only while assignment ids never reached a client, and the OmniDeliv
    /// driver app is the first thing to put them in couriers' phones — at which
    /// point one courier who learns an id can follow another around the city.
    ///
    /// `None` for an unauthorized reader, identical to an unknown assignment
    /// and to a courier with no fix yet. All three are a 404, so the response
    /// cannot be used to learn which assignment ids exist.
    pub async fn position_for_assignment_as(
        &self,
        tenant_id: Uuid,
        reader: PositionReader,
        assignment_id: Uuid,
    ) -> anyhow::Result<Option<(Uuid, CourierLocation, Option<f64>)>> {
        if let PositionReader::Courier(user_id) = reader {
            // Addressed-to, not status-gated — unlike the milestone calls. The
            // position returned is the assignment's own courier's, so a courier
            // reading a stale offer of theirs learns only where they already
            // are. Requiring `Claimed` would buy nothing and would break a
            // legitimate read between claiming and the first GPS fix.
            if self
                .assignment_for_courier(tenant_id, user_id, assignment_id)
                .await?
                .is_none()
            {
                return Ok(None);
            }
        }
        self.position_for_assignment(tenant_id, assignment_id).await
    }

    /// Where the courier holding this assignment is, with enough recent history
    /// to smooth a speed.
    ///
    /// Keyed on the assignment rather than the courier so a caller never needs
    /// to hold a courier id. field-ops therefore stays product-agnostic — it is
    /// answering "where is the courier on this job", not "where is this person".
    ///
    /// `None` for an unknown assignment and `None` for a courier with no fix on
    /// record. Both are a 404 to the caller: distinguishing them would confirm
    /// that an assignment id is real to someone who guessed it.
    ///
    /// **Unauthorized.** Everything reaching this must have gone through
    /// `position_for_assignment_as`, which is why this stays private to the
    /// service rather than being called from a handler.
    async fn position_for_assignment(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
    ) -> anyhow::Result<Option<(Uuid, CourierLocation, Option<f64>)>> {
        const SMOOTHING_WINDOW: i64 = 5;

        let Some(a) = self.assignments.find_by_id(tenant_id, assignment_id).await? else {
            return Ok(None);
        };

        let recent = self
            .locations
            .recent(tenant_id, a.courier_id, SMOOTHING_WINDOW)
            .await?;

        let Some(latest) = recent.first().cloned() else {
            return Ok(None);
        };

        let smoothed = crate::domain::entities::smoothed_speed_kph(&recent);
        Ok(Some((a.courier_id, latest, smoothed)))
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
        /// Not modelled by this fake — the roster is asserted in `courier_admin`.
        async fn list_for_tenant(&self, _: Uuid, _: i64, _: i64)
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
        /// This fake does not model the fan-out, so there is nothing to retire.
        /// Explicit rather than a trait default: a default would silently do
        /// nothing for a real repository that forgot to implement it.
        async fn expire_other_offers(
            &self,
            _: Uuid,
            _: &ProductKey,
            _: Uuid,
            _: Uuid,
        ) -> anyhow::Result<u64> { Ok(0) }
    }

    struct NoLocations;
    #[async_trait::async_trait]
    impl LocationRepository for NoLocations {
        async fn record(&self, _: &CourierLocation) -> anyhow::Result<()> { Ok(()) }
        async fn latest(&self, _: Uuid, _: Uuid) -> anyhow::Result<Option<CourierLocation>> { Ok(None) }
        async fn recent(&self, _: Uuid, _: Uuid, _: i64) -> anyhow::Result<Vec<CourierLocation>> { Ok(vec![]) }
    }

    struct NoLedgers;
    #[async_trait::async_trait]
    impl crate::infrastructure::db::CourierLedgerRepository for NoLedgers {
        async fn find_open(&self, _: Uuid, _: Uuid, _: &str)
            -> anyhow::Result<Option<CourierLedger>> { Ok(None) }
        async fn save(&self, _: &CourierLedger) -> anyhow::Result<()> { Ok(()) }
        async fn find_all_open(&self, _: &str) -> anyhow::Result<Vec<CourierLedger>> { Ok(vec![]) }
        async fn entry_exists_for_job(&self, _: Uuid, _: Uuid, _: Uuid) -> anyhow::Result<bool> { Ok(false) }
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

// ─────────────────────────────────────────────────────────────────────────────
// Payout rules
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod payout_rules {
    use super::*;
    use crate::domain::entities::{Courier, CourierLocation};
    use crate::domain::repositories::CourierRepository;
    use crate::infrastructure::db::{ClaimOutcome, CourierLedgerRepository, LocationRepository};
    use std::sync::Mutex;

    const PERIOD: &str = "2026-W32";

    struct Ledgers(Mutex<Vec<CourierLedger>>);

    #[async_trait::async_trait]
    impl CourierLedgerRepository for Ledgers {
        async fn find_open(&self, _: Uuid, courier_id: Uuid, _: &str)
            -> anyhow::Result<Option<CourierLedger>> {
            Ok(self.0.lock().unwrap().iter().find(|l| l.courier_id == courier_id).cloned())
        }
        async fn save(&self, l: &CourierLedger) -> anyhow::Result<()> {
            let mut v = self.0.lock().unwrap();
            if let Some(slot) = v.iter_mut().find(|x| x.id == l.id) { *slot = l.clone(); }
            Ok(())
        }
        async fn find_all_open(&self, _: &str) -> anyhow::Result<Vec<CourierLedger>> {
            Ok(self.0.lock().unwrap().clone())
        }
        async fn entry_exists_for_job(&self, _: Uuid, _: Uuid, _: Uuid) -> anyhow::Result<bool> { Ok(false) }
    }

    struct NoCouriers;
    #[async_trait::async_trait]
    impl CourierRepository for NoCouriers {
        async fn find_by_id(&self, _: Uuid, _: Uuid) -> anyhow::Result<Option<Courier>> { Ok(None) }
        async fn find_by_user(&self, _: Uuid, _: Uuid) -> anyhow::Result<Option<Courier>> { Ok(None) }
        async fn save(&self, _: &Courier) -> anyhow::Result<()> { Ok(()) }
        async fn find_available_near(&self, _: Uuid, _: f64, _: f64, _: f64, _: i64)
            -> anyhow::Result<Vec<Courier>> { Ok(vec![]) }
        /// Not modelled by this fake — the roster is asserted in `courier_admin`.
        async fn list_for_tenant(&self, _: Uuid, _: i64, _: i64)
            -> anyhow::Result<Vec<Courier>> { Ok(vec![]) }
    }
    struct NoLoc;
    #[async_trait::async_trait]
    impl LocationRepository for NoLoc {
        async fn record(&self, _: &CourierLocation) -> anyhow::Result<()> { Ok(()) }
        async fn latest(&self, _: Uuid, _: Uuid) -> anyhow::Result<Option<CourierLocation>> { Ok(None) }
        async fn recent(&self, _: Uuid, _: Uuid, _: i64) -> anyhow::Result<Vec<CourierLocation>> { Ok(vec![]) }
    }
    struct NoAssign;
    #[async_trait::async_trait]
    impl AssignmentRepository for NoAssign {
        async fn save(&self, _: &CourierAssignment) -> anyhow::Result<()> { Ok(()) }
        async fn try_claim(&self, _: Uuid, _: Uuid) -> anyhow::Result<ClaimOutcome> { Ok(ClaimOutcome::Lost) }
        async fn find_by_id(&self, _: Uuid, _: Uuid) -> anyhow::Result<Option<CourierAssignment>> { Ok(None) }
        async fn find_offered_for_courier(&self, _: Uuid, _: Uuid)
            -> anyhow::Result<Vec<CourierAssignment>> { Ok(vec![]) }
        /// This fake does not model the fan-out, so there is nothing to retire.
        /// Explicit rather than a trait default: a default would silently do
        /// nothing for a real repository that forgot to implement it.
        async fn expire_other_offers(
            &self,
            _: Uuid,
            _: &ProductKey,
            _: Uuid,
            _: Uuid,
        ) -> anyhow::Result<u64> { Ok(0) }
    }

    fn svc(ledgers: Arc<Ledgers>) -> DispatchService {
        DispatchService::new(
            Arc::new(NoCouriers), Arc::new(NoAssign), Arc::new(NoLoc), ledgers,
            Arc::new(crate::infrastructure::messaging::NoopCourierEvents),
            PayBounds::default(),
        )
    }

    fn ledger_owed(amount: i64) -> CourierLedger {
        let mut l = CourierLedger::open(Uuid::new_v4(), Uuid::new_v4(), PERIOD.into());
        l.credit_trip(amount, 1, Uuid::new_v4());
        l
    }

    #[tokio::test]
    async fn a_courier_who_is_owed_money_gets_paid_and_lands_at_zero() {
        let ledgers = Arc::new(Ledgers(Mutex::new(vec![ledger_owed(3_500)])));
        let run = svc(ledgers.clone()).run_payout(PERIOD, "b1").await.unwrap();

        assert_eq!(run.paid.len(), 1);
        assert_eq!(run.paid_cents, 3_500);
        assert_eq!(ledgers.0.lock().unwrap()[0].balance_cents, 0,
                   "a paid ledger is square, not still owing");
    }

    /// The rule that protects real money. A courier can be in credit overall
    /// and still be holding our cash — earn 5000, collect 3000, balance 2000.
    /// Paying that 2000 before the 3000 comes back leaves the platform down
    /// 3000 with nothing to reconcile against.
    #[tokio::test]
    async fn a_courier_still_holding_cash_is_not_paid_even_when_in_credit() {
        let mut l = ledger_owed(5_000);
        l.record_cod_collected(3_000, Uuid::new_v4());
        assert_eq!(l.balance_cents, 2_000, "precondition: in credit overall");

        let ledgers = Arc::new(Ledgers(Mutex::new(vec![l])));
        let run = svc(ledgers.clone()).run_payout(PERIOD, "b1").await.unwrap();

        assert!(run.paid.is_empty(), "must not pay while our cash is out");
        assert_eq!(run.skipped_holding_cash, vec![(ledgers.0.lock().unwrap()[0].courier_id, 3_000)]);
        assert_eq!(ledgers.0.lock().unwrap()[0].balance_cents, 2_000, "untouched");
    }

    /// A negative balance means the courier owes us. "Paying" it would be a
    /// second transfer in the wrong direction.
    #[tokio::test]
    async fn a_courier_in_debt_is_never_paid() {
        let mut l = ledger_owed(1_000);
        l.record_cod_collected(9_000, Uuid::new_v4());
        l.record_cod_remitted(9_000, None);
        l.adjust(-5_000, "damages".into());
        assert!(l.balance_cents < 0);

        let ledgers = Arc::new(Ledgers(Mutex::new(vec![l])));
        let run = svc(ledgers).run_payout(PERIOD, "b1").await.unwrap();

        assert!(run.paid.is_empty());
        assert_eq!(run.skipped_nothing_owed.len(), 1);
    }

    /// Running twice must not pay twice — the first run leaves a zero balance,
    /// which the second reads as nothing owed.
    #[tokio::test]
    async fn a_second_run_pays_nothing_more() {
        let ledgers = Arc::new(Ledgers(Mutex::new(vec![ledger_owed(3_500)])));
        let s = svc(ledgers.clone());

        let first  = s.run_payout(PERIOD, "b1").await.unwrap();
        let second = s.run_payout(PERIOD, "b2").await.unwrap();

        assert_eq!(first.paid_cents, 3_500);
        assert_eq!(second.paid_cents, 0, "a repeated run must not pay again");
        assert_eq!(ledgers.0.lock().unwrap()[0].balance_cents, 0);
    }

    /// Once the cash comes back, the same courier is paid on the next run.
    #[tokio::test]
    async fn remitting_unblocks_the_next_payout() {
        let mut l = ledger_owed(5_000);
        l.record_cod_collected(3_000, Uuid::new_v4());
        let ledgers = Arc::new(Ledgers(Mutex::new(vec![l])));
        let s = svc(ledgers.clone());

        assert!(s.run_payout(PERIOD, "b1").await.unwrap().paid.is_empty());

        ledgers.0.lock().unwrap()[0].record_cod_remitted(3_000, None);
        let after = s.run_payout(PERIOD, "b2").await.unwrap();

        assert_eq!(after.paid_cents, 5_000, "the full earning, once we have our cash back");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Assignment → courier position lookup
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod position_lookup {
    use super::*;

    // ── Reader authorization ─────────────────────────────────────────────
    //
    // The route was capability-based: any valid tenant JWT plus the assignment
    // UUID read a live courier position. Safe only while ids never reached a
    // client, which the driver app ends.

    /// A courier with a fix on record. `position_for_assignment` reads
    /// `recent()` rather than `latest()` — it needs a short history to smooth a
    /// speed — so a mock that only answers `latest` reports "no position" and
    /// makes an authorization test pass for the wrong reason.
    struct HeldFix;
    #[async_trait::async_trait]
    impl LocationRepository for HeldFix {
        async fn record(&self, _: &CourierLocation) -> anyhow::Result<()> { Ok(()) }
        async fn latest(&self, tenant_id: Uuid, courier_id: Uuid)
            -> anyhow::Result<Option<CourierLocation>> {
            Ok(Some(CourierLocation::new(tenant_id, courier_id, 14.5547, 121.0244, None)))
        }
        async fn recent(&self, tenant_id: Uuid, courier_id: Uuid, _: i64)
            -> anyhow::Result<Vec<CourierLocation>> {
            Ok(vec![CourierLocation::new(tenant_id, courier_id, 14.5547, 121.0244, None)])
        }
    }

    struct TwoCouriers { by_user: Vec<(Uuid, Courier)> }
    #[async_trait::async_trait]
    impl CourierRepository for TwoCouriers {
        async fn find_by_id(&self, _: Uuid, id: Uuid) -> anyhow::Result<Option<Courier>> {
            Ok(self.by_user.iter().find(|(_, c)| c.id == id).map(|(_, c)| c.clone()))
        }
        async fn find_by_user(&self, _: Uuid, user_id: Uuid) -> anyhow::Result<Option<Courier>> {
            Ok(self.by_user.iter().find(|(u, _)| *u == user_id).map(|(_, c)| c.clone()))
        }
        async fn save(&self, _: &Courier) -> anyhow::Result<()> { Ok(()) }
        async fn find_available_near(&self, _: Uuid, _: f64, _: f64, _: f64, _: i64)
            -> anyhow::Result<Vec<Courier>> { Ok(vec![]) }
        /// Not modelled by this fake — the roster is asserted in `courier_admin`.
        async fn list_for_tenant(&self, _: Uuid, _: i64, _: i64)
            -> anyhow::Result<Vec<Courier>> { Ok(vec![]) }
    }

    /// (service, assignment_id, holder_user, other_user)
    fn reader_fixture() -> (DispatchService, Uuid, Uuid, Uuid) {
        let holder_user = Uuid::new_v4();
        let other_user  = Uuid::new_v4();
        let holder = Courier::new(TENANT, holder_user, "A".into(), "B".into(), "+63".into());
        let other  = Courier::new(TENANT, other_user,  "C".into(), "D".into(), "+63".into());

        let mut a = CourierAssignment::offer_with_earnings(
            TENANT, holder.id, ProductKey::new("omnideliv".to_string()),
            Uuid::new_v4(), 3_500, 0, 0,
        );
        a.status = crate::domain::entities::AssignmentStatus::Claimed;
        let id = a.id;

        let assignments = Arc::new(Assignments::default());
        assignments.0.lock().unwrap().push(a);

        let svc = DispatchService::new(
            Arc::new(TwoCouriers { by_user: vec![(holder_user, holder), (other_user, other)] }),
            assignments,
            Arc::new(HeldFix),
            Arc::new(NoLedgers),
            Arc::new(crate::infrastructure::messaging::NoopCourierEvents),
            PayBounds::default(),
        );
        (svc, id, holder_user, other_user)
    }

    #[tokio::test]
    async fn the_courier_the_assignment_is_addressed_to_can_read_the_position() {
        let (svc, id, holder, _) = reader_fixture();
        let seen = svc
            .position_for_assignment_as(TENANT, PositionReader::Courier(holder), id)
            .await
            .unwrap();
        assert!(seen.is_some());
    }

    /// The leak this closes. Without it, one courier who learns an assignment
    /// id can follow another courier around the city.
    #[tokio::test]
    async fn a_courier_cannot_read_another_couriers_position() {
        let (svc, id, _, other) = reader_fixture();
        let seen = svc
            .position_for_assignment_as(TENANT, PositionReader::Courier(other), id)
            .await
            .unwrap();
        assert!(seen.is_none(), "a courier the job is not addressed to gets nothing");
    }

    /// omnideliv renders customer tracking and is not a courier; its minted
    /// token carries the permission instead.
    #[tokio::test]
    async fn the_product_service_can_read_any_assignment_in_its_tenant() {
        let (svc, id, _, _) = reader_fixture();
        let seen = svc
            .position_for_assignment_as(TENANT, PositionReader::Service, id)
            .await
            .unwrap();
        assert!(seen.is_some());
    }

    /// Indistinguishable from "not yours": a caller must not be able to use the
    /// response to learn which assignment ids are real.
    #[tokio::test]
    async fn an_unknown_assignment_reads_the_same_as_a_forbidden_one() {
        let (svc, _, holder, _) = reader_fixture();
        let seen = svc
            .position_for_assignment_as(TENANT, PositionReader::Courier(holder), Uuid::new_v4())
            .await
            .unwrap();
        assert!(seen.is_none());
    }
    use crate::domain::entities::{Courier, CourierLocation};
    use crate::domain::repositories::CourierRepository;
    use crate::infrastructure::db::{ClaimOutcome, CourierLedgerRepository, LocationRepository};
    use std::sync::Mutex;

    const TENANT: Uuid = Uuid::from_u128(1);

    struct NoCouriers;
    #[async_trait::async_trait]
    impl CourierRepository for NoCouriers {
        async fn find_by_id(&self, _: Uuid, _: Uuid) -> anyhow::Result<Option<Courier>> { Ok(None) }
        async fn find_by_user(&self, _: Uuid, _: Uuid) -> anyhow::Result<Option<Courier>> { Ok(None) }
        async fn save(&self, _: &Courier) -> anyhow::Result<()> { Ok(()) }
        async fn find_available_near(&self, _: Uuid, _: f64, _: f64, _: f64, _: i64)
            -> anyhow::Result<Vec<Courier>> { Ok(vec![]) }
        /// Not modelled by this fake — the roster is asserted in `courier_admin`.
        async fn list_for_tenant(&self, _: Uuid, _: i64, _: i64)
            -> anyhow::Result<Vec<Courier>> { Ok(vec![]) }
    }

    struct NoLedgers;
    #[async_trait::async_trait]
    impl CourierLedgerRepository for NoLedgers {
        async fn find_open(&self, _: Uuid, _: Uuid, _: &str)
            -> anyhow::Result<Option<CourierLedger>> { Ok(None) }
        async fn save(&self, _: &CourierLedger) -> anyhow::Result<()> { Ok(()) }
        async fn find_all_open(&self, _: &str) -> anyhow::Result<Vec<CourierLedger>> { Ok(vec![]) }
        async fn entry_exists_for_job(&self, _: Uuid, _: Uuid, _: Uuid) -> anyhow::Result<bool> { Ok(false) }
    }

    #[derive(Default)]
    struct Assignments(Mutex<Vec<CourierAssignment>>);
    #[async_trait::async_trait]
    impl AssignmentRepository for Assignments {
        async fn save(&self, _: &CourierAssignment) -> anyhow::Result<()> { Ok(()) }
        async fn try_claim(&self, _: Uuid, _: Uuid) -> anyhow::Result<ClaimOutcome> { Ok(ClaimOutcome::Lost) }
        async fn find_by_id(&self, _: Uuid, id: Uuid) -> anyhow::Result<Option<CourierAssignment>> {
            Ok(self.0.lock().unwrap().iter().find(|a| a.id == id).cloned())
        }
        async fn find_offered_for_courier(&self, _: Uuid, _: Uuid)
            -> anyhow::Result<Vec<CourierAssignment>> { Ok(vec![]) }
        /// This fake does not model the fan-out, so there is nothing to retire.
        /// Explicit rather than a trait default: a default would silently do
        /// nothing for a real repository that forgot to implement it.
        async fn expire_other_offers(
            &self,
            _: Uuid,
            _: &ProductKey,
            _: Uuid,
            _: Uuid,
        ) -> anyhow::Result<u64> { Ok(0) }
    }

    /// Empty for every courier — the "no fix on record" case.
    struct NoFixes;
    #[async_trait::async_trait]
    impl LocationRepository for NoFixes {
        async fn record(&self, _: &CourierLocation) -> anyhow::Result<()> { Ok(()) }
        async fn latest(&self, _: Uuid, _: Uuid) -> anyhow::Result<Option<CourierLocation>> { Ok(None) }
        async fn recent(&self, _: Uuid, _: Uuid, _: i64) -> anyhow::Result<Vec<CourierLocation>> { Ok(vec![]) }
    }

    /// A fixed, caller-supplied newest-first list for any courier — enough to
    /// prove the method hands back `recent()`'s first element verbatim rather
    /// than recomputing anything of its own.
    struct WithFixes(Vec<CourierLocation>);
    #[async_trait::async_trait]
    impl LocationRepository for WithFixes {
        async fn record(&self, _: &CourierLocation) -> anyhow::Result<()> { Ok(()) }
        async fn latest(&self, _: Uuid, _: Uuid) -> anyhow::Result<Option<CourierLocation>> {
            Ok(self.0.first().cloned())
        }
        async fn recent(&self, _: Uuid, _: Uuid, _: i64) -> anyhow::Result<Vec<CourierLocation>> {
            Ok(self.0.clone())
        }
    }

    fn assignment() -> CourierAssignment {
        CourierAssignment::offer_with_earnings(
            TENANT, Uuid::new_v4(), ProductKey::new("omnideliv".to_string()),
            Uuid::new_v4(), 3_500, 0, 0,
        )
    }

    fn fix(courier_id: Uuid) -> CourierLocation {
        CourierLocation::new(TENANT, courier_id, 14.5995, 120.9842, None)
    }

    fn svc(assignments: Arc<Assignments>, locations: Arc<dyn LocationRepository>) -> DispatchService {
        DispatchService::new(
            Arc::new(NoCouriers), assignments, locations, Arc::new(NoLedgers),
            Arc::new(crate::infrastructure::messaging::NoopCourierEvents),
            PayBounds::default(),
        )
    }

    /// A guessed id must read exactly like an unknown one — see the doc comment
    /// on `position_for_assignment` for why that is deliberate.
    #[tokio::test]
    async fn an_unknown_assignment_id_is_none() {
        let svc = svc(Arc::new(Assignments::default()), Arc::new(NoFixes));
        assert!(svc.position_for_assignment(TENANT, Uuid::new_v4()).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn a_known_assignment_whose_courier_has_no_fix_is_none() {
        let a = assignment();
        let assignments = Arc::new(Assignments::default());
        assignments.0.lock().unwrap().push(a.clone());

        let svc = svc(assignments, Arc::new(NoFixes));
        assert!(svc.position_for_assignment(TENANT, a.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn a_known_assignment_resolves_to_its_couriers_newest_fix() {
        let a = assignment();
        let assignments = Arc::new(Assignments::default());
        assignments.0.lock().unwrap().push(a.clone());

        let newest = fix(a.courier_id);
        let older = fix(a.courier_id);
        let locations: Arc<dyn LocationRepository> = Arc::new(WithFixes(vec![newest.clone(), older]));

        let (courier_id, loc, _) = svc(assignments, locations)
            .position_for_assignment(TENANT, a.id)
            .await
            .unwrap()
            .expect("a known assignment with fixes on record must resolve");

        assert_eq!(courier_id, a.courier_id, "must be the assignment's courier, never trusted from elsewhere");
        assert_eq!(loc.id, newest.id, "must be recent()'s first element — the newest fix");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Milestone authorization
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod milestone_authorization {
    use super::*;
    use crate::domain::entities::{AssignmentStatus, Courier, CourierLocation};
    use crate::domain::repositories::CourierRepository;
    use crate::infrastructure::db::{ClaimOutcome, LocationRepository};
    use std::sync::Mutex;

    const TENANT: Uuid = Uuid::from_u128(1);

    struct Couriers { by_user: Vec<(Uuid, Courier)> }

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
        /// Not modelled by this fake — the roster is asserted in `courier_admin`.
        async fn list_for_tenant(&self, _: Uuid, _: i64, _: i64)
            -> anyhow::Result<Vec<Courier>> { Ok(vec![]) }
    }

    #[derive(Default)]
    struct Assignments { rows: Mutex<Vec<CourierAssignment>> }

    #[async_trait::async_trait]
    impl AssignmentRepository for Assignments {
        async fn save(&self, a: &CourierAssignment) -> anyhow::Result<()> {
            let mut rows = self.rows.lock().unwrap();
            if let Some(row) = rows.iter_mut().find(|r| r.id == a.id) { *row = a.clone(); }
            Ok(())
        }
        async fn try_claim(&self, _: Uuid, _: Uuid) -> anyhow::Result<ClaimOutcome> {
            Ok(ClaimOutcome::Lost)
        }
        async fn find_by_id(&self, _: Uuid, id: Uuid) -> anyhow::Result<Option<CourierAssignment>> {
            Ok(self.rows.lock().unwrap().iter().find(|a| a.id == id).cloned())
        }
        async fn find_offered_for_courier(&self, _: Uuid, _: Uuid)
            -> anyhow::Result<Vec<CourierAssignment>> { Ok(vec![]) }
        /// This fake does not model the fan-out, so there is nothing to retire.
        /// Explicit rather than a trait default: a default would silently do
        /// nothing for a real repository that forgot to implement it.
        async fn expire_other_offers(
            &self,
            _: Uuid,
            _: &ProductKey,
            _: Uuid,
            _: Uuid,
        ) -> anyhow::Result<u64> { Ok(0) }
    }

    struct NoLocations;
    #[async_trait::async_trait]
    impl LocationRepository for NoLocations {
        async fn record(&self, _: &CourierLocation) -> anyhow::Result<()> { Ok(()) }
        async fn latest(&self, _: Uuid, _: Uuid) -> anyhow::Result<Option<CourierLocation>> { Ok(None) }
        async fn recent(&self, _: Uuid, _: Uuid, _: i64) -> anyhow::Result<Vec<CourierLocation>> { Ok(vec![]) }
    }

    /// Records every entry written, so a test can assert nobody was paid.
    #[derive(Default)]
    struct RecordingLedgers { saved: Mutex<Vec<CourierLedger>> }

    #[async_trait::async_trait]
    impl crate::infrastructure::db::CourierLedgerRepository for RecordingLedgers {
        async fn find_open(&self, _: Uuid, _: Uuid, _: &str)
            -> anyhow::Result<Option<CourierLedger>> { Ok(None) }
        async fn save(&self, ledger: &CourierLedger) -> anyhow::Result<()> {
            self.saved.lock().unwrap().push(ledger.clone());
            Ok(())
        }
        async fn find_all_open(&self, _: &str) -> anyhow::Result<Vec<CourierLedger>> { Ok(vec![]) }
        async fn entry_exists_for_job(&self, _: Uuid, _: Uuid, _: Uuid) -> anyhow::Result<bool> {
            Ok(false)
        }
    }

    /// Records which milestones reached the broker.
    #[derive(Default)]
    struct RecordingEvents { emitted: Mutex<Vec<&'static str>> }

    #[async_trait::async_trait]
    impl crate::infrastructure::messaging::CourierEvents for RecordingEvents {
        async fn publish(&self, e: &CourierEvent) -> anyhow::Result<()> {
            self.emitted.lock().unwrap().push(match e {
                CourierEvent::Assigned  { .. } => "assigned",
                CourierEvent::Arrived   { .. } => "arrived",
                CourierEvent::Collected { .. } => "collected",
                CourierEvent::Delivered { .. } => "delivered",
            });
            Ok(())
        }
    }

    fn courier() -> Courier {
        Courier::new(TENANT, Uuid::new_v4(), "A".into(), "B".into(), "+63".into())
    }

    /// One job, one courier it is addressed to, one who is not.
    ///
    /// A struct rather than a tuple because the status matters as much as the
    /// identity here, and a positional 8-tuple destructured eight ways is how a
    /// test ends up asserting against the wrong uuid.
    struct Fixture {
        svc:         DispatchService,
        assignment:  Uuid,
        /// The identity user the assignment is addressed to.
        holder:      Uuid,
        /// That user's *courier* id — a different uuid, which is the whole
        /// point of the two-hop lookup being tested.
        holder_courier: Uuid,
        other:       Uuid,
        assignments: Arc<Assignments>,
        ledgers:     Arc<RecordingLedgers>,
        events:      Arc<RecordingEvents>,
    }

    /// The normal case: the courier claimed the job and is working it.
    fn fixture() -> Fixture { fixture_in(AssignmentStatus::Claimed) }

    /// The same job in a chosen status.
    ///
    /// `Offered` is not hypothetical: `offer_to_nearest` addresses one job to
    /// `fanout` couriers and only the winner's row is ever claimed, so at any
    /// moment most assignments in this state are held by couriers who did not
    /// get the job and can still read their id from `/assignments/mine`.
    fn fixture_in(status: AssignmentStatus) -> Fixture {
        let holder_user = Uuid::new_v4();
        let other_user  = Uuid::new_v4();
        let holder = courier();
        let other  = courier();
        let holder_courier = holder.id;

        let mut a = CourierAssignment::offer_with_earnings(
            TENANT, holder.id, ProductKey::new("omnideliv".to_string()),
            Uuid::new_v4(), 3_500, 0, 38_900,
        );
        a.status = status;
        let id = a.id;

        let assignments = Arc::new(Assignments::default());
        assignments.rows.lock().unwrap().push(a);

        let ledgers = Arc::new(RecordingLedgers::default());
        let events  = Arc::new(RecordingEvents::default());

        let svc = DispatchService::new(
            Arc::new(Couriers { by_user: vec![(holder_user, holder), (other_user, other)] }),
            assignments.clone(),
            Arc::new(NoLocations),
            ledgers.clone(),
            events.clone(),
            PayBounds::default(),
        );
        Fixture {
            svc, assignment: id, holder: holder_user, holder_courier,
            other: other_user, assignments, ledgers, events,
        }
    }

    #[tokio::test]
    async fn the_holder_can_mark_collected() {
        let f = fixture();
        assert!(f.svc.mark_collected(TENANT, f.holder, f.assignment, Uuid::new_v4(), None)
                     .await.unwrap());
        assert_eq!(*f.events.emitted.lock().unwrap(), vec!["collected"]);
    }

    #[tokio::test]
    async fn the_holder_can_mark_delivered_and_is_the_one_paid() {
        let f = fixture();
        assert!(f.svc.mark_delivered(TENANT, f.holder, f.assignment, None).await.unwrap());
        assert_eq!(*f.events.emitted.lock().unwrap(), vec!["delivered"]);

        let saved = f.ledgers.saved.lock().unwrap();
        assert_eq!(saved.len(), 1);
        // Identity and amount, not just a count. The count is 1 by construction
        // — `find_open` always returns `None` — so it would pass even if the
        // credit landed on the wrong courier or for the wrong sum.
        assert_eq!(saved[0].courier_id, f.holder_courier,
                   "the courier who did the job is the one credited");
        assert_eq!(saved[0].balance_cents, 3_500 - 38_900,
                   "earned 3500, now holding 38900 of the platform's cash");
    }

    /// The assignment ids are handed to the dispatching product, so they are
    /// not secret. Another courier naming one must not be able to collect
    /// against it.
    #[tokio::test]
    async fn another_courier_cannot_mark_collected() {
        let f = fixture();
        assert!(!f.svc.mark_collected(TENANT, f.other, f.assignment, Uuid::new_v4(), None)
                      .await.unwrap());
        assert!(f.events.emitted.lock().unwrap().is_empty(),
                "no milestone may reach the broker for a job the caller does not hold");
    }

    /// The one that moves money: `mark_delivered` completes the assignment,
    /// credits the courier ledger and debits COD.
    #[tokio::test]
    async fn another_courier_cannot_mark_delivered_or_trigger_a_credit() {
        let f = fixture();

        assert!(!f.svc.mark_delivered(TENANT, f.other, f.assignment, None).await.unwrap());
        assert!(f.events.emitted.lock().unwrap().is_empty());
        assert!(f.ledgers.saved.lock().unwrap().is_empty(),
                "an unauthorized delivery must not credit anyone");

        let rows = f.assignments.rows.lock().unwrap();
        assert_eq!(rows[0].status, AssignmentStatus::Claimed,
                   "the assignment must not be completed by a caller who does not hold it");
    }

    #[tokio::test]
    async fn a_user_who_is_not_a_courier_is_refused_both_milestones() {
        let f = fixture();
        let stranger = Uuid::new_v4();
        assert!(!f.svc.mark_collected(TENANT, stranger, f.assignment, Uuid::new_v4(), None)
                      .await.unwrap());
        assert!(!f.svc.mark_delivered(TENANT, stranger, f.assignment, None).await.unwrap());
    }

    /// The fan-out hole. `offer_to_nearest` addresses one job to five couriers,
    /// each row carrying the full `trip_cents`; `try_claim` flips only the
    /// winner's and nothing expires the rest. A loser keeps a readable
    /// assignment id, and identity alone does not distinguish them from the
    /// winner — so without the status gate they could complete a job they never
    /// took, be paid for it, and publish `Delivered`, which advances the
    /// customer's order while the real courier is still carrying it.
    #[tokio::test]
    async fn a_courier_who_only_received_an_offer_cannot_deliver_it() {
        let f = fixture_in(AssignmentStatus::Offered);

        assert!(!f.svc.mark_delivered(TENANT, f.holder, f.assignment, None).await.unwrap());
        assert!(f.events.emitted.lock().unwrap().is_empty(),
                "an unclaimed job must not publish a delivery");
        assert!(f.ledgers.saved.lock().unwrap().is_empty(),
                "being offered a job is not doing it — nobody is paid");
        assert_eq!(f.assignments.rows.lock().unwrap()[0].status, AssignmentStatus::Offered,
                   "the offer must not be completed out from under the claim");
    }

    #[tokio::test]
    async fn a_courier_who_only_received_an_offer_cannot_mark_collected() {
        let f = fixture_in(AssignmentStatus::Offered);
        assert!(!f.svc.mark_collected(TENANT, f.holder, f.assignment, Uuid::new_v4(), None)
                      .await.unwrap());
        assert!(f.events.emitted.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn the_holder_can_mark_arrived() {
        let f = fixture();
        assert!(f.svc.mark_arrived(TENANT, f.holder, f.assignment, Uuid::new_v4(), None)
                     .await.unwrap());
        assert_eq!(*f.events.emitted.lock().unwrap(), vec!["arrived"]);
    }

    #[tokio::test]
    async fn another_courier_cannot_mark_arrived() {
        let f = fixture();
        assert!(!f.svc.mark_arrived(TENANT, f.other, f.assignment, Uuid::new_v4(), None)
                      .await.unwrap());
        assert!(f.events.emitted.lock().unwrap().is_empty());
    }

    /// Same fan-out rule as every other milestone.
    #[tokio::test]
    async fn a_courier_who_only_received_an_offer_cannot_mark_arrived() {
        let f = fixture_in(AssignmentStatus::Offered);
        assert!(!f.svc.mark_arrived(TENANT, f.holder, f.assignment, Uuid::new_v4(), None)
                      .await.unwrap());
        assert!(f.events.emitted.lock().unwrap().is_empty());
    }

    /// An at-least-once client — which the driver app's offline queue is —
    /// retries a delivery whose response was lost. That is not an error: the
    /// milestone already landed. It must be accepted without paying again, and
    /// the duplicate must not reach the broker.
    #[tokio::test]
    async fn redelivering_a_completed_job_is_accepted_without_paying_again() {
        let f = fixture();

        assert!(f.svc.mark_delivered(TENANT, f.holder, f.assignment, None).await.unwrap());
        assert!(f.svc.mark_delivered(TENANT, f.holder, f.assignment, None).await.unwrap(),
                "a retry reports success, so the queue does not park a milestone that landed");

        assert_eq!(f.ledgers.saved.lock().unwrap().len(), 1, "paid exactly once");
        assert_eq!(*f.events.emitted.lock().unwrap(), vec!["delivered"],
                   "the duplicate stops here rather than relying on the consumer to absorb it");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Credit idempotency across a period boundary
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod credit_idempotency {
    use super::*;
    use crate::domain::entities::{AssignmentStatus, Courier, CourierLocation};
    use crate::domain::repositories::CourierRepository;
    use crate::infrastructure::db::{ClaimOutcome, LocationRepository};
    use std::sync::Mutex;

    const TENANT: Uuid = Uuid::from_u128(1);

    struct Couriers { by_user: Vec<(Uuid, Courier)> }

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
        /// Not modelled by this fake — the roster is asserted in `courier_admin`.
        async fn list_for_tenant(&self, _: Uuid, _: i64, _: i64)
            -> anyhow::Result<Vec<Courier>> { Ok(vec![]) }
    }

    #[derive(Default)]
    struct Assignments { rows: Mutex<Vec<CourierAssignment>> }

    #[async_trait::async_trait]
    impl AssignmentRepository for Assignments {
        async fn save(&self, a: &CourierAssignment) -> anyhow::Result<()> {
            let mut rows = self.rows.lock().unwrap();
            if let Some(row) = rows.iter_mut().find(|r| r.id == a.id) { *row = a.clone(); }
            Ok(())
        }
        async fn try_claim(&self, _: Uuid, _: Uuid) -> anyhow::Result<ClaimOutcome> {
            Ok(ClaimOutcome::Lost)
        }
        async fn find_by_id(&self, _: Uuid, id: Uuid) -> anyhow::Result<Option<CourierAssignment>> {
            Ok(self.rows.lock().unwrap().iter().find(|a| a.id == id).cloned())
        }
        async fn find_offered_for_courier(&self, _: Uuid, _: Uuid)
            -> anyhow::Result<Vec<CourierAssignment>> { Ok(vec![]) }
        /// This fake does not model the fan-out, so there is nothing to retire.
        /// Explicit rather than a trait default: a default would silently do
        /// nothing for a real repository that forgot to implement it.
        async fn expire_other_offers(
            &self,
            _: Uuid,
            _: &ProductKey,
            _: Uuid,
            _: Uuid,
        ) -> anyhow::Result<u64> { Ok(0) }
    }

    struct NoLocations;
    #[async_trait::async_trait]
    impl LocationRepository for NoLocations {
        async fn record(&self, _: &CourierLocation) -> anyhow::Result<()> { Ok(()) }
        async fn latest(&self, _: Uuid, _: Uuid) -> anyhow::Result<Option<CourierLocation>> { Ok(None) }
        async fn recent(&self, _: Uuid, _: Uuid, _: i64) -> anyhow::Result<Vec<CourierLocation>> { Ok(vec![]) }
    }

    /// A ledger store that behaves like the real one across a period rollover:
    /// entries persist forever, but `find_open` only ever returns the ledger for
    /// the period it is asked about. That asymmetry is the whole bug — the old
    /// guard scanned the ledger in hand, which after a rollover is empty.
    #[derive(Default)]
    struct PeriodAwareLedgers {
        /// (tenant, courier, kind, external_ref) for every entry ever written.
        all_entries: Mutex<Vec<(Uuid, Uuid, &'static str, Uuid)>>,
        ledgers:     Mutex<Vec<CourierLedger>>,
    }

    #[async_trait::async_trait]
    impl crate::infrastructure::db::CourierLedgerRepository for PeriodAwareLedgers {
        async fn find_open(&self, tenant_id: Uuid, courier_id: Uuid, period: &str)
            -> anyhow::Result<Option<CourierLedger>> {
            Ok(self.ledgers.lock().unwrap().iter()
                .find(|l| l.tenant_id == tenant_id && l.courier_id == courier_id && l.period == period)
                .cloned())
        }
        async fn save(&self, ledger: &CourierLedger) -> anyhow::Result<()> {
            {
                let mut all = self.all_entries.lock().unwrap();
                for e in &ledger.entries {
                    if let Some(r) = e.external_ref {
                        let row = (ledger.tenant_id, ledger.courier_id, e.kind.as_str(), r);
                        if !all.contains(&row) { all.push(row); }
                    }
                }
            }
            let mut ls = self.ledgers.lock().unwrap();
            match ls.iter_mut().find(|l| l.id == ledger.id) {
                Some(existing) => *existing = ledger.clone(),
                None => ls.push(ledger.clone()),
            }
            Ok(())
        }
        async fn find_all_open(&self, _: &str) -> anyhow::Result<Vec<CourierLedger>> { Ok(vec![]) }
        async fn entry_exists_for_job(&self, tenant_id: Uuid, courier_id: Uuid, external_ref: Uuid)
            -> anyhow::Result<bool> {
            Ok(self.all_entries.lock().unwrap().iter()
                .any(|(t, c, _, r)| *t == tenant_id && *c == courier_id && *r == external_ref))
        }
    }

    /// The delivery is credited once. Then the week rolls over — which is all
    /// `current_period()` does — and the same job is credited again, which is
    /// what an offline queue retrying a lost response does.
    #[tokio::test]
    async fn a_retry_across_a_period_boundary_does_not_pay_twice() {
        let user = Uuid::new_v4();
        let courier = Courier::new(TENANT, user, "A".into(), "B".into(), "+63".into());
        let job = Uuid::new_v4();

        let mut a = CourierAssignment::offer_with_earnings(
            TENANT, courier.id, ProductKey::new("omnideliv".to_string()),
            job, 3_500, 0, 38_900,
        );
        a.status = AssignmentStatus::Claimed;

        let assignments = Arc::new(Assignments::default());
        assignments.rows.lock().unwrap().push(a.clone());

        let ledgers = Arc::new(PeriodAwareLedgers::default());
        let svc = DispatchService::new(
            Arc::new(Couriers { by_user: vec![(user, courier.clone())] }),
            assignments.clone(),
            Arc::new(NoLocations),
            ledgers.clone(),
            Arc::new(crate::infrastructure::messaging::NoopCourierEvents),
            PayBounds::default(),
        );

        svc.credit_courier(&a).await.unwrap();

        let after_first: i64 = ledgers.ledgers.lock().unwrap()
            .iter().map(|l| l.balance_cents).sum();
        assert_eq!(after_first, 3_500 - 38_900, "earned 3500, holding 38900 of our cash");

        // The week rolls over: nothing is open for the new period, so the ledger
        // the old guard scanned would come back empty.
        ledgers.ledgers.lock().unwrap()
            .iter_mut().for_each(|l| l.period = "2026-W33".to_string());

        svc.credit_courier(&a).await.unwrap();

        let after_retry: i64 = ledgers.ledgers.lock().unwrap()
            .iter().map(|l| l.balance_cents).sum();
        assert_eq!(after_retry, after_first,
                   "a retried delivery must not credit the trip or re-debit the COD");

        let trips = ledgers.all_entries.lock().unwrap().iter()
            .filter(|(_, _, kind, r)| *kind == "trip_earning" && *r == job).count();
        assert_eq!(trips, 1, "exactly one trip earning for one job, ever");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Availability
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod availability {
    use super::*;
    use crate::domain::entities::{Courier, CourierLocation, CourierStatus};
    use crate::domain::repositories::CourierRepository;
    use crate::infrastructure::db::{ClaimOutcome, CourierLedgerRepository, LocationRepository};
    use std::sync::Mutex;

    const TENANT: Uuid = Uuid::from_u128(7);

    struct Couriers(Mutex<Vec<(Uuid, Courier)>>);

    #[async_trait::async_trait]
    impl CourierRepository for Couriers {
        async fn find_by_id(&self, _: Uuid, id: Uuid) -> anyhow::Result<Option<Courier>> {
            Ok(self.0.lock().unwrap().iter().find(|(_, c)| c.id == id).map(|(_, c)| c.clone()))
        }
        async fn find_by_user(&self, _: Uuid, user_id: Uuid) -> anyhow::Result<Option<Courier>> {
            Ok(self.0.lock().unwrap().iter().find(|(u, _)| *u == user_id).map(|(_, c)| c.clone()))
        }
        async fn save(&self, courier: &Courier) -> anyhow::Result<()> {
            let mut rows = self.0.lock().unwrap();
            if let Some((_, slot)) = rows.iter_mut().find(|(_, c)| c.id == courier.id) {
                *slot = courier.clone();
            }
            Ok(())
        }
        async fn find_available_near(&self, _: Uuid, _: f64, _: f64, _: f64, _: i64)
            -> anyhow::Result<Vec<Courier>> {
            Ok(self.0.lock().unwrap().iter()
                .filter(|(_, c)| c.is_dispatchable())
                .map(|(_, c)| c.clone())
                .collect())
        }
        /// Not modelled by this fake — the roster is asserted in `courier_admin`.
        async fn list_for_tenant(&self, _: Uuid, _: i64, _: i64)
            -> anyhow::Result<Vec<Courier>> { Ok(vec![]) }
    }

    struct NoAssignments;
    #[async_trait::async_trait]
    impl AssignmentRepository for NoAssignments {
        async fn save(&self, _: &CourierAssignment) -> anyhow::Result<()> { Ok(()) }
        async fn try_claim(&self, _: Uuid, _: Uuid) -> anyhow::Result<ClaimOutcome> {
            Ok(ClaimOutcome::Lost)
        }
        async fn find_by_id(&self, _: Uuid, _: Uuid) -> anyhow::Result<Option<CourierAssignment>> {
            Ok(None)
        }
        async fn find_offered_for_courier(&self, _: Uuid, _: Uuid)
            -> anyhow::Result<Vec<CourierAssignment>> { Ok(vec![]) }
        /// This fake does not model the fan-out, so there is nothing to retire.
        /// Explicit rather than a trait default: a default would silently do
        /// nothing for a real repository that forgot to implement it.
        async fn expire_other_offers(
            &self,
            _: Uuid,
            _: &ProductKey,
            _: Uuid,
            _: Uuid,
        ) -> anyhow::Result<u64> { Ok(0) }
    }

    struct NoLocations;
    #[async_trait::async_trait]
    impl LocationRepository for NoLocations {
        async fn record(&self, _: &CourierLocation) -> anyhow::Result<()> { Ok(()) }
        async fn latest(&self, _: Uuid, _: Uuid) -> anyhow::Result<Option<CourierLocation>> { Ok(None) }
        async fn recent(&self, _: Uuid, _: Uuid, _: i64) -> anyhow::Result<Vec<CourierLocation>> { Ok(vec![]) }
    }

    struct NoLedgers;
    #[async_trait::async_trait]
    impl CourierLedgerRepository for NoLedgers {
        async fn find_open(&self, _: Uuid, _: Uuid, _: &str)
            -> anyhow::Result<Option<CourierLedger>> { Ok(None) }
        async fn save(&self, _: &CourierLedger) -> anyhow::Result<()> { Ok(()) }
        async fn find_all_open(&self, _: &str) -> anyhow::Result<Vec<CourierLedger>> { Ok(vec![]) }
        async fn entry_exists_for_job(&self, _: Uuid, _: Uuid, _: Uuid) -> anyhow::Result<bool> { Ok(false) }
    }

    /// A freshly registered courier, exactly as `register` leaves them: offline.
    fn fixture() -> (DispatchService, Arc<Couriers>, Uuid) {
        let user = Uuid::new_v4();
        let courier = Courier::new(
            TENANT, user, "Ave".into(), "Test".into(), "+639170000009".into(),
        );
        let couriers = Arc::new(Couriers(Mutex::new(vec![(user, courier)])));
        let svc = DispatchService::new(
            couriers.clone(),
            Arc::new(NoAssignments),
            Arc::new(NoLocations),
            Arc::new(NoLedgers),
            Arc::new(crate::infrastructure::messaging::NoopCourierEvents),
            PayBounds::default(),
        );
        (svc, couriers, user)
    }

    fn only(couriers: &Couriers) -> Courier {
        couriers.0.lock().unwrap()[0].1.clone()
    }

    /// The whole point. `register` leaves a courier `offline` and, before this
    /// existed, nothing in the service ever moved them off it — so
    /// `find_available_near` skipped every courier in the tenant and every
    /// order fanned out to nobody.
    #[tokio::test]
    async fn going_on_duty_puts_the_courier_into_supply() {
        let (svc, couriers, user) = fixture();
        assert!(!only(&couriers).is_dispatchable(), "registration must not opt anyone in");

        assert!(svc.set_availability(TENANT, user, true).await.unwrap());

        assert!(only(&couriers).is_dispatchable());
        assert_eq!(
            svc.couriers.find_available_near(TENANT, 0.0, 0.0, 5.0, 5).await.unwrap().len(),
            1,
            "a courier on duty is what a proximity search is supposed to find",
        );
    }

    #[tokio::test]
    async fn going_off_duty_takes_them_out_of_supply() {
        let (svc, couriers, user) = fixture();
        svc.set_availability(TENANT, user, true).await.unwrap();

        assert!(svc.set_availability(TENANT, user, false).await.unwrap());

        assert_eq!(only(&couriers).status, CourierStatus::Offline);
        assert!(svc.couriers.find_available_near(TENANT, 0.0, 0.0, 5.0, 5).await.unwrap().is_empty());
    }

    /// The app calls this every time the toggle is flipped, and a courier
    /// re-opening the app while already on duty must not be an error.
    #[tokio::test]
    async fn going_on_duty_twice_is_not_an_error() {
        let (svc, couriers, user) = fixture();

        assert!(svc.set_availability(TENANT, user, true).await.unwrap());
        assert!(svc.set_availability(TENANT, user, true).await.unwrap());

        assert!(only(&couriers).is_dispatchable());
    }

    /// A deactivated courier is deactivated by someone who meant it. Going on
    /// duty is a courier's own decision about their shift and must not
    /// undo it.
    #[tokio::test]
    async fn a_deactivated_courier_stays_out_of_supply() {
        let (svc, couriers, user) = fixture();
        couriers.0.lock().unwrap()[0].1.is_active = false;

        svc.set_availability(TENANT, user, true).await.unwrap();

        assert!(!only(&couriers).is_dispatchable());
        assert!(svc.couriers.find_available_near(TENANT, 0.0, 0.0, 5.0, 5).await.unwrap().is_empty());
    }

    /// Same shape as `claim` and `offers_for_user`: the caller is resolved from
    /// the token, and a user with no courier row in this tenant is simply not a
    /// courier. `false` so the route can answer 404 without leaking whether the
    /// id exists.
    #[tokio::test]
    async fn a_user_who_is_not_a_courier_is_refused() {
        let (svc, _, _) = fixture();
        assert!(!svc.set_availability(TENANT, Uuid::new_v4(), true).await.unwrap());
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Losing offers
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod losing_offers {
    use super::*;
    use crate::domain::entities::{AssignmentStatus, Courier, CourierLocation};
    use crate::domain::repositories::CourierRepository;
    use crate::infrastructure::db::{ClaimOutcome, CourierLedgerRepository, LocationRepository};
    use std::sync::Mutex;

    const TENANT: Uuid = Uuid::from_u128(11);

    struct Couriers(Vec<(Uuid, Courier)>);

    #[async_trait::async_trait]
    impl CourierRepository for Couriers {
        async fn find_by_id(&self, _: Uuid, id: Uuid) -> anyhow::Result<Option<Courier>> {
            Ok(self.0.iter().find(|(_, c)| c.id == id).map(|(_, c)| c.clone()))
        }
        async fn find_by_user(&self, _: Uuid, user_id: Uuid) -> anyhow::Result<Option<Courier>> {
            Ok(self.0.iter().find(|(u, _)| *u == user_id).map(|(_, c)| c.clone()))
        }
        async fn save(&self, _: &Courier) -> anyhow::Result<()> { Ok(()) }
        async fn find_available_near(&self, _: Uuid, _: f64, _: f64, _: f64, _: i64)
            -> anyhow::Result<Vec<Courier>> { Ok(vec![]) }
        /// Not modelled by this fake — the roster is asserted in `courier_admin`.
        async fn list_for_tenant(&self, _: Uuid, _: i64, _: i64)
            -> anyhow::Result<Vec<Courier>> { Ok(vec![]) }
    }

    #[derive(Default)]
    struct Assignments(Mutex<Vec<CourierAssignment>>);

    #[async_trait::async_trait]
    impl AssignmentRepository for Assignments {
        async fn save(&self, a: &CourierAssignment) -> anyhow::Result<()> {
            let mut rows = self.0.lock().unwrap();
            match rows.iter_mut().find(|r| r.id == a.id) {
                Some(slot) => *slot = a.clone(),
                None => rows.push(a.clone()),
            }
            Ok(())
        }
        async fn try_claim(&self, _: Uuid, id: Uuid) -> anyhow::Result<ClaimOutcome> {
            let mut rows = self.0.lock().unwrap();
            // Mirrors the partial unique index: one live claim per courier.
            let holder = rows.iter().find(|r| r.id == id).map(|r| r.courier_id);
            if let Some(courier) = holder {
                if rows.iter().any(|r| r.courier_id == courier && r.status == AssignmentStatus::Claimed) {
                    return Ok(ClaimOutcome::Lost);
                }
            }
            match rows.iter_mut().find(|r| r.id == id && r.status == AssignmentStatus::Offered) {
                Some(r) => { r.status = AssignmentStatus::Claimed; Ok(ClaimOutcome::Won) }
                None => Ok(ClaimOutcome::Lost),
            }
        }
        async fn find_by_id(&self, _: Uuid, id: Uuid) -> anyhow::Result<Option<CourierAssignment>> {
            Ok(self.0.lock().unwrap().iter().find(|r| r.id == id).cloned())
        }
        async fn find_offered_for_courier(&self, _: Uuid, courier_id: Uuid)
            -> anyhow::Result<Vec<CourierAssignment>> {
            Ok(self.0.lock().unwrap().iter()
                .filter(|r| r.courier_id == courier_id && r.status == AssignmentStatus::Offered)
                .cloned().collect())
        }
        async fn expire_other_offers(
            &self,
            _: Uuid,
            product: &ProductKey,
            external_ref: Uuid,
            winner: Uuid,
        ) -> anyhow::Result<u64> {
            let mut rows = self.0.lock().unwrap();
            let mut n = 0;
            for r in rows.iter_mut() {
                if r.id != winner
                    && r.external_ref == external_ref
                    && r.product.as_str() == product.as_str()
                    && r.status == AssignmentStatus::Offered
                {
                    r.status = AssignmentStatus::Expired;
                    n += 1;
                }
            }
            Ok(n)
        }
    }

    struct NoLocations;
    #[async_trait::async_trait]
    impl LocationRepository for NoLocations {
        async fn record(&self, _: &CourierLocation) -> anyhow::Result<()> { Ok(()) }
        async fn latest(&self, _: Uuid, _: Uuid) -> anyhow::Result<Option<CourierLocation>> { Ok(None) }
        async fn recent(&self, _: Uuid, _: Uuid, _: i64) -> anyhow::Result<Vec<CourierLocation>> { Ok(vec![]) }
    }

    struct NoLedgers;
    #[async_trait::async_trait]
    impl CourierLedgerRepository for NoLedgers {
        async fn find_open(&self, _: Uuid, _: Uuid, _: &str)
            -> anyhow::Result<Option<CourierLedger>> { Ok(None) }
        async fn save(&self, _: &CourierLedger) -> anyhow::Result<()> { Ok(()) }
        async fn find_all_open(&self, _: &str) -> anyhow::Result<Vec<CourierLedger>> { Ok(vec![]) }
        async fn entry_exists_for_job(&self, _: Uuid, _: Uuid, _: Uuid) -> anyhow::Result<bool> { Ok(false) }
    }

    /// One job fanned out to five couriers, exactly as `offer_to_nearest` does.
    fn fanout() -> (DispatchService, Arc<Assignments>, Vec<(Uuid, Uuid)>) {
        let job = Uuid::new_v4();
        let product = ProductKey::new("omnideliv");
        let assignments = Arc::new(Assignments::default());
        let mut couriers = Vec::new();
        let mut ids = Vec::new();

        for i in 0..5u128 {
            let user = Uuid::from_u128(100 + i);
            let courier = Courier::new(
                TENANT, user, format!("C{i}"), "Test".into(), format!("+639170000{i:03}"),
            );
            let a = CourierAssignment::offer_with_earnings(
                TENANT, courier.id, product.clone(), job, 3_500, 0, 0,
            );
            assignments.0.lock().unwrap().push(a.clone());
            ids.push((user, a.id));
            couriers.push((user, courier));
        }

        let svc = DispatchService::new(
            Arc::new(Couriers(couriers)),
            assignments.clone(),
            Arc::new(NoLocations),
            Arc::new(NoLedgers),
            Arc::new(crate::infrastructure::messaging::NoopCourierEvents),
            PayBounds::default(),
        );
        (svc, assignments, ids)
    }

    /// `offer_to_nearest` fans out to five couriers and `try_claim` flips only
    /// the winner. Nothing ever expired the other four, so they sat `Offered`
    /// forever and kept appearing in their couriers' inboxes — a job the app
    /// polls every six seconds, renders, and can only ever answer "That job was
    /// taken" for.
    #[tokio::test]
    async fn claiming_a_job_expires_the_offers_made_to_everyone_else() {
        let (svc, rows, ids) = fanout();
        let (winner_user, winner_id) = ids[0];

        assert!(svc.claim(TENANT, winner_user, winner_id).await.unwrap());

        let stored = rows.0.lock().unwrap();
        let winner = stored.iter().find(|r| r.id == winner_id).unwrap();
        assert_eq!(winner.status, AssignmentStatus::Claimed);

        for (_, id) in ids.iter().skip(1) {
            let loser = stored.iter().find(|r| r.id == *id).unwrap();
            assert_eq!(
                loser.status,
                AssignmentStatus::Expired,
                "a losing offer must not stay in its courier's inbox",
            );
        }
    }

    /// The inbox is what a courier actually sees.
    #[tokio::test]
    async fn a_losing_courier_sees_no_offer_afterwards() {
        let (svc, _, ids) = fanout();
        let (winner_user, winner_id) = ids[0];
        let (loser_user, _) = ids[1];

        assert!(!svc.offers_for_user(TENANT, loser_user).await.unwrap().is_empty());
        svc.claim(TENANT, winner_user, winner_id).await.unwrap();

        assert!(
            svc.offers_for_user(TENANT, loser_user).await.unwrap().is_empty(),
            "the offer is gone, so the app stops showing a job nobody can take",
        );
    }

    /// Only this job's siblings. Expiring by courier rather than by job would
    /// empty every inbox in the tenant on any claim.
    #[tokio::test]
    async fn an_unrelated_offer_is_left_alone() {
        let (svc, rows, ids) = fanout();
        let (winner_user, winner_id) = ids[0];
        let (loser_user, _) = ids[1];

        let other_courier = rows.0.lock().unwrap()[1].courier_id;
        let other = CourierAssignment::offer_with_earnings(
            TENANT, other_courier, ProductKey::new("omnideliv"), Uuid::new_v4(), 4_000, 0, 0,
        );
        rows.0.lock().unwrap().push(other.clone());

        svc.claim(TENANT, winner_user, winner_id).await.unwrap();

        let remaining = svc.offers_for_user(TENANT, loser_user).await.unwrap();
        assert_eq!(remaining.len(), 1, "the unrelated job must survive");
        assert_eq!(remaining[0].id, other.id);
    }

    /// Two couriers must not both hold one job — and before this change they
    /// could.
    ///
    /// `try_claim` is a CAS on `id` (`WHERE id = $1 AND status = 'offered'`)
    /// and the only unique index is per **courier**, not per job. So each of
    /// the five couriers in a fan-out could claim their *own* sibling row and
    /// every one of them would win: five `Claimed` rows, five
    /// `CourierEvent::Assigned` for the same `external_ref`, and five couriers
    /// whose milestone calls all pass the status gate, because their row really
    /// is `Claimed`.
    ///
    /// The status gate from the hardening work stopped a *loser* posting
    /// milestones on an `Offered` row. It could not help here: a second claimer
    /// was not a loser, they held a claim of their own.
    ///
    /// Retiring the siblings at the moment of the win is what leaves the second
    /// claim no `offered` row to swap.
    #[tokio::test]
    async fn a_second_courier_cannot_claim_a_job_already_taken() {
        let (svc, rows, ids) = fanout();
        let (first_user, first_id) = ids[0];
        let (second_user, second_id) = ids[1];

        assert!(svc.claim(TENANT, first_user, first_id).await.unwrap());
        assert!(
            !svc.claim(TENANT, second_user, second_id).await.unwrap(),
            "a job already held must not be claimable through a sibling offer",
        );

        let stored = rows.0.lock().unwrap();
        assert_eq!(
            stored.iter().filter(|r| r.status == AssignmentStatus::Claimed).count(),
            1,
            "exactly one courier may hold a job",
        );
        assert_eq!(
            stored.iter().find(|r| r.id == first_id).unwrap().status,
            AssignmentStatus::Claimed,
            "and it is the one who actually won it",
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Courier administration
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod courier_admin {
    use super::*;
    use crate::domain::entities::{Courier, CourierLocation, CourierStatus};
    use crate::domain::repositories::CourierRepository;
    use crate::infrastructure::db::{ClaimOutcome, CourierLedgerRepository, LocationRepository};
    use std::sync::Mutex;

    const TENANT: Uuid = Uuid::from_u128(21);
    const OTHER_TENANT: Uuid = Uuid::from_u128(22);

    struct Couriers(Mutex<Vec<Courier>>);

    #[async_trait::async_trait]
    impl CourierRepository for Couriers {
        async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<Option<Courier>> {
            Ok(self.0.lock().unwrap().iter()
                .find(|c| c.id == id && c.tenant_id == tenant_id).cloned())
        }
        async fn find_by_user(&self, tenant_id: Uuid, user_id: Uuid) -> anyhow::Result<Option<Courier>> {
            Ok(self.0.lock().unwrap().iter()
                .find(|c| c.user_id == user_id && c.tenant_id == tenant_id).cloned())
        }
        async fn save(&self, courier: &Courier) -> anyhow::Result<()> {
            let mut rows = self.0.lock().unwrap();
            match rows.iter_mut().find(|c| c.id == courier.id) {
                Some(slot) => *slot = courier.clone(),
                None => rows.push(courier.clone()),
            }
            Ok(())
        }
        async fn find_available_near(&self, _: Uuid, _: f64, _: f64, _: f64, _: i64)
            -> anyhow::Result<Vec<Courier>> { Ok(vec![]) }
        async fn list_for_tenant(&self, tenant_id: Uuid, limit: i64, offset: i64)
            -> anyhow::Result<Vec<Courier>> {
            Ok(self.0.lock().unwrap().iter()
                .filter(|c| c.tenant_id == tenant_id)
                .skip(offset as usize)
                .take(limit as usize)
                .cloned()
                .collect())
        }
    }

    struct NoAssignments;
    #[async_trait::async_trait]
    impl AssignmentRepository for NoAssignments {
        async fn save(&self, _: &CourierAssignment) -> anyhow::Result<()> { Ok(()) }
        async fn try_claim(&self, _: Uuid, _: Uuid) -> anyhow::Result<ClaimOutcome> { Ok(ClaimOutcome::Lost) }
        async fn find_by_id(&self, _: Uuid, _: Uuid) -> anyhow::Result<Option<CourierAssignment>> { Ok(None) }
        async fn find_offered_for_courier(&self, _: Uuid, _: Uuid)
            -> anyhow::Result<Vec<CourierAssignment>> { Ok(vec![]) }
        async fn expire_other_offers(&self, _: Uuid, _: &ProductKey, _: Uuid, _: Uuid)
            -> anyhow::Result<u64> { Ok(0) }
    }

    struct NoLocations;
    #[async_trait::async_trait]
    impl LocationRepository for NoLocations {
        async fn record(&self, _: &CourierLocation) -> anyhow::Result<()> { Ok(()) }
        async fn latest(&self, _: Uuid, _: Uuid) -> anyhow::Result<Option<CourierLocation>> { Ok(None) }
        async fn recent(&self, _: Uuid, _: Uuid, _: i64) -> anyhow::Result<Vec<CourierLocation>> { Ok(vec![]) }
    }

    struct NoLedgers;
    #[async_trait::async_trait]
    impl CourierLedgerRepository for NoLedgers {
        async fn find_open(&self, _: Uuid, _: Uuid, _: &str)
            -> anyhow::Result<Option<CourierLedger>> { Ok(None) }
        async fn save(&self, _: &CourierLedger) -> anyhow::Result<()> { Ok(()) }
        async fn find_all_open(&self, _: &str) -> anyhow::Result<Vec<CourierLedger>> { Ok(vec![]) }
        async fn entry_exists_for_job(&self, _: Uuid, _: Uuid, _: Uuid) -> anyhow::Result<bool> { Ok(false) }
    }

    fn fixture() -> (DispatchService, Arc<Couriers>, Uuid) {
        let mut mine = Courier::new(
            TENANT, Uuid::new_v4(), "Ana".into(), "Cruz".into(), "+639170000021".into(),
        );
        mine.go_available();
        let id = mine.id;

        // A courier belonging to somebody else entirely. field-ops has no
        // row-level security — the tenant bound into each query is the whole of
        // the isolation — so every list has to prove it.
        let theirs = Courier::new(
            OTHER_TENANT, Uuid::new_v4(), "Ben".into(), "Reyes".into(), "+639170000022".into(),
        );

        let couriers = Arc::new(Couriers(Mutex::new(vec![mine, theirs])));
        let svc = DispatchService::new(
            couriers.clone(),
            Arc::new(NoAssignments),
            Arc::new(NoLocations),
            Arc::new(NoLedgers),
            Arc::new(crate::infrastructure::messaging::NoopCourierEvents),
            PayBounds::default(),
        );
        (svc, couriers, id)
    }

    #[tokio::test]
    async fn ops_can_list_the_couriers_in_their_tenant() {
        let (svc, _, id) = fixture();

        let listed = svc.list_couriers(TENANT, 50, 0).await.unwrap();

        assert_eq!(listed.len(), 1, "another tenant's courier must never appear");
        assert_eq!(listed[0].id, id);
    }

    /// Suspension is ops' lever, and it is deliberately not the courier's own
    /// duty toggle: `is_dispatchable` requires **both**, so a suspended courier
    /// can flip themselves on duty all day and still never be offered a job.
    #[tokio::test]
    async fn suspending_a_courier_takes_them_out_of_dispatch_without_touching_duty() {
        let (svc, couriers, id) = fixture();

        assert!(svc.set_courier_active(TENANT, id, false).await.unwrap());

        let stored = couriers.0.lock().unwrap().iter().find(|c| c.id == id).unwrap().clone();
        assert!(!stored.is_active);
        assert!(!stored.is_dispatchable(), "a suspended courier is not dispatchable");
        assert_eq!(
            stored.status,
            CourierStatus::Available,
            "their own duty flag is theirs; ops suspends, it does not clock them off",
        );
    }

    #[tokio::test]
    async fn reinstating_puts_a_courier_back_into_dispatch() {
        let (svc, couriers, id) = fixture();
        svc.set_courier_active(TENANT, id, false).await.unwrap();

        assert!(svc.set_courier_active(TENANT, id, true).await.unwrap());

        let stored = couriers.0.lock().unwrap().iter().find(|c| c.id == id).unwrap().clone();
        assert!(stored.is_dispatchable());
    }

    /// The tenant comes from the validated JWT, and this service has no
    /// row-level security — so an id from another tenant must read as absent,
    /// not as forbidden, exactly like every other route here.
    #[tokio::test]
    async fn a_courier_from_another_tenant_cannot_be_suspended() {
        let (svc, couriers, _) = fixture();
        let theirs = couriers.0.lock().unwrap()
            .iter().find(|c| c.tenant_id == OTHER_TENANT).unwrap().id;

        assert!(!svc.set_courier_active(TENANT, theirs, false).await.unwrap());

        assert!(
            couriers.0.lock().unwrap().iter().find(|c| c.id == theirs).unwrap().is_active,
            "the other tenant's courier must be untouched",
        );
    }
}
