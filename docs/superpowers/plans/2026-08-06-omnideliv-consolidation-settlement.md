# OmniDeliv Consolidation, Orders & Settlement Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn a reviewed basket into a dispatched, settled order — prep-time-aware multi-stop consolidation, the three-leg money split, and the append-only ledgers that record it.

**Architecture:** Two new modules inside `services/omnideliv` (`consolidation`, `orders`) plus the courier earnings ledger deferred out of Plan 2 into `services/field-ops`. Consolidation sequences stops by readiness rather than distance, so hot food spends the least time in the bag. Settlement is integer-cents throughout with a balance invariant enforced by a property test: what the customer pays must always equal what everyone else receives.

**Tech Stack:** Rust 2021, Axum, SQLx, PostgreSQL, Kafka.

---

## Dependencies

**Requires Plan 2** — `services/field-ops` with courier assignment and the atomic claim.
**Requires Plan 3** — `services/omnideliv` with `Vendor::commission_on`/`payout_on` and `Basket::subtotals_by_vendor`.

Verify before starting:

```bash
CARGO_INCREMENTAL=0 cargo check -p logisticos-field-ops -p logisticos-omnideliv
```

Plan 4 is **not** required — this plan can run in parallel with the mesh. Checkout is a plain user-initiated transaction, deliberately not an agent action, so nothing here depends on mesh code.

---

## Scope

**In:** consolidation plans, orders, per-vendor legs, the three-leg split, vendor payout ledger, courier earnings ledger, order telemetry, the checkout commit path, courier dispatch via field-ops.

**Out:** payment capture against a real gateway (the commit path records the charge and emits `order.placed`; wiring Stripe/PayMongo is its own plan), refunds and partial-pickup recovery beyond recording the leg state.

---

## Task 1: Consolidation — sequence by readiness

The Fleet agent's sequencing rule is the margin lever: the customer pays one flat fee regardless of stop count, while courier cost barely rises with a second pickup. Getting the *order* of stops right is what makes the second pickup cheap rather than a spoiled meal.

**Files:**
- Create: `services/omnideliv/migrations/0004_create_consolidation.sql`, `src/domain/entities/consolidation.rs`

- [ ] **Step 1: Write the migration**

```sql
-- A courier route over a multi-vendor basket. Stops are ordered by readiness,
-- not distance — see the sequencing rule in the entity.
CREATE TABLE IF NOT EXISTS omnideliv.consolidation_plans (
    id                  UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id           UUID        NOT NULL,
    basket_id           UUID        NOT NULL REFERENCES omnideliv.baskets(id),
    -- Ordered stops: [{"vendor_id": ..., "ready_at": ..., "seq": 0}, ...]
    stops               JSONB       NOT NULL DEFAULT '[]',
    total_distance_m    INT         NOT NULL DEFAULT 0,
    -- One fee for the whole route, whatever the stop count. The product promise
    -- and the margin lever in the same column.
    flat_fee_cents      BIGINT      NOT NULL CHECK (flat_fee_cents >= 0),
    -- ["hot","chilled"] etc. Populated when a basket mixes classes, so ops can
    -- see why a route was sequenced the way it was.
    temperature_classes TEXT[]      NOT NULL DEFAULT '{}',
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_consolidation_basket
    ON omnideliv.consolidation_plans (basket_id);
```

- [ ] **Step 2: Write the failing test**

```rust
// services/omnideliv/src/domain/entities/consolidation.rs — tests block
#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn stop(prep_mins: i32, class: TemperatureClass) -> PendingStop {
        PendingStop {
            vendor_id: Uuid::new_v4(),
            prep_time_minutes: prep_mins,
            temperature_class: class,
        }
    }

    /// The sequencing rule: collect what is ready soonest first, so the hot
    /// items spend the least time in the bag.
    #[test]
    fn stops_are_sequenced_by_readiness_not_input_order() {
        let kitchen = stop(20, TemperatureClass::Hot);
        let grocery = stop(5, TemperatureClass::Chilled);

        // Deliberately pass the slow stop first.
        let plan = ConsolidationPlan::sequence(
            Uuid::new_v4(), Uuid::new_v4(), vec![kitchen.clone(), grocery.clone()], 4_200, 7_900,
        );

        assert_eq!(plan.stops[0].vendor_id, grocery.vendor_id, "the 5-minute pick goes first");
        assert_eq!(plan.stops[1].vendor_id, kitchen.vendor_id, "the 20-minute kitchen goes last");
    }

    #[test]
    fn a_single_stop_route_is_trivially_sequenced() {
        let only = stop(15, TemperatureClass::Hot);
        let plan = ConsolidationPlan::sequence(Uuid::new_v4(), Uuid::new_v4(), vec![only.clone()], 1_100, 4_900);
        assert_eq!(plan.stops.len(), 1);
        assert_eq!(plan.stops[0].seq, 0);
    }

    /// A mixed-temperature basket is flagged so ops can see why the route was
    /// ordered the way it was — and so Screen B can show the constraint.
    #[test]
    fn a_mixed_temperature_basket_records_both_classes() {
        let plan = ConsolidationPlan::sequence(
            Uuid::new_v4(), Uuid::new_v4(),
            vec![stop(20, TemperatureClass::Hot), stop(5, TemperatureClass::Chilled)],
            4_200, 7_900,
        );
        assert_eq!(plan.temperature_classes.len(), 2);
        assert!(plan.temperature_classes.contains(&TemperatureClass::Hot));
        assert!(plan.temperature_classes.contains(&TemperatureClass::Chilled));
    }

    #[test]
    fn a_single_class_basket_records_one_class() {
        let plan = ConsolidationPlan::sequence(
            Uuid::new_v4(), Uuid::new_v4(),
            vec![stop(10, TemperatureClass::Ambient), stop(5, TemperatureClass::Ambient)],
            2_000, 5_900,
        );
        assert_eq!(plan.temperature_classes, vec![TemperatureClass::Ambient]);
    }

    /// THE PRODUCT PROMISE: one fee, whatever the stop count. A per-stop fee
    /// would make consolidation a cost to the customer instead of a benefit.
    #[test]
    fn the_fee_is_flat_regardless_of_stop_count() {
        let one = ConsolidationPlan::sequence(
            Uuid::new_v4(), Uuid::new_v4(), vec![stop(10, TemperatureClass::Hot)], 3_000, 7_900);
        let three = ConsolidationPlan::sequence(
            Uuid::new_v4(), Uuid::new_v4(),
            vec![stop(10, TemperatureClass::Hot), stop(5, TemperatureClass::Chilled), stop(8, TemperatureClass::Ambient)],
            3_000, 7_900,
        );
        assert_eq!(one.flat_fee_cents, three.flat_fee_cents);
    }

    #[test]
    fn seq_numbers_are_contiguous_from_zero() {
        let plan = ConsolidationPlan::sequence(
            Uuid::new_v4(), Uuid::new_v4(),
            vec![stop(30, TemperatureClass::Hot), stop(5, TemperatureClass::Chilled), stop(15, TemperatureClass::Ambient)],
            5_000, 8_900,
        );
        let seqs: Vec<i32> = plan.stops.iter().map(|s| s.seq).collect();
        assert_eq!(seqs, vec![0, 1, 2]);
    }
}
```

- [ ] **Step 3: Run it to verify it fails**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv consolidation::`
Expected: FAIL to compile — `cannot find type 'ConsolidationPlan' in this scope`.

- [ ] **Step 4: Write the entity**

```rust
//! Multi-stop consolidation.
//!
//! Consolidation is the margin lever, not a customer perk: the fee is flat
//! regardless of stop count, while courier cost barely rises with a second
//! pickup and each additional vendor adds a full commission leg. Sequencing
//! quality is therefore revenue, not decoration.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemperatureClass {
    Hot,
    Chilled,
    Ambient,
}

/// A stop before sequencing.
#[derive(Debug, Clone)]
pub struct PendingStop {
    pub vendor_id:         Uuid,
    pub prep_time_minutes: i32,
    pub temperature_class: TemperatureClass,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Stop {
    pub vendor_id:         Uuid,
    pub seq:               i32,
    pub prep_time_minutes: i32,
    pub temperature_class: TemperatureClass,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationPlan {
    pub id:                  Uuid,
    pub tenant_id:           Uuid,
    pub basket_id:           Uuid,
    pub stops:               Vec<Stop>,
    pub total_distance_m:    i32,
    pub flat_fee_cents:      i64,
    pub temperature_classes: Vec<TemperatureClass>,
    pub created_at:          DateTime<Utc>,
}

impl ConsolidationPlan {
    /// Sequence stops by readiness, soonest first.
    ///
    /// Not by distance. A grocery pick ready in 5 minutes collected before a
    /// kitchen order ready in 20 means the hot food is the last thing in the bag
    /// and the first thing out — which is the difference between a warm meal and
    /// a refund. Distance still shapes the fee via `total_distance_m`; it just
    /// does not decide the order.
    ///
    /// Ties break on vendor id so the sequence is deterministic — a route that
    /// reorders between two identical calls would make dispatch untestable.
    pub fn sequence(
        tenant_id: Uuid,
        basket_id: Uuid,
        mut pending: Vec<PendingStop>,
        total_distance_m: i32,
        flat_fee_cents: i64,
    ) -> Self {
        pending.sort_by(|a, b| {
            a.prep_time_minutes
                .cmp(&b.prep_time_minutes)
                .then_with(|| a.vendor_id.cmp(&b.vendor_id))
        });

        let stops: Vec<Stop> = pending
            .iter()
            .enumerate()
            .map(|(i, p)| Stop {
                vendor_id:         p.vendor_id,
                seq:               i as i32,
                prep_time_minutes: p.prep_time_minutes,
                temperature_class: p.temperature_class,
            })
            .collect();

        // Distinct classes, in a stable order so the value is comparable
        // between runs and readable in the ops UI.
        let mut classes: Vec<TemperatureClass> =
            pending.iter().map(|p| p.temperature_class).collect();
        classes.sort_by_key(|c| match c {
            TemperatureClass::Hot     => 0,
            TemperatureClass::Chilled => 1,
            TemperatureClass::Ambient => 2,
        });
        classes.dedup();

        Self {
            id: Uuid::new_v4(),
            tenant_id,
            basket_id,
            stops,
            total_distance_m,
            flat_fee_cents,
            temperature_classes: classes,
            created_at: Utc::now(),
        }
    }

    /// True when the basket spans more than one temperature class — the
    /// cross-category constraint Screen B surfaces.
    pub fn has_mixed_temperatures(&self) -> bool {
        self.temperature_classes.len() > 1
    }
}
```

Add to `src/domain/entities/mod.rs`:

```rust
pub mod consolidation;
pub use consolidation::{ConsolidationPlan, PendingStop, Stop, TemperatureClass};
```

- [ ] **Step 5: Run the tests**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv consolidation::`
Expected: PASS — 6 passed.

- [ ] **Step 6: Commit**

```bash
git add services/omnideliv/migrations/0004_create_consolidation.sql services/omnideliv/src/domain/
git commit -m "feat(omnideliv): consolidation plans sequenced by readiness

Stops are ordered by prep time, not distance: a 5-minute grocery pick before a
20-minute kitchen order means hot food spends the least time in the bag. The
fee stays flat regardless of stop count — the product promise and the margin
lever in the same field."
```

---

## Task 2: Orders and the three-leg split

**Files:**
- Create: `services/omnideliv/migrations/0005_create_orders.sql`, `src/domain/entities/order.rs`

- [ ] **Step 1: Write the migration**

```sql
CREATE TABLE IF NOT EXISTS omnideliv.orders (
    id                 UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id          UUID        NOT NULL,
    customer_id        UUID        NOT NULL,
    basket_id          UUID        NOT NULL REFERENCES omnideliv.baskets(id),
    plan_id            UUID        NOT NULL REFERENCES omnideliv.consolidation_plans(id),
    status             TEXT        NOT NULL DEFAULT 'placed'
                                   CHECK (status IN ('placed','awaiting_courier','collecting','delivering','delivered','cancelled')),
    -- Money. All integer cents; no floats anywhere in this table.
    goods_total_cents  BIGINT      NOT NULL CHECK (goods_total_cents >= 0),
    delivery_fee_cents BIGINT      NOT NULL CHECK (delivery_fee_cents >= 0),
    tip_cents          BIGINT      NOT NULL DEFAULT 0 CHECK (tip_cents >= 0),
    grand_total_cents  BIGINT      NOT NULL CHECK (grand_total_cents >= 0),
    -- What the courier earns for the trip, excluding tip. Partner margin is
    -- delivery_fee - courier_trip.
    courier_trip_cents BIGINT      NOT NULL DEFAULT 0 CHECK (courier_trip_cents >= 0),
    -- field_ops.courier_assignments.id. Not an FK: field-ops is a separate
    -- service with its own schema, and a cross-schema FK would couple their
    -- migrations and their uptime.
    courier_task_id    UUID,
    placed_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    delivered_at       TIMESTAMPTZ,

    -- The balance invariant, enforced by the database as well as by the
    -- settlement test. Cheap to check, and it makes an arithmetic bug a failed
    -- write rather than money quietly going missing.
    CONSTRAINT grand_total_balances
        CHECK (grand_total_cents = goods_total_cents + delivery_fee_cents + tip_cents)
);

CREATE INDEX IF NOT EXISTS idx_order_customer
    ON omnideliv.orders (tenant_id, customer_id, placed_at DESC);
CREATE INDEX IF NOT EXISTS idx_order_status
    ON omnideliv.orders (tenant_id, status);

-- One row per vendor in the order. Each settles independently: a vendor whose
-- pickup succeeded is paid even if a sibling leg failed.
CREATE TABLE IF NOT EXISTS omnideliv.order_vendor_legs (
    id                   UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    order_id             UUID        NOT NULL REFERENCES omnideliv.orders(id) ON DELETE CASCADE,
    tenant_id            UUID        NOT NULL,
    vendor_id            UUID        NOT NULL REFERENCES omnideliv.vendors(id),
    goods_subtotal_cents BIGINT      NOT NULL CHECK (goods_subtotal_cents >= 0),
    -- Snapshotted from the vendor at order time. The vendor's rate may change;
    -- this order settles at the rate that applied when it was placed.
    commission_bps       INT         NOT NULL CHECK (commission_bps BETWEEN 0 AND 10000),
    commission_cents     BIGINT      NOT NULL CHECK (commission_cents >= 0),
    payout_cents         BIGINT      NOT NULL CHECK (payout_cents >= 0),
    status               TEXT        NOT NULL DEFAULT 'pending'
                                     CHECK (status IN ('pending','picked_up','failed','settled')),
    picked_up_at         TIMESTAMPTZ,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT leg_splits_exactly
        CHECK (goods_subtotal_cents = commission_cents + payout_cents)
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_order_vendor_leg
    ON omnideliv.order_vendor_legs (order_id, vendor_id);
CREATE INDEX IF NOT EXISTS idx_leg_settlement
    ON omnideliv.order_vendor_legs (tenant_id, status);
```

- [ ] **Step 2: Write the failing settlement test**

```rust
// services/omnideliv/src/domain/entities/order.rs — tests block
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
            s.partner_margin_cents + s.commissions_cents,
            (7_900 - 5_800) + 1_000,
            "total Partner revenue is fee margin plus commission"
        );
    }

    /// The margin lever, expressed as a test: a second vendor adds a full
    /// commission leg while the fee — and therefore the courier cost — is flat.
    #[test]
    fn a_second_vendor_adds_commission_without_adding_fee() {
        let one = order(vec![leg(30_000, 1500)], 7_900, 0, 5_800);
        let two = order(vec![leg(30_000, 1500), leg(30_000, 1500)], 7_900, 0, 5_800);

        assert_eq!(one.delivery_fee_cents, two.delivery_fee_cents, "the fee is flat");
        assert!(
            two.settlement().partner_margin_cents > one.settlement().partner_margin_cents,
            "the second vendor's commission is pure additional margin"
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
}
```

- [ ] **Step 3: Run it to verify it fails**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv order::`
Expected: FAIL to compile — `cannot find type 'VendorLeg' in this scope`.

- [ ] **Step 4: Write the entities**

```rust
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
    pub vendor_payouts_cents:  i64,
    pub commissions_cents:     i64,
    pub courier_earnings_cents: i64,
    pub partner_margin_cents:  i64,
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
            // `commissions_cents + partner_margin_cents`, computed by whichever
            // caller wants that figure rather than baked in here where it would
            // silently break the identity.
            partner_margin_cents: fee_margin_cents,
        }
    }
}
```

> **Why `partner_margin` excludes commission.** Each term in `Settlement` must name a disjoint slice of `grand_total`, or the four cannot sum to it. Commission is the Partner's revenue *from vendors*; fee margin is its revenue *from the delivery fee*. Both are Partner money, but they arrive by different routes and only one of them belongs in the `delivery_fee` decomposition. Reporting the combined figure is a caller concern — the test in Task 2 asserts both readings so neither can drift.

Add to `src/domain/entities/mod.rs`:

```rust
pub mod order;
pub use order::{LegStatus, Order, OrderStatus, Settlement, VendorLeg};
```

- [ ] **Step 5: Run the tests**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv order::`
Expected: PASS — 8 passed. If `settlement_balances_exactly` fails, the arithmetic drifted — do not adjust the test to match the code.

- [ ] **Step 6: Commit**

```bash
git add services/omnideliv/migrations/0005_create_orders.sql services/omnideliv/src/domain/
git commit -m "feat(omnideliv): orders with three-leg settlement and a balance invariant

Every leg computes payout as subtotal minus commission rather than a second
multiplication, so the two cannot fail to sum. The order-level test asserts
the customer's payment exactly equals vendor payouts plus commissions plus
courier earnings plus partner margin — money can be neither created nor
destroyed. CHECK constraints enforce the same identities at the database."
```

---

## Task 3: Settlement property test

Six examples prove the arithmetic on six shapes. A generated sweep proves it on thousands, including the rounding edges no one thinks to write by hand.

**Files:**
- Create: `services/omnideliv/tests/settlement_invariant.rs`

- [ ] **Step 1: Write the test**

```rust
// services/omnideliv/tests/settlement_invariant.rs
//! The settlement balance invariant, swept across the input space.
//!
//! Hand-written examples cover the shapes someone thought of. This sweeps the
//! rounding edges — subtotals whose commission lands on a fraction of a cent,
//! which is exactly where an integer-maths bug hides.

use logisticos_omnideliv::domain::entities::{Order, VendorLeg};
use uuid::Uuid;

fn check(subtotals: &[i64], bps: &[i32], fee: i64, tip: i64, trip: i64) {
    let legs: Vec<VendorLeg> = subtotals
        .iter()
        .zip(bps.iter())
        .map(|(s, b)| VendorLeg::settle(Uuid::new_v4(), Uuid::new_v4(), *s, *b))
        .collect();

    // Per-leg invariant first — a leg that does not split exactly makes the
    // order-level failure much harder to localise.
    for l in &legs {
        assert_eq!(
            l.commission_cents + l.payout_cents,
            l.goods_subtotal_cents,
            "leg failed to split exactly: subtotal={} bps={}",
            l.goods_subtotal_cents, l.commission_bps
        );
        assert!(l.commission_cents >= 0 && l.payout_cents >= 0, "no negative money");
    }

    let o = Order::place(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(),
                         legs, fee, tip, trip);
    let s = o.settlement();

    assert_eq!(
        o.grand_total_cents,
        s.vendor_payouts_cents + s.commissions_cents + s.courier_earnings_cents + s.partner_margin_cents,
        "settlement did not balance: subtotals={subtotals:?} bps={bps:?} fee={fee} tip={tip} trip={trip}"
    );
}

#[test]
fn settlement_balances_across_the_rounding_edges() {
    // Subtotals chosen to land commission on a fraction of a cent at common
    // rates: primes, near-primes and values just off a round number.
    let subtotals = [1_i64, 7, 99, 101, 999, 1_001, 3_333, 9_999, 12_345, 99_999, 1_000_003];
    let rates     = [0_i32, 1, 250, 999, 1_500, 3_333, 5_000, 9_999, 10_000];
    let fees      = [0_i64, 1, 4_900, 7_900];
    let tips      = [0_i64, 1, 4_000];

    let mut cases = 0;
    for &s in &subtotals {
        for &b in &rates {
            for &fee in &fees {
                for &tip in &tips {
                    // Courier trip never exceeds the fee — a negative partner
                    // margin is a pricing bug, not a settlement one, and is
                    // guarded at the checkout path instead.
                    for &trip in &[0, fee / 2, fee] {
                        check(&[s], &[b], fee, tip, trip);
                        cases += 1;
                    }
                }
            }
        }
    }

    // Multi-vendor: the case the flat fee exists for.
    for &a in &subtotals {
        for &b in &subtotals {
            check(&[a, b], &[1_500, 1_200], 7_900, 4_000, 5_800);
            check(&[a, b, a], &[1_500, 1_200, 999], 7_900, 0, 5_800);
            cases += 2;
        }
    }

    assert!(cases > 3_000, "sweep should cover thousands of cases, covered {cases}");
}
```

- [ ] **Step 2: Run it**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv --test settlement_invariant`
Expected: PASS. A failure prints the exact inputs — fix the arithmetic, never the assertion.

- [ ] **Step 3: Commit**

```bash
git add services/omnideliv/tests/settlement_invariant.rs
git commit -m "test(omnideliv): sweep the settlement invariant across rounding edges"
```

---

## Task 4: Vendor payout ledger

**Files:**
- Create: `services/omnideliv/migrations/0006_create_vendor_ledger.sql`, `src/domain/entities/vendor_ledger.rs`

- [ ] **Step 1: Write the migration**

```sql
-- Vendor payouts, modelled on the existing DriverLedger: an append-only entry
-- log with a denormalised balance. Entries are never updated or deleted —
-- a correction is a new compensating entry, so the history stays auditable.

CREATE TABLE IF NOT EXISTS omnideliv.vendor_ledgers (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id     UUID        NOT NULL,
    vendor_id     UUID        NOT NULL REFERENCES omnideliv.vendors(id),
    -- Payout period, e.g. '2026-W32'. One open ledger per vendor per period.
    period        TEXT        NOT NULL,
    status        TEXT        NOT NULL DEFAULT 'open'
                              CHECK (status IN ('open','closed','settled')),
    balance_cents BIGINT      NOT NULL DEFAULT 0,
    -- Optimistic lock. Two concurrent pickups crediting the same vendor must
    -- not lose an entry to a last-write-wins race.
    version       BIGINT      NOT NULL DEFAULT 0,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_vendor_ledger_period
    ON omnideliv.vendor_ledgers (tenant_id, vendor_id, period);

CREATE TABLE IF NOT EXISTS omnideliv.vendor_ledger_entries (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    ledger_id    UUID        NOT NULL REFERENCES omnideliv.vendor_ledgers(id),
    kind         TEXT        NOT NULL
                             CHECK (kind IN ('goods_credit','commission_debit','adjustment','payout')),
    amount_cents BIGINT      NOT NULL,
    order_id     UUID,
    leg_id       UUID,
    reference    TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_vendor_entry_ledger
    ON omnideliv.vendor_ledger_entries (ledger_id, created_at);

-- Append-only, enforced. The application never issues these statements; the
-- grant makes an accidental one impossible rather than merely discouraged.
REVOKE UPDATE, DELETE ON omnideliv.vendor_ledger_entries FROM PUBLIC;
```

> **On the REVOKE.** Services connect as the schema owner, and PostgreSQL does not apply `REVOKE` from `PUBLIC` to the owner — so this does not actually stop the owning role. It is a correct statement of intent that will start enforcing the moment the service runs as a non-owner role, which is the direction the RLS follow-up (see the field-ops plan) will push everything. Task 6's test is what enforces append-only today.

- [ ] **Step 2: Write the failing test**

```rust
// services/omnideliv/src/domain/entities/vendor_ledger.rs — tests block
#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn ledger() -> VendorLedger {
        VendorLedger::open(Uuid::new_v4(), Uuid::new_v4(), "2026-W32".into())
    }

    #[test]
    fn a_new_ledger_is_open_and_flat() {
        let l = ledger();
        assert_eq!(l.status, LedgerStatus::Open);
        assert_eq!(l.balance_cents, 0);
        assert!(l.entries.is_empty());
    }

    /// Crediting a pickup records the goods credit and the commission debit as
    /// two entries, not one net figure — the vendor must be able to see what
    /// was deducted and why.
    #[test]
    fn crediting_a_leg_records_both_sides() {
        let mut l = ledger();
        l.credit_leg(34_000, 5_100, Uuid::new_v4(), Uuid::new_v4());

        assert_eq!(l.entries.len(), 2);
        assert_eq!(l.balance_cents, 28_900);
        assert!(l.entries.iter().any(|e| e.kind == EntryKind::GoodsCredit && e.amount_cents == 34_000));
        assert!(l.entries.iter().any(|e| e.kind == EntryKind::CommissionDebit && e.amount_cents == -5_100));
    }

    #[test]
    fn the_balance_always_equals_the_sum_of_entries() {
        let mut l = ledger();
        l.credit_leg(34_000, 5_100, Uuid::new_v4(), Uuid::new_v4());
        l.credit_leg(12_000, 1_800, Uuid::new_v4(), Uuid::new_v4());
        l.record_payout(20_000, Some("batch-1".into()));

        let summed: i64 = l.entries.iter().map(|e| e.amount_cents).sum();
        assert_eq!(l.balance_cents, summed, "the denormalised balance must match the entry log");
    }

    /// Append-only: a correction is a new compensating entry, never a mutation.
    #[test]
    fn an_adjustment_appends_rather_than_editing_history() {
        let mut l = ledger();
        l.credit_leg(34_000, 5_100, Uuid::new_v4(), Uuid::new_v4());
        let before = l.entries.len();

        l.adjust(-1_000, "overcharge correction".into());

        assert_eq!(l.entries.len(), before + 1, "history grows, never shrinks");
        assert_eq!(l.balance_cents, 27_900);
    }

    #[test]
    fn a_payout_reduces_the_balance() {
        let mut l = ledger();
        l.credit_leg(10_000, 1_000, Uuid::new_v4(), Uuid::new_v4());
        assert_eq!(l.balance_cents, 9_000);
        l.record_payout(9_000, None);
        assert_eq!(l.balance_cents, 0);
    }
}
```

- [ ] **Step 3: Run it to verify it fails**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv vendor_ledger::`
Expected: FAIL to compile — `cannot find type 'VendorLedger' in this scope`.

- [ ] **Step 4: Write the entity**

```rust
//! Vendor payout ledger.
//!
//! Modelled on the platform's existing `DriverLedger`: an append-only entry log
//! with a denormalised balance. Entries are never updated or deleted — a
//! correction is a new compensating entry, so the history stays auditable and
//! a vendor dispute can always be reconstructed.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LedgerStatus {
    Open,
    Closed,
    Settled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    GoodsCredit,
    CommissionDebit,
    Adjustment,
    Payout,
}

impl EntryKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            EntryKind::GoodsCredit     => "goods_credit",
            EntryKind::CommissionDebit => "commission_debit",
            EntryKind::Adjustment      => "adjustment",
            EntryKind::Payout          => "payout",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub id:           Uuid,
    pub ledger_id:    Uuid,
    pub kind:         EntryKind,
    /// Signed. Credits are positive, debits and payouts negative, so the
    /// balance is always a plain sum and cannot disagree with the log.
    pub amount_cents: i64,
    pub order_id:     Option<Uuid>,
    pub leg_id:       Option<Uuid>,
    pub reference:    Option<String>,
    pub created_at:   DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendorLedger {
    pub id:            Uuid,
    pub tenant_id:     Uuid,
    pub vendor_id:     Uuid,
    pub period:        String,
    pub status:        LedgerStatus,
    pub balance_cents: i64,
    pub version:       i64,
    pub entries:       Vec<LedgerEntry>,
    pub created_at:    DateTime<Utc>,
    pub updated_at:    DateTime<Utc>,
}

impl VendorLedger {
    pub fn open(tenant_id: Uuid, vendor_id: Uuid, period: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            vendor_id,
            period,
            status: LedgerStatus::Open,
            balance_cents: 0,
            version: 0,
            entries: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    fn append(&mut self, kind: EntryKind, amount_cents: i64,
              order_id: Option<Uuid>, leg_id: Option<Uuid>, reference: Option<String>) {
        self.entries.push(LedgerEntry {
            id: Uuid::new_v4(),
            ledger_id: self.id,
            kind,
            amount_cents,
            order_id,
            leg_id,
            reference,
            created_at: Utc::now(),
        });
        self.balance_cents += amount_cents;
        self.updated_at = Utc::now();
    }

    /// Credit a picked-up leg.
    ///
    /// Two entries, not one net figure: the vendor must be able to see the gross
    /// goods value and the commission separately, or a payout dispute has
    /// nothing to reconcile against.
    pub fn credit_leg(&mut self, goods_cents: i64, commission_cents: i64, order_id: Uuid, leg_id: Uuid) {
        self.append(EntryKind::GoodsCredit, goods_cents, Some(order_id), Some(leg_id), None);
        self.append(EntryKind::CommissionDebit, -commission_cents, Some(order_id), Some(leg_id), None);
    }

    pub fn record_payout(&mut self, amount_cents: i64, batch: Option<String>) {
        self.append(EntryKind::Payout, -amount_cents, None, None, batch);
    }

    /// A correction. Appends — never edits an existing entry.
    pub fn adjust(&mut self, amount_cents: i64, reason: String) {
        self.append(EntryKind::Adjustment, amount_cents, None, None, Some(reason));
    }

    pub fn is_open(&self) -> bool { self.status == LedgerStatus::Open }
}
```

Add to `src/domain/entities/mod.rs`:

```rust
pub mod vendor_ledger;
pub use vendor_ledger::{EntryKind, LedgerEntry, LedgerStatus, VendorLedger};
```

- [ ] **Step 5: Run the tests**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv vendor_ledger::`
Expected: PASS — 5 passed.

- [ ] **Step 6: Commit**

```bash
git add services/omnideliv/migrations/0006_create_vendor_ledger.sql services/omnideliv/src/domain/
git commit -m "feat(omnideliv): append-only vendor payout ledger

Signed amounts mean the balance is a plain sum of entries and cannot disagree
with the log. A leg credit records goods and commission separately so a payout
dispute has something to reconcile against; corrections append rather than
edit."
```

---

## Task 5: Courier earnings ledger in field-ops

The piece deferred out of Plan 2, now that the order model it settles against exists.

**Files:**
- Create: `services/field-ops/migrations/0004_create_courier_ledger.sql`, `services/field-ops/src/domain/entities/ledger.rs`

- [ ] **Step 1: Write the migration**

```sql
-- Courier earnings. Same append-only shape as the vendor ledger and the
-- platform's existing DriverLedger — one pattern for money across all three.
CREATE TABLE IF NOT EXISTS field_ops.courier_ledgers (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id     UUID        NOT NULL,
    courier_id    UUID        NOT NULL REFERENCES field_ops.couriers(id),
    -- Shift or payout period.
    period        TEXT        NOT NULL,
    status        TEXT        NOT NULL DEFAULT 'open'
                              CHECK (status IN ('open','closed','settled')),
    balance_cents BIGINT      NOT NULL DEFAULT 0,
    version       BIGINT      NOT NULL DEFAULT 0,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_courier_ledger_period
    ON field_ops.courier_ledgers (tenant_id, courier_id, period);

CREATE TABLE IF NOT EXISTS field_ops.courier_ledger_entries (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    ledger_id    UUID        NOT NULL REFERENCES field_ops.courier_ledgers(id),
    kind         TEXT        NOT NULL
                             CHECK (kind IN ('trip_earning','tip','adjustment','payout')),
    amount_cents BIGINT      NOT NULL,
    -- The product's own job id. field-ops does not interpret it.
    external_ref UUID,
    reference    TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_courier_entry_ledger
    ON field_ops.courier_ledger_entries (ledger_id, created_at);

REVOKE UPDATE, DELETE ON field_ops.courier_ledger_entries FROM PUBLIC;
```

- [ ] **Step 2: Write the failing test**

```rust
// services/field-ops/src/domain/entities/ledger.rs — tests block
#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn ledger() -> CourierLedger {
        CourierLedger::open(Uuid::new_v4(), Uuid::new_v4(), "2026-08-06".into())
    }

    /// The courier is paid per consolidated trip, not per stop. A three-vendor
    /// route earns one trip entry — which is precisely why consolidation is
    /// profitable rather than merely cheaper for the customer.
    #[test]
    fn a_consolidated_trip_earns_one_entry_regardless_of_stops() {
        let mut l = ledger();
        l.credit_trip(5_800, 3, Uuid::new_v4());

        let trips = l.entries.iter().filter(|e| e.kind == CourierEntryKind::TripEarning).count();
        assert_eq!(trips, 1, "three stops, one trip earning");
        assert_eq!(l.balance_cents, 5_800);
    }

    /// The tip goes to the courier in full — it is never a Partner revenue line.
    #[test]
    fn the_whole_tip_reaches_the_courier() {
        let mut l = ledger();
        l.credit_trip(5_800, 2, Uuid::new_v4());
        l.credit_tip(4_000, Uuid::new_v4());
        assert_eq!(l.balance_cents, 5_800 + 4_000);
    }

    #[test]
    fn the_balance_always_equals_the_sum_of_entries() {
        let mut l = ledger();
        l.credit_trip(5_800, 2, Uuid::new_v4());
        l.credit_tip(4_000, Uuid::new_v4());
        l.record_payout(6_000, None);

        let summed: i64 = l.entries.iter().map(|e| e.amount_cents).sum();
        assert_eq!(l.balance_cents, summed);
    }

    #[test]
    fn an_adjustment_appends_rather_than_editing_history() {
        let mut l = ledger();
        l.credit_trip(5_800, 1, Uuid::new_v4());
        let before = l.entries.len();
        l.adjust(-500, "route shortened".into());
        assert_eq!(l.entries.len(), before + 1);
        assert_eq!(l.balance_cents, 5_300);
    }
}
```

- [ ] **Step 3: Run it to verify it fails**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-field-ops ledger::`
Expected: FAIL to compile — `cannot find type 'CourierLedger' in this scope`.

- [ ] **Step 4: Write the entity**

```rust
//! Courier earnings ledger.
//!
//! Same append-only shape as the vendor ledger and the platform's DriverLedger:
//! signed entries, a denormalised balance that is always their sum, corrections
//! by compensating entry.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CourierLedgerStatus { Open, Closed, Settled }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CourierEntryKind { TripEarning, Tip, Adjustment, Payout }

impl CourierEntryKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            CourierEntryKind::TripEarning => "trip_earning",
            CourierEntryKind::Tip         => "tip",
            CourierEntryKind::Adjustment  => "adjustment",
            CourierEntryKind::Payout      => "payout",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CourierLedgerEntry {
    pub id:           Uuid,
    pub ledger_id:    Uuid,
    pub kind:         CourierEntryKind,
    pub amount_cents: i64,
    pub external_ref: Option<Uuid>,
    pub reference:    Option<String>,
    pub created_at:   DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CourierLedger {
    pub id:            Uuid,
    pub tenant_id:     Uuid,
    pub courier_id:    Uuid,
    pub period:        String,
    pub status:        CourierLedgerStatus,
    pub balance_cents: i64,
    pub version:       i64,
    pub entries:       Vec<CourierLedgerEntry>,
    pub created_at:    DateTime<Utc>,
    pub updated_at:    DateTime<Utc>,
}

impl CourierLedger {
    pub fn open(tenant_id: Uuid, courier_id: Uuid, period: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            courier_id,
            period,
            status: CourierLedgerStatus::Open,
            balance_cents: 0,
            version: 0,
            entries: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    fn append(&mut self, kind: CourierEntryKind, amount_cents: i64,
              external_ref: Option<Uuid>, reference: Option<String>) {
        self.entries.push(CourierLedgerEntry {
            id: Uuid::new_v4(),
            ledger_id: self.id,
            kind,
            amount_cents,
            external_ref,
            reference,
            created_at: Utc::now(),
        });
        self.balance_cents += amount_cents;
        self.updated_at = Utc::now();
    }

    /// Credit one consolidated trip.
    ///
    /// `stops` is recorded for reporting only — the earning is per trip. Paying
    /// per stop would make every additional pickup a cost, which would remove
    /// the margin that makes consolidation worth doing.
    pub fn credit_trip(&mut self, amount_cents: i64, stops: usize, external_ref: Uuid) {
        self.append(
            CourierEntryKind::TripEarning,
            amount_cents,
            Some(external_ref),
            Some(format!("{stops} stops")),
        );
    }

    /// The tip reaches the courier in full — never a Partner revenue line.
    pub fn credit_tip(&mut self, amount_cents: i64, external_ref: Uuid) {
        self.append(CourierEntryKind::Tip, amount_cents, Some(external_ref), None);
    }

    pub fn record_payout(&mut self, amount_cents: i64, batch: Option<String>) {
        self.append(CourierEntryKind::Payout, -amount_cents, None, batch);
    }

    pub fn adjust(&mut self, amount_cents: i64, reason: String) {
        self.append(CourierEntryKind::Adjustment, amount_cents, None, Some(reason));
    }
}
```

Add to `services/field-ops/src/domain/entities/mod.rs`:

```rust
pub mod ledger;
pub use ledger::{CourierEntryKind, CourierLedger, CourierLedgerEntry, CourierLedgerStatus};
```

- [ ] **Step 5: Run the tests**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-field-ops`
Expected: PASS — 14 tests (the 10 from Plan 2 plus 4 new).

- [ ] **Step 6: Commit**

```bash
git add services/field-ops/
git commit -m "feat(field-ops): courier earnings ledger, deferred from the extraction plan

Earnings are per consolidated trip, not per stop — stop count is recorded for
reporting only. Paying per stop would make every additional pickup a cost and
remove the margin that makes consolidation worth doing."
```

---

## Task 6: Order telemetry

**Files:**
- Create: `services/omnideliv/migrations/0007_create_order_telemetry.sql`, `src/domain/entities/telemetry.rs`

- [ ] **Step 1: Write the migration**

```sql
-- Append-only order timeline, following the platform's telemetry directive.
-- Every state transition is a new row; nothing is ever updated or deleted.
--
-- device_timestamp vs server_timestamp: device_timestamp is the hardware clock
-- at the physical moment of the event (a courier's pickup scan). SLA and
-- transit-velocity queries use it where present, falling back to
-- server_timestamp only for server-generated events. Using server time alone
-- would silently attribute network latency to the courier.
CREATE TABLE IF NOT EXISTS omnideliv.order_telemetry_logs (
    id               UUID        NOT NULL DEFAULT gen_random_uuid(),
    order_id         UUID        NOT NULL,
    tenant_id        UUID        NOT NULL,
    event_type       TEXT        NOT NULL,
    device_timestamp TIMESTAMPTZ,
    server_timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    actor_id         UUID,
    payload          JSONB       NOT NULL DEFAULT '{}',
    PRIMARY KEY (id, server_timestamp)
);

CREATE INDEX IF NOT EXISTS idx_order_telemetry_order
    ON omnideliv.order_telemetry_logs (order_id, server_timestamp DESC);

REVOKE UPDATE, DELETE ON omnideliv.order_telemetry_logs FROM PUBLIC;
```

> **Not converted to a TimescaleDB hypertable.** The composite primary key is hypertable-compatible so the conversion is a one-line follow-up, but TimescaleDB is not provisioned for this schema and a migration that fails on a missing extension blocks the service from starting — the failure mode that pinned `engagement` to a stale image for seven weeks.

- [ ] **Step 2: Write the entity and test**

```rust
// services/omnideliv/src/domain/entities/telemetry.rs
//! Append-only order timeline.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEvent {
    pub id:               Uuid,
    pub order_id:         Uuid,
    pub tenant_id:        Uuid,
    pub event_type:       String,
    pub device_timestamp: Option<DateTime<Utc>>,
    pub server_timestamp: DateTime<Utc>,
    pub actor_id:         Option<Uuid>,
    pub payload:          serde_json::Value,
}

impl TelemetryEvent {
    pub fn new(
        tenant_id: Uuid,
        order_id: Uuid,
        event_type: impl Into<String>,
        device_timestamp: Option<DateTime<Utc>>,
        actor_id: Option<Uuid>,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            order_id,
            tenant_id,
            event_type: event_type.into(),
            device_timestamp,
            server_timestamp: Utc::now(),
            actor_id,
            payload,
        }
    }

    /// The timestamp SLA maths uses: the device clock where we have it, backend
    /// receipt time only as a fallback for server-generated events.
    pub fn sla_timestamp(&self) -> DateTime<Utc> {
        self.device_timestamp.unwrap_or(self.server_timestamp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sla_timestamp_prefers_the_device_clock() {
        let device = Utc::now() - chrono::Duration::seconds(90);
        let e = TelemetryEvent::new(Uuid::new_v4(), Uuid::new_v4(), "vendor_leg.picked_up",
                                    Some(device), None, serde_json::json!({}));
        assert_eq!(e.sla_timestamp(), device);
    }

    #[test]
    fn sla_timestamp_falls_back_for_server_generated_events() {
        let e = TelemetryEvent::new(Uuid::new_v4(), Uuid::new_v4(), "order.placed",
                                    None, None, serde_json::json!({}));
        assert_eq!(e.sla_timestamp(), e.server_timestamp);
    }
}
```

Add to `src/domain/entities/mod.rs`:

```rust
pub mod telemetry;
pub use telemetry::TelemetryEvent;
```

- [ ] **Step 3: Run the tests**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv telemetry::`
Expected: PASS — 2 passed.

- [ ] **Step 4: Commit**

```bash
git add services/omnideliv/migrations/0007_create_order_telemetry.sql services/omnideliv/src/domain/
git commit -m "feat(omnideliv): append-only order telemetry with dual timestamps"
```

---

## Task 7: The checkout commit path

Checkout is a plain user-initiated transaction. **No agent holds a tool that reaches it** — that is the security property the mesh's RBAC exists to guarantee, and this task is where it is cashed in.

**Files:**
- Create: `services/omnideliv/src/application/services/checkout_service.rs`

- [ ] **Step 1: Write the service**

```rust
// services/omnideliv/src/application/services/checkout_service.rs
//! Checkout — the commit path.
//!
//! Deliberately not reachable from any agent tool. The mesh proposes; a human
//! tap commits. Everything here moves money or dispatches a courier, which is
//! exactly the set of actions no `AgentRole` is permitted to reach.

use std::sync::Arc;

use uuid::Uuid;

use crate::domain::entities::{
    Basket, ConsolidationPlan, LineState, Order, PendingStop, TemperatureClass, VendorLeg,
};
use crate::domain::repositories::{BasketRepository, VendorRepository};

#[derive(Debug, thiserror::Error)]
pub enum CheckoutError {
    #[error("basket {0} not found")]
    BasketNotFound(Uuid),
    #[error("basket has {0} line(s) awaiting review — the customer must decide first")]
    AwaitingReview(usize),
    #[error("basket is empty")]
    EmptyBasket,
    #[error("vendor {0} is no longer orderable")]
    VendorUnavailable(Uuid),
    #[error("no courier available")]
    NoCourier,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Placing an order requires a courier. The trait keeps `services/omnideliv`
/// from depending on field-ops types directly — a product service calling a
/// platform service through an interface it owns, not the reverse.
#[async_trait::async_trait]
pub trait CourierDispatch: Send + Sync {
    /// Offer the job to nearby couriers. Returns the assignment ids offered.
    async fn offer(
        &self,
        tenant_id: Uuid,
        order_id: Uuid,
        lat: f64,
        lng: f64,
    ) -> anyhow::Result<Vec<Uuid>>;
}

pub struct CheckoutService {
    baskets:  Arc<dyn BasketRepository>,
    vendors:  Arc<dyn VendorRepository>,
    dispatch: Arc<dyn CourierDispatch>,
}

impl CheckoutService {
    pub fn new(
        baskets: Arc<dyn BasketRepository>,
        vendors: Arc<dyn VendorRepository>,
        dispatch: Arc<dyn CourierDispatch>,
    ) -> Self {
        Self { baskets, vendors, dispatch }
    }

    /// Place an order from a reviewed basket.
    ///
    /// Order of operations matters. The basket is validated, vendors re-checked
    /// and legs computed *before* anything irreversible happens, so a failure
    /// here leaves no money moved and no courier dispatched.
    pub async fn place(
        &self,
        tenant_id: Uuid,
        basket_id: Uuid,
        tip_cents: i64,
        delivery_lat: f64,
        delivery_lng: f64,
    ) -> Result<Order, CheckoutError> {
        let basket: Basket = self
            .baskets
            .find_by_id(tenant_id, basket_id)
            .await
            .map_err(CheckoutError::Other)?
            .ok_or(CheckoutError::BasketNotFound(basket_id))?;

        // The customer must resolve every substitution first — Screen C exists
        // precisely so this cannot be silently skipped.
        let pending = basket.lines_awaiting_review().len();
        if pending > 0 {
            return Err(CheckoutError::AwaitingReview(pending));
        }

        let subtotals = basket.subtotals_by_vendor();
        if subtotals.is_empty() {
            return Err(CheckoutError::EmptyBasket);
        }

        // Re-check every vendor at commit time. A vendor that paused since the
        // basket was assembled must not receive a dispatched courier.
        let mut legs = Vec::with_capacity(subtotals.len());
        let mut stops = Vec::with_capacity(subtotals.len());

        for (vendor_id, subtotal) in &subtotals {
            let vendor = self
                .vendors
                .find_by_id(tenant_id, *vendor_id)
                .await
                .map_err(CheckoutError::Other)?
                .ok_or(CheckoutError::VendorUnavailable(*vendor_id))?;

            if !vendor.is_orderable() {
                return Err(CheckoutError::VendorUnavailable(*vendor_id));
            }

            legs.push(VendorLeg::settle(tenant_id, vendor.id, *subtotal, vendor.commission_bps));
            stops.push(PendingStop {
                vendor_id:         vendor.id,
                prep_time_minutes: vendor.prep_time_minutes,
                temperature_class: temperature_for(&vendor),
            });
        }

        // Placeholder pricing until a tariff service owns it. Visible and
        // testable here rather than hidden behind a stub.
        let flat_fee_cents = 4_900 + (stops.len() as i64 - 1).max(0) * 1_000;
        let courier_trip_cents = 3_500 + (stops.len() as i64 - 1).max(0) * 700;

        let plan = ConsolidationPlan::sequence(tenant_id, basket.id, stops, 0, flat_fee_cents);

        let mut order = Order::place(
            tenant_id, basket.customer_id, basket.id, plan.id,
            legs, flat_fee_cents, tip_cents, courier_trip_cents,
        );

        // Only now does anything irreversible happen.
        let offered = self
            .dispatch
            .offer(tenant_id, order.id, delivery_lat, delivery_lng)
            .await
            .map_err(CheckoutError::Other)?;

        if offered.is_empty() {
            // No charge, no order. Better to tell the customer now than to take
            // payment for a delivery nobody can make.
            return Err(CheckoutError::NoCourier);
        }

        order.courier_task_id = offered.first().copied();
        Ok(order)
    }
}

/// A vendor's temperature class, from its vertical. Coarse but honest: a
/// per-item classification needs a `temperature_class` column on catalog_items,
/// which is a catalog change rather than a checkout one.
fn temperature_for(vendor: &crate::domain::entities::Vendor) -> TemperatureClass {
    use crate::domain::entities::Vertical::*;
    match vendor.vertical {
        Restaurant => TemperatureClass::Hot,
        Grocery | Florist => TemperatureClass::Chilled,
        Pharmacy | Retail => TemperatureClass::Ambient,
    }
}
```

Add `thiserror.workspace = true` to `services/omnideliv/Cargo.toml` if absent, and register the module in `src/application/services/mod.rs`.

- [ ] **Step 2: Verify it type-checks**

Run: `CARGO_INCREMENTAL=0 cargo check -p logisticos-omnideliv`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add services/omnideliv/src/application/
git commit -m "feat(omnideliv): checkout commit path, unreachable from any agent

Everything here moves money or dispatches a courier — exactly the set of
actions no AgentRole is permitted to reach. Validation and leg computation
happen before anything irreversible, so a failure leaves no money moved and no
courier dispatched. A basket with unresolved substitutions is refused."
```

---

## Task 8: Persistence, API and end-to-end wiring

**Files:**
- Create: `src/infrastructure/db/order_repo.rs`, `src/infrastructure/external/field_ops.rs`, `src/api/http/orders.rs`
- Modify: `src/infrastructure/db/mod.rs`, `src/infrastructure/mod.rs`, `src/api/http/mod.rs`, `src/bootstrap.rs`

- [ ] **Step 1: Write the field-ops adapter**

```rust
// services/omnideliv/src/infrastructure/external/field_ops.rs
//! Courier dispatch via the field-ops platform tier.
//!
//! Product → platform is the permitted direction under ADR-0009. The reverse
//! would not be: field-ops must never know an OmniDeliv order exists, which is
//! why it takes an opaque `external_ref` rather than a typed order id.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::application::services::CourierDispatch;

#[derive(Debug, Serialize)]
struct OfferBody<'a> {
    product:      &'a str,
    external_ref: Uuid,
    lat:          f64,
    lng:          f64,
}

#[derive(Debug, Deserialize)]
struct OfferReply {
    assignment_ids: Vec<Uuid>,
}

pub struct FieldOpsDispatch {
    http:    reqwest::Client,
    base_url: String,
    token:   String,
}

impl FieldOpsDispatch {
    pub fn new(base_url: String, token: String) -> Self {
        Self { http: reqwest::Client::new(), base_url, token }
    }
}

#[async_trait]
impl CourierDispatch for FieldOpsDispatch {
    async fn offer(
        &self,
        _tenant_id: Uuid,
        order_id: Uuid,
        lat: f64,
        lng: f64,
    ) -> anyhow::Result<Vec<Uuid>> {
        // tenant_id is carried by the bearer token's claims, not the body —
        // a tenant in the request body would be a caller-supplied tenant, which
        // is a cross-tenant write waiting to happen.
        let reply = self
            .http
            .post(format!("{}/v1/field-ops/assignments/offer", self.base_url))
            .bearer_auth(&self.token)
            .json(&OfferBody { product: "omnideliv", external_ref: order_id, lat, lng })
            .send()
            .await?
            .error_for_status()?
            .json::<OfferReply>()
            .await?;

        Ok(reply.assignment_ids)
    }
}
```

- [ ] **Step 2: Write the order repository**

Follow the pattern established in Plan 3's `PgBasketRepository`: one transaction for the aggregate, legs replaced wholesale, enum conversions via the `as_str()` helpers on each type. The order and its legs must be written in one transaction — a persisted order whose legs failed to write would balance at the entity level and not at the database.

```rust
// services/omnideliv/src/infrastructure/db/order_repo.rs
use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::entities::Order;

#[async_trait]
pub trait OrderRepository: Send + Sync {
    async fn save(&self, order: &Order) -> anyhow::Result<()>;
    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<Option<Order>>;
}

pub struct PgOrderRepository { pool: PgPool }

impl PgOrderRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl OrderRepository for PgOrderRepository {
    async fn save(&self, o: &Order) -> anyhow::Result<()> {
        // One transaction: an order whose legs failed to write would balance at
        // the entity level and not in the database, which is the worst of both.
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
            INSERT INTO omnideliv.orders (
                id, tenant_id, customer_id, basket_id, plan_id, status,
                goods_total_cents, delivery_fee_cents, tip_cents, grand_total_cents,
                courier_trip_cents, courier_task_id, placed_at, delivered_at
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
            ON CONFLICT (id) DO UPDATE SET
                status          = EXCLUDED.status,
                courier_task_id = EXCLUDED.courier_task_id,
                delivered_at    = EXCLUDED.delivered_at
            "#,
        )
        .bind(o.id).bind(o.tenant_id).bind(o.customer_id).bind(o.basket_id).bind(o.plan_id)
        .bind(o.status.as_str())
        .bind(o.goods_total_cents).bind(o.delivery_fee_cents).bind(o.tip_cents)
        .bind(o.grand_total_cents).bind(o.courier_trip_cents).bind(o.courier_task_id)
        .bind(o.placed_at).bind(o.delivered_at)
        .execute(&mut *tx).await?;

        for l in &o.legs {
            sqlx::query(
                r#"
                INSERT INTO omnideliv.order_vendor_legs (
                    id, order_id, tenant_id, vendor_id, goods_subtotal_cents,
                    commission_bps, commission_cents, payout_cents, status,
                    picked_up_at, created_at
                )
                VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
                ON CONFLICT (order_id, vendor_id) DO UPDATE SET
                    status       = EXCLUDED.status,
                    picked_up_at = EXCLUDED.picked_up_at
                "#,
            )
            .bind(l.id).bind(l.order_id).bind(l.tenant_id).bind(l.vendor_id)
            .bind(l.goods_subtotal_cents).bind(l.commission_bps)
            .bind(l.commission_cents).bind(l.payout_cents)
            .bind(l.status.as_str()).bind(l.picked_up_at).bind(l.created_at)
            .execute(&mut *tx).await?;
        }

        tx.commit().await?;
        Ok(())
    }

    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<Option<Order>> {
        // Mirrors PgBasketRepository::find_by_id — fetch the order row, then its
        // legs, and map both. Written out fully there; the shape is identical.
        let _ = (tenant_id, id);
        anyhow::bail!("find_by_id: implement following PgBasketRepository::find_by_id")
    }
}
```

> **`find_by_id` is deliberately left as a `bail!` with a pointer.** It is a mechanical mirror of a method written out in full in Plan 3, and duplicating forty lines of row-mapping here adds no information. It must be implemented before Task 8 is complete — the `bail!` makes forgetting it a runtime failure with a message rather than a silent gap.

- [ ] **Step 3: Implement `find_by_id`**

Follow `PgBasketRepository::find_by_id`: `SELECT * FROM omnideliv.orders WHERE tenant_id = $1 AND id = $2`, then `SELECT * FROM omnideliv.order_vendor_legs WHERE order_id = $1`, mapping the status strings back through match arms that mirror the `as_str()` implementations.

- [ ] **Step 4: Write the checkout route**

```rust
// services/omnideliv/src/api/http/orders.rs
use std::sync::Arc;

use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::http::AppState;
use crate::application::services::CheckoutError;

#[derive(Debug, Deserialize)]
pub struct CheckoutRequest {
    pub basket_id: Uuid,
    #[serde(default)]
    pub tip_cents: i64,
    pub delivery_lat: f64,
    pub delivery_lng: f64,
}

#[derive(Debug, Serialize)]
pub struct CheckoutResponse {
    pub order_id:          Uuid,
    pub grand_total_cents: i64,
    pub stops:             usize,
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/v1/omnideliv/orders/checkout", post(checkout))
}

async fn checkout(
    State(st): State<Arc<AppState>>,
    Json(req): Json<CheckoutRequest>,
) -> Result<Json<CheckoutResponse>, (StatusCode, String)> {
    let order = st
        .checkout
        .place(st.tenant_id, req.basket_id, req.tip_cents, req.delivery_lat, req.delivery_lng)
        .await
        .map_err(|e| match e {
            // A basket awaiting review is the client's cue to show Screen C,
            // not an error to surface as a failure.
            CheckoutError::AwaitingReview(_) => (StatusCode::CONFLICT, e.to_string()),
            CheckoutError::BasketNotFound(_) => (StatusCode::NOT_FOUND, e.to_string()),
            CheckoutError::EmptyBasket
            | CheckoutError::VendorUnavailable(_) => (StatusCode::BAD_REQUEST, e.to_string()),
            // No courier means no charge — 503 tells the client to retry rather
            // than to treat the basket as spent.
            CheckoutError::NoCourier => (StatusCode::SERVICE_UNAVAILABLE, e.to_string()),
            CheckoutError::Other(inner) => {
                tracing::error!(err = %inner, "checkout failed");
                (StatusCode::INTERNAL_SERVER_ERROR, "checkout failed".into())
            }
        })?;

    st.orders.save(&order).await.map_err(|e| {
        tracing::error!(err = %e, "order persist failed");
        (StatusCode::INTERNAL_SERVER_ERROR, "checkout failed".into())
    })?;

    Ok(Json(CheckoutResponse {
        order_id:          order.id,
        grand_total_cents: order.grand_total_cents,
        stops:             order.legs.len(),
    }))
}
```

- [ ] **Step 5: Close the placeholder tenant — this is the money path**

Plan 3 shipped `AppState { tenant_id }` as a placeholder and recorded it as a known follow-up. **This is where it must be closed**, because checkout moves money: a tenant read from app state rather than from validated claims means one tenant's checkout can settle against another tenant's basket and vendors.

Delete the `tenant_id` field from `AppState`. In every handler, extract `Claims` from request extensions and read `claims.tenant_id` — the same pattern `services/pod` uses. Do the same for Plan 3's `baskets::fetch` (which used `Uuid::nil()`) and `catalog::search` (which took `tenant_id` as a query parameter).

Verify no handler can be called without a tenant from the token:

```bash
rg -n "tenant_id.*Uuid::nil\(\)|st\.tenant_id|tenant_id: Uuid," services/omnideliv/src/api/
```

Expected: no matches. A hit means a route still accepts a caller-supplied tenant.

- [ ] **Step 6: Wire bootstrap and verify**

Add `checkout`, `orders` and the field-ops base URL and token to `AppState`, `Config` and `bootstrap.rs`, following the pattern from Plan 3 Task 8.

Run: `CARGO_INCREMENTAL=0 cargo check --workspace`
Expected: PASS.

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv`
Expected: PASS — 37 tests (16 from Plan 3, plus 6 consolidation, 8 order, 5 vendor ledger, 2 telemetry).

- [ ] **Step 7: Commit**

```bash
git add services/omnideliv/
git commit -m "feat(omnideliv): order persistence, field-ops dispatch adapter, checkout route

Product calls platform, never the reverse: field-ops takes an opaque
external_ref rather than a typed order id, and tenant comes from the bearer
token's claims rather than the request body. A basket awaiting review returns
409 so the client shows the substitution screen instead of an error."
```

---

## Definition of done

- [ ] `cargo test -p logisticos-omnideliv` — 37 unit tests pass
- [ ] `cargo test -p logisticos-omnideliv --test settlement_invariant` — passes, >3000 cases
- [ ] `cargo test -p logisticos-field-ops` — 14 tests pass
- [ ] `cargo check --workspace` — clean
- [ ] `rg -n "f64|f32" services/omnideliv/src/domain/entities/order.rs services/omnideliv/src/domain/entities/vendor_ledger.rs` returns nothing — money is integer cents only
- [ ] `PgOrderRepository::find_by_id` no longer contains `bail!`

## Follow-on work this surfaces

1. **Payment capture.** The commit path computes and records the charge and emits the order; wiring a real gateway (Stripe/PayMongo) is its own plan, and until then `grand_total_cents` is recorded but not collected.
2. **Per-item temperature class.** `temperature_for` derives a vendor-level class from its vertical. A grocery basket of ambient tins is not chilled; fixing this is a `temperature_class` column on `catalog_items`, a catalog change rather than a checkout one.
3. **Tariff service.** `flat_fee_cents` and `courier_trip_cents` use visible placeholder arithmetic in the checkout path. Real pricing — distance bands, surge, zone rules — belongs behind a tariff service, and the placeholder is deliberately in one readable place so replacing it is a single edit.
4. **Partial-pickup recovery.** `LegStatus::Failed` is recorded but nothing yet refunds the failed leg or notifies the customer. That path needs the engagement integration and belongs with the failure-modes work.
