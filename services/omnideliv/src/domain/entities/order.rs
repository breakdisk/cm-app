//! Orders and three-leg settlement.
//!
//! All money is integer cents. No floats appear anywhere in this module — a
//! rounding error here is money created or destroyed, and `f64` cannot
//! represent a cent exactly.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    Placed,
    AwaitingCourier,
    Collecting,
    Delivering,
    Delivered,
    Cancelled,
}

impl OrderStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderStatus::Placed          => "placed",
            OrderStatus::AwaitingCourier => "awaiting_courier",
            OrderStatus::Collecting      => "collecting",
            OrderStatus::Delivering      => "delivering",
            OrderStatus::Delivered       => "delivered",
            OrderStatus::Cancelled       => "cancelled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegStatus {
    Pending,
    PickedUp,
    Failed,
    Settled,
}

impl LegStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            LegStatus::Pending  => "pending",
            LegStatus::PickedUp => "picked_up",
            LegStatus::Failed   => "failed",
            LegStatus::Settled  => "settled",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendorLeg {
    pub id:                   Uuid,
    pub order_id:             Uuid,
    pub tenant_id:            Uuid,
    pub vendor_id:            Uuid,
    pub goods_subtotal_cents: i64,
    /// Snapshotted at order time. The vendor's rate may change later; this
    /// order settles at the rate that applied when it was placed.
    pub commission_bps:       i32,
    pub commission_cents:     i64,
    pub payout_cents:         i64,
    pub status:               LegStatus,
    pub picked_up_at:         Option<DateTime<Utc>>,
    pub created_at:           DateTime<Utc>,
}

impl VendorLeg {
    /// Split a subtotal into commission and payout.
    ///
    /// `payout = subtotal - commission` rather than a second multiplication, so
    /// the two can never fail to sum to the subtotal regardless of rounding.
    pub fn settle(
        tenant_id: Uuid,
        vendor_id: Uuid,
        goods_subtotal_cents: i64,
        commission_bps: i32,
    ) -> Self {
        let commission_cents = goods_subtotal_cents * commission_bps as i64 / 10_000;
        Self {
            id: Uuid::new_v4(),
            order_id: Uuid::nil(), // set by Order::place
            tenant_id,
            vendor_id,
            goods_subtotal_cents,
            commission_bps,
            commission_cents,
            payout_cents: goods_subtotal_cents - commission_cents,
            status: LegStatus::Pending,
            picked_up_at: None,
            created_at: Utc::now(),
        }
    }

    pub fn mark_picked_up(&mut self) {
        self.status = LegStatus::PickedUp;
        self.picked_up_at = Some(Utc::now());
    }

    /// A vendor whose pickup failed is not paid. Per-leg status is what lets an
    /// order deliver what was collected and refund only the failed leg.
    pub fn mark_failed(&mut self) {
        self.status = LegStatus::Failed;
    }
}

/// The full three-leg split for one order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Settlement {
    pub vendor_payouts_cents:   i64,
    pub commissions_cents:      i64,
    pub courier_earnings_cents: i64,
    pub partner_margin_cents:   i64,
}

impl Settlement {
    /// Everything the Partner keeps, by both routes.
    ///
    /// Provided as a method rather than a field on `Settlement` because the
    /// struct's four fields must each name a disjoint slice of the grand total
    /// — a fifth field overlapping two of them would break the balance
    /// identity the moment anyone summed the struct naively.
    pub fn partner_revenue_cents(&self) -> i64 {
        self.commissions_cents + self.partner_margin_cents
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TransitionError {
    #[error("cannot go from {from:?} to {to:?}")]
    Illegal { from: OrderStatus, to: OrderStatus },
    #[error("{0} leg(s) still pending collection")]
    LegsPending(usize),
    #[error("no leg was collected — nothing to deliver")]
    NothingCollected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id:                 Uuid,
    pub tenant_id:          Uuid,
    pub customer_id:        Uuid,
    pub basket_id:          Uuid,
    pub plan_id:            Uuid,
    pub status:             OrderStatus,
    pub goods_total_cents:  i64,
    pub delivery_fee_cents: i64,
    pub tip_cents:          i64,
    pub grand_total_cents:  i64,
    pub courier_trip_cents: i64,
    pub courier_task_id:    Option<Uuid>,
    pub legs:               Vec<VendorLeg>,
    pub placed_at:          DateTime<Utc>,
    pub delivered_at:       Option<DateTime<Utc>>,
}

impl Order {
    #[allow(clippy::too_many_arguments)]
    pub fn place(
        tenant_id: Uuid,
        customer_id: Uuid,
        basket_id: Uuid,
        plan_id: Uuid,
        mut legs: Vec<VendorLeg>,
        delivery_fee_cents: i64,
        tip_cents: i64,
        courier_trip_cents: i64,
    ) -> Self {
        let id = Uuid::new_v4();
        for l in &mut legs {
            l.order_id = id;
        }

        // Derived, never passed in — a caller-supplied total is a place for the
        // arithmetic to disagree with itself.
        let goods_total_cents: i64 = legs.iter().map(|l| l.goods_subtotal_cents).sum();

        Self {
            id,
            tenant_id,
            customer_id,
            basket_id,
            plan_id,
            status: OrderStatus::Placed,
            goods_total_cents,
            delivery_fee_cents,
            tip_cents,
            grand_total_cents: goods_total_cents + delivery_fee_cents + tip_cents,
            courier_trip_cents,
            courier_task_id: None,
            legs,
            placed_at: Utc::now(),
            delivered_at: None,
        }
    }

    /// Kafka is at-least-once, so a repeat of the transition we already made is
    /// a no-op rather than an error. Anything else is refused: silently
    /// accepting an out-of-order event is how an uncollected order gets marked
    /// delivered.
    fn advance(&mut self, to: OrderStatus, from: &[OrderStatus]) -> Result<(), TransitionError> {
        if self.status == to {
            return Ok(());
        }
        if !from.contains(&self.status) {
            return Err(TransitionError::Illegal { from: self.status, to });
        }
        self.status = to;
        Ok(())
    }

    pub fn courier_offered(&mut self) -> Result<(), TransitionError> {
        self.advance(OrderStatus::AwaitingCourier, &[OrderStatus::Placed])
    }

    pub fn courier_claimed(&mut self, assignment_id: Uuid) -> Result<(), TransitionError> {
        self.advance(OrderStatus::Collecting, &[OrderStatus::Placed, OrderStatus::AwaitingCourier])?;
        self.courier_task_id = Some(assignment_id);
        Ok(())
    }

    /// Every leg has reached a terminal state and at least one was collected.
    ///
    /// A failed leg is resolved, not pending — the courier delivers what they
    /// have and the failed leg is refunded separately. Only a still-`Pending`
    /// leg blocks, because delivering then would pay a vendor whose goods were
    /// never picked up.
    pub fn all_legs_collected(&mut self) -> Result<(), TransitionError> {
        let pending = self.legs.iter().filter(|l| l.status == LegStatus::Pending).count();
        if pending > 0 {
            return Err(TransitionError::LegsPending(pending));
        }
        if !self.legs.iter().any(|l| l.status == LegStatus::PickedUp) {
            return Err(TransitionError::NothingCollected);
        }
        self.advance(OrderStatus::Delivering, &[OrderStatus::Collecting])
    }

    pub fn delivered(&mut self) -> Result<(), TransitionError> {
        self.advance(OrderStatus::Delivered, &[OrderStatus::Delivering])?;
        self.delivered_at = Some(Utc::now());
        Ok(())
    }

    /// Cancel, from any state except delivered.
    ///
    /// The plan called this terminal from *any* state, which would let a
    /// delivered order be flipped to cancelled — quietly dropping a completed
    /// order out of settlement while the vendor and courier have already been
    /// credited. Undoing a delivery is a refund, which is a separate concern
    /// with its own money movement, so it is refused here rather than
    /// approximated.
    pub fn cancel(&mut self) -> Result<(), TransitionError> {
        if self.status == OrderStatus::Delivered {
            return Err(TransitionError::Illegal {
                from: OrderStatus::Delivered,
                to: OrderStatus::Cancelled,
            });
        }
        self.status = OrderStatus::Cancelled;
        Ok(())
    }

    /// Where every cent the customer paid goes.
    ///
    /// The four legs sum to `grand_total_cents` by construction:
    ///   goods_total  = vendor_payouts + commissions        (per-leg invariant)
    ///   delivery_fee = courier_trip   + fee_margin         (by definition)
    ///   tip          → courier, in full
    ///
    /// so vendor_payouts + commissions + (courier_trip + tip) + fee_margin
    ///  = goods_total + delivery_fee + tip
    ///  = grand_total.
    pub fn settlement(&self) -> Settlement {
        let vendor_payouts_cents: i64 = self.legs.iter().map(|l| l.payout_cents).sum();
        let commissions_cents:    i64 = self.legs.iter().map(|l| l.commission_cents).sum();

        // Per trip, not per stop. This asymmetry is the business model: a second
        // pickup barely moves courier cost but adds a full commission leg.
        let courier_earnings_cents = self.courier_trip_cents + self.tip_cents;
        let fee_margin_cents       = self.delivery_fee_cents - self.courier_trip_cents;

        Settlement {
            vendor_payouts_cents,
            commissions_cents,
            courier_earnings_cents,
            // Fee margin only — NOT `fee_margin + commissions`.
            //
            // Commission is already its own term above. Folding it in here too
            // would report the same money twice, and the balance test would
            // fail (correctly). Total Partner revenue is
            // `Settlement::partner_revenue_cents`, which adds the two back
            // together for reporting without breaking the identity.
            partner_margin_cents: fee_margin_cents,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn leg(subtotal: i64, bps: i32) -> VendorLeg {
        VendorLeg::settle(Uuid::new_v4(), Uuid::new_v4(), subtotal, bps)
    }

    fn order(legs: Vec<VendorLeg>, fee: i64, tip: i64, trip: i64) -> Order {
        Order::place(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), legs, fee, tip, trip)
    }

    #[test]
    fn a_leg_splits_its_subtotal_exactly() {
        let l = leg(34_000, 1500);
        assert_eq!(l.commission_cents, 5_100);
        assert_eq!(l.payout_cents, 28_900);
        assert_eq!(l.commission_cents + l.payout_cents, l.goods_subtotal_cents);
    }

    #[test]
    fn commission_truncates_in_the_vendors_favour() {
        // 999 * 15% = 149.85 → 149, so the vendor keeps the part-cent.
        let l = leg(999, 1500);
        assert_eq!(l.commission_cents, 149);
        assert_eq!(l.payout_cents, 850);
        assert_eq!(l.commission_cents + l.payout_cents, 999);
    }

    #[test]
    fn the_grand_total_is_goods_plus_fee_plus_tip() {
        let o = order(vec![leg(34_000, 1500), leg(28_000, 1200)], 7_900, 4_000, 5_800);
        assert_eq!(o.goods_total_cents, 62_000);
        assert_eq!(o.grand_total_cents, 62_000 + 7_900 + 4_000);
    }

    /// THE INVARIANT. What the customer pays must exactly equal what everyone
    /// else receives. If this can drift, money is being created or destroyed.
    #[test]
    fn settlement_balances_exactly() {
        let o = order(vec![leg(34_000, 1500), leg(28_000, 1200)], 7_900, 4_000, 5_800);
        let s = o.settlement();

        assert_eq!(
            o.grand_total_cents,
            s.vendor_payouts_cents + s.commissions_cents + s.courier_earnings_cents + s.partner_margin_cents,
            "customer_paid must equal vendor_payouts + commissions + courier_earnings + partner_margin"
        );
    }

    /// The courier is paid per trip, not per stop — which is exactly why adding
    /// a second pickup is nearly free while adding a full commission leg.
    #[test]
    fn courier_earnings_are_the_trip_plus_the_whole_tip() {
        let o = order(vec![leg(10_000, 1000)], 7_900, 4_000, 5_800);
        let s = o.settlement();
        assert_eq!(s.courier_earnings_cents, 5_800 + 4_000);
    }

    /// `partner_margin` is the fee margin only — commission is its own term.
    /// Each term must name a disjoint slice of the total or they cannot sum to
    /// it. Total Partner revenue is the two added together, asserted here so
    /// neither reading can drift from the other.
    #[test]
    fn partner_margin_is_the_fee_less_the_courier_trip() {
        let o = order(vec![leg(10_000, 1000)], 7_900, 0, 5_800);
        let s = o.settlement();

        assert_eq!(s.partner_margin_cents, 7_900 - 5_800, "fee margin only");
        assert_eq!(s.commissions_cents, 1_000);
        assert_eq!(
            s.partner_revenue_cents(),
            (7_900 - 5_800) + 1_000,
            "total Partner revenue is fee margin plus commission"
        );
    }

    /// The margin lever, expressed as a test: a second vendor adds a full
    /// commission leg while the fee — and therefore the courier cost — is flat.
    ///
    /// The comparison is on total Partner revenue, not `partner_margin_cents`.
    /// That field is fee-margin-only by design, so it is *identical* for both
    /// orders; asserting it grew would contradict the decision recorded on
    /// `settlement` and fail.
    #[test]
    fn a_second_vendor_adds_commission_without_adding_fee() {
        let one = order(vec![leg(30_000, 1500)], 7_900, 0, 5_800);
        let two = order(vec![leg(30_000, 1500), leg(30_000, 1500)], 7_900, 0, 5_800);

        assert_eq!(one.delivery_fee_cents, two.delivery_fee_cents, "the fee is flat");
        assert_eq!(
            one.settlement().partner_margin_cents,
            two.settlement().partner_margin_cents,
            "fee margin is unchanged — the second stop costs the Partner nothing",
        );
        assert!(
            two.settlement().partner_revenue_cents() > one.settlement().partner_revenue_cents(),
            "the second vendor's commission is pure additional revenue"
        );
    }

    #[test]
    fn a_zero_tip_order_still_balances() {
        let o = order(vec![leg(15_000, 2000)], 4_900, 0, 3_500);
        let s = o.settlement();
        assert_eq!(
            o.grand_total_cents,
            s.vendor_payouts_cents + s.commissions_cents + s.courier_earnings_cents + s.partner_margin_cents,
        );
    }

    /// A courier trip costing more than the fee is a loss-leading order, not an
    /// impossible one — short-distance pricing floors make it routine. The
    /// identity must still hold with a negative margin, or the ledger would
    /// silently absorb the loss.
    #[test]
    fn an_underwater_delivery_fee_still_balances() {
        let o = order(vec![leg(20_000, 1500)], 4_900, 0, 6_500);
        let s = o.settlement();

        assert_eq!(s.partner_margin_cents, -1_600, "the Partner eats the difference");
        assert_eq!(
            o.grand_total_cents,
            s.vendor_payouts_cents + s.commissions_cents + s.courier_earnings_cents + s.partner_margin_cents,
        );
    }

    /// The happy path, in order. Each transition is legal only from the state
    /// before it — a machine that accepts any transition from any state is not
    /// a machine, it is a mutable field.
    #[test]
    fn the_lifecycle_advances_in_order() {
        let mut o = order(vec![leg(10_000, 1000)], 4_900, 0, 3_500);
        assert_eq!(o.status, OrderStatus::Placed);

        assert!(o.courier_offered().is_ok());
        assert_eq!(o.status, OrderStatus::AwaitingCourier);

        assert!(o.courier_claimed(Uuid::new_v4()).is_ok());
        assert_eq!(o.status, OrderStatus::Collecting);

        o.legs[0].mark_picked_up();
        assert!(o.all_legs_collected().is_ok());
        assert_eq!(o.status, OrderStatus::Delivering);

        assert!(o.delivered().is_ok());
        assert_eq!(o.status, OrderStatus::Delivered);
        assert!(o.delivered_at.is_some());
    }

    /// Kafka delivers at least once, so the same event can arrive twice.
    /// A repeat of the current transition must be a no-op, not an error and
    /// not a double-advance.
    #[test]
    fn a_repeated_transition_is_idempotent() {
        let mut o = order(vec![leg(10_000, 1000)], 4_900, 0, 3_500);
        o.courier_offered().unwrap();

        assert!(o.courier_offered().is_ok(), "a duplicate event must not error");
        assert_eq!(o.status, OrderStatus::AwaitingCourier, "and must not advance");
    }

    /// Out-of-order delivery is also possible. Skipping ahead must be refused
    /// loudly rather than silently marking an uncollected order delivered.
    #[test]
    fn skipping_a_state_is_refused() {
        let mut o = order(vec![leg(10_000, 1000)], 4_900, 0, 3_500);
        assert!(o.delivered().is_err(), "a placed order cannot jump to delivered");
        assert_eq!(o.status, OrderStatus::Placed);
    }

    /// Delivering with a leg still pending would pay a vendor whose goods were
    /// never collected.
    #[test]
    fn collection_is_refused_while_a_leg_is_pending() {
        let mut o = order(vec![leg(10_000, 1000), leg(5_000, 1000)], 4_900, 0, 3_500);
        o.courier_offered().unwrap();
        o.courier_claimed(Uuid::new_v4()).unwrap();

        o.legs[0].mark_picked_up();
        assert!(o.all_legs_collected().is_err(), "one leg is still pending");

        o.legs[1].mark_picked_up();
        assert!(o.all_legs_collected().is_ok());
    }

    /// A failed leg does not block the trip — the courier delivers what was
    /// collected and the failed leg is refunded separately.
    #[test]
    fn a_failed_leg_does_not_block_collection() {
        let mut o = order(vec![leg(10_000, 1000), leg(5_000, 1000)], 4_900, 0, 3_500);
        o.courier_offered().unwrap();
        o.courier_claimed(Uuid::new_v4()).unwrap();

        o.legs[0].mark_picked_up();
        o.legs[1].mark_failed();

        assert!(o.all_legs_collected().is_ok(), "a failed leg is resolved, not pending");
    }

    /// Every leg failing means there is nothing to deliver.
    #[test]
    fn an_order_with_no_collected_legs_cannot_be_delivered() {
        let mut o = order(vec![leg(10_000, 1000)], 4_900, 0, 3_500);
        o.courier_offered().unwrap();
        o.courier_claimed(Uuid::new_v4()).unwrap();
        o.legs[0].mark_failed();

        assert!(o.all_legs_collected().is_err(), "nothing was collected");
    }

    #[test]
    fn a_cancelled_order_accepts_no_further_transitions() {
        let mut o = order(vec![leg(10_000, 1000)], 4_900, 0, 3_500);
        o.cancel().unwrap();
        assert!(o.courier_offered().is_err());
        assert!(o.delivered().is_err());
    }

    /// A delivered order cannot be cancelled. Flipping it would drop a completed
    /// order out of settlement while the vendor and courier have already been
    /// credited — undoing a delivery is a refund, with its own money movement.
    #[test]
    fn a_delivered_order_cannot_be_cancelled() {
        let mut o = order(vec![leg(10_000, 1000)], 4_900, 0, 3_500);
        o.courier_offered().unwrap();
        o.courier_claimed(Uuid::new_v4()).unwrap();
        o.legs[0].mark_picked_up();
        o.all_legs_collected().unwrap();
        o.delivered().unwrap();

        assert!(o.cancel().is_err());
        assert_eq!(o.status, OrderStatus::Delivered, "the delivery stands");
    }

    /// A courier claiming straight from Placed is legal: the offer event and
    /// the claim can arrive out of order, and refusing the claim would strand
    /// an order whose courier is already on the way.
    #[test]
    fn a_claim_without_a_preceding_offer_is_accepted() {
        let mut o = order(vec![leg(10_000, 1000)], 4_900, 0, 3_500);
        let assignment = Uuid::new_v4();

        assert!(o.courier_claimed(assignment).is_ok());
        assert_eq!(o.status, OrderStatus::Collecting);
        assert_eq!(o.courier_task_id, Some(assignment));
    }
}
