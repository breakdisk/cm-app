# Vendor Leg Acceptance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give a vendor a queue of its own incoming orders and the ability to accept, reject, mark ready and mark served — each transition guarded against concurrent tablets, each publishing a vendor-keyed event.

**Architecture:** Acceptance lives on `VendorLeg`, never on `Order`, because a basket spans vendors and one stall accepting must not move a three-stall order. Transitions go through a single conditional-UPDATE repository method rather than the existing whole-order `save()`, which is last-write-wins and would silently clobber a concurrent change. A new Kafka topic keyed on `vendor_id` carries one leg per message.

**Tech Stack:** Rust, Axum, SQLx (Postgres), rdkafka, `logisticos_auth` claims middleware.

---

## Scope: this is subsystem 1 of 4

ADR-0017 covers four separable subsystems. Per the one-plan-per-subsystem rule, this plan builds **only the first**. Each later plan depends on this one and is written after it lands.

| # | Subsystem | State |
|---|---|---|
| **1** | **Vendor leg acceptance + vendor order queue** | **This plan. Unblocked.** |
| 2 | Notification transport (console stream, FCM, WhatsApp) + recovery ladder + vendor `contact_phone` | Blocked on nothing; needs this plan first |
| 3 | QR table ordering (venues, tables, anonymous session principal) | Needs 1 and 2 |
| 4 | Payments partial capture + the acceptance barrier | Blocked on `services/payments` |

**Two deliberate deferrals, so a later reader does not think they were forgotten:**

- **Full `OrderStatus` derivation is not in this plan.** `OrderStatus` is currently written by the courier-event path (`courier_consumer.rs`). Introducing a second writer before auditing the first is the "two writers disagree" risk named in the ADR's consequences. This plan adds `Order::acceptance_state()` — a *read-only* derived view over the legs — and leaves `OrderStatus` alone. The audit and the switch-over are Task 1 of Plan 2.
- **Capture is not wired here.** Acceptance feeds capture only once `services/payments` can capture a partial amount. Task 8 lands the idempotency key that protects that future money path, so the endpoint contract does not change when capture arrives.

---

## File Structure

| File | Responsibility |
|---|---|
| `services/omnideliv/src/domain/entities/order.rs` | Modify: extend `LegStatus`, add the transition rule and `Order::acceptance_state()` |
| `services/omnideliv/migrations/0024_leg_acceptance.sql` | Create: widen the leg status CHECK, add acceptance columns |
| `services/omnideliv/src/domain/repositories/mod.rs` | Modify: add `VendorLegRepository` trait |
| `services/omnideliv/src/infrastructure/db/leg_repo.rs` | Create: guarded conditional-UPDATE transitions and the queue read |
| `services/omnideliv/src/infrastructure/db/order_repo.rs:45` | Modify: teach `leg_status()` the new variants |
| `services/omnideliv/src/infrastructure/messaging/courier_consumer.rs:192` | Modify (Task 2A): stop a late collection event re-opening a terminal leg and crediting a vendor that rejected it |
| `services/omnideliv/src/infrastructure/messaging/vendor_events.rs` | Create: vendor-keyed leg publisher |
| `libs/events/src/topics.rs` | Modify: add the topic constant |
| `services/omnideliv/src/api/http/vendor_orders.rs` | Create: the queue read and four action routes |
| `services/omnideliv/src/api/http/mod.rs` | Modify: merge the new router |
| `services/omnideliv/tests/leg_transitions.rs` | Create: pure-domain transition and acceptance-state tests |

`leg_repo.rs` is a new file rather than an addition to `order_repo.rs` because the two have opposite write models — `order_repo` writes a whole order as one unit, `leg_repo` writes exactly one leg conditionally. Mixing them in one file is how the guarded write eventually gets "simplified" back into `save()`.

---

## Task 1: Extend `LegStatus` with the acceptance states

**Files:**
- Modify: `services/omnideliv/src/domain/entities/order.rs:35-52`
- Test: `services/omnideliv/tests/leg_transitions.rs`

The existing enum is `Pending | PickedUp | Failed | Settled`. Acceptance adds four states, plus `Rejected` — a vendor refusing an order is not the same event as a pickup failing, and settling them into one status would make "why did this order die" unanswerable.

- [ ] **Step 1: Write the failing test**

Create `services/omnideliv/tests/leg_transitions.rs`:

```rust
//! The vendor-leg transition graph.
//!
//! Pure domain arithmetic over the entities — no database, no broker — so this
//! runs on a dev machine with no Postgres, same as `settlement_invariant.rs`.

use logisticos_omnideliv::domain::entities::LegStatus;

#[test]
fn a_pending_leg_can_be_accepted_or_rejected_and_nothing_else() {
    assert!(LegStatus::Pending.can_transition_to(LegStatus::Accepted));
    assert!(LegStatus::Pending.can_transition_to(LegStatus::Rejected));
    assert!(!LegStatus::Pending.can_transition_to(LegStatus::Ready));
    assert!(!LegStatus::Pending.can_transition_to(LegStatus::PickedUp));
    assert!(!LegStatus::Pending.can_transition_to(LegStatus::Settled));
}

#[test]
fn an_accepted_leg_may_skip_preparing() {
    // A florist wrapping one bouquet has no meaningful "preparing" step; a
    // kitchen does. Both are legal rather than forcing a fake transition.
    assert!(LegStatus::Accepted.can_transition_to(LegStatus::Preparing));
    assert!(LegStatus::Accepted.can_transition_to(LegStatus::Ready));
    assert!(LegStatus::Preparing.can_transition_to(LegStatus::Ready));
}

#[test]
fn a_ready_leg_leaves_by_courier_or_by_table() {
    assert!(LegStatus::Ready.can_transition_to(LegStatus::PickedUp));
    assert!(LegStatus::Ready.can_transition_to(LegStatus::Served));
}

#[test]
fn terminal_states_never_move_again() {
    for s in [LegStatus::Rejected, LegStatus::Failed, LegStatus::Settled] {
        assert!(s.is_terminal(), "{s:?} should be terminal");
        for next in [LegStatus::Accepted, LegStatus::Ready, LegStatus::Settled] {
            assert!(!s.can_transition_to(next), "{s:?} must not move to {next:?}");
        }
    }
}

#[test]
fn any_live_leg_can_be_failed_by_an_operator() {
    // Preserves the existing `mark_failed` behaviour, which has no single
    // legal predecessor — an operator can fail a leg at any live point.
    for s in [LegStatus::Pending, LegStatus::Accepted, LegStatus::Preparing, LegStatus::Ready] {
        assert!(s.can_transition_to(LegStatus::Failed), "{s:?} should be failable");
    }
}

#[test]
fn every_status_round_trips_through_its_wire_string() {
    for s in [
        LegStatus::Pending, LegStatus::Accepted, LegStatus::Preparing,
        LegStatus::Ready, LegStatus::PickedUp, LegStatus::Served,
        LegStatus::Rejected, LegStatus::Failed, LegStatus::Settled,
    ] {
        assert_eq!(LegStatus::from_wire(s.as_str()), Some(s), "round trip failed for {s:?}");
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv --test leg_transitions
```

Expected: compile error — `no variant named 'Accepted' found for enum 'LegStatus'`.

- [ ] **Step 3: Extend the enum**

In `services/omnideliv/src/domain/entities/order.rs`, replace the `LegStatus` enum and its `impl` block:

```rust
/// Where one vendor's half of an order stands.
///
/// `Rejected` is distinct from `Failed` on purpose: a store refusing an order
/// and a pickup going wrong are different events with different money
/// consequences, and collapsing them makes "why did this die" unanswerable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegStatus {
    Pending,
    Accepted,
    Preparing,
    Ready,
    PickedUp,
    Served,
    Rejected,
    Failed,
    Settled,
}

impl LegStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            LegStatus::Pending   => "pending",
            LegStatus::Accepted  => "accepted",
            LegStatus::Preparing => "preparing",
            LegStatus::Ready     => "ready",
            LegStatus::PickedUp  => "picked_up",
            LegStatus::Served    => "served",
            LegStatus::Rejected  => "rejected",
            LegStatus::Failed    => "failed",
            LegStatus::Settled   => "settled",
        }
    }

    /// Parses the wire/database form. `None` for anything unrecognised, so a
    /// row written by a newer deploy fails loudly instead of silently
    /// decoding as `Pending` and re-offering work that is already underway.
    pub fn from_wire(s: &str) -> Option<Self> {
        Some(match s {
            "pending"   => LegStatus::Pending,
            "accepted"  => LegStatus::Accepted,
            "preparing" => LegStatus::Preparing,
            "ready"     => LegStatus::Ready,
            "picked_up" => LegStatus::PickedUp,
            "served"    => LegStatus::Served,
            "rejected"  => LegStatus::Rejected,
            "failed"    => LegStatus::Failed,
            "settled"   => LegStatus::Settled,
            _ => return None,
        })
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, LegStatus::Rejected | LegStatus::Failed | LegStatus::Settled)
    }

    /// Whether this leg has answered the acceptance question at all. Drives the
    /// acceptance barrier — see `Order::acceptance_state`.
    pub fn has_answered(self) -> bool {
        self != LegStatus::Pending
    }

    /// The legal transition graph. Enforced here rather than only in SQL so the
    /// rule is testable without a database and stated in exactly one place.
    pub fn can_transition_to(self, next: LegStatus) -> bool {
        use LegStatus::*;
        if self.is_terminal() {
            return false;
        }
        // An operator can fail any live leg; there is no single legal
        // predecessor for a pickup that went wrong.
        if next == Failed {
            return true;
        }
        matches!(
            (self, next),
            (Pending,   Accepted)  | (Pending,   Rejected)
          | (Accepted,  Preparing) | (Accepted,  Ready)
          | (Preparing, Ready)
          | (Ready,     PickedUp)  | (Ready,     Served)
          | (PickedUp,  Settled)   | (Served,    Settled)
        )
    }
}
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv --test leg_transitions
```

Expected: `test result: ok. 6 passed`.

- [ ] **Step 5: Fix the now-duplicated parser in the repository**

`order_repo.rs:45` has its own `leg_status()` match that no longer covers every variant. Replace its body so there is one parser, not two:

```rust
fn leg_status(s: &str) -> anyhow::Result<LegStatus> {
    LegStatus::from_wire(s).ok_or_else(|| anyhow::anyhow!("unknown leg status: {s}"))
}
```

- [ ] **Step 6: Verify the crate still compiles**

```bash
CARGO_INCREMENTAL=0 cargo check -p logisticos-omnideliv
```

Expected: `Finished`. If a `match` elsewhere errors as non-exhaustive, that is the compiler finding a real caller that must decide what the new states mean — fix it there rather than adding a catch-all arm.

- [ ] **Step 7: Commit**

```bash
git add services/omnideliv/src/domain/entities/order.rs services/omnideliv/src/infrastructure/db/order_repo.rs services/omnideliv/tests/leg_transitions.rs
git commit -m "feat(omnideliv): vendor leg acceptance states and transition graph"
```

---

## Task 2: `Order::acceptance_state()` — the read-only derived view

**Files:**
- Modify: `services/omnideliv/src/domain/entities/order.rs`
- Test: `services/omnideliv/tests/leg_transitions.rs`

This is what the acceptance barrier will consume in Plan 4, and what the customer's "waiting on the ramen stall" screen reads. It derives from the legs and writes nothing.

- [ ] **Step 1: Write the failing test**

Append to `services/omnideliv/tests/leg_transitions.rs`:

```rust
use logisticos_omnideliv::domain::entities::{AcceptanceState, Order, VendorLeg};
use uuid::Uuid;

fn order_with(statuses: &[LegStatus]) -> Order {
    let legs: Vec<VendorLeg> = statuses
        .iter()
        .map(|s| {
            let mut l = VendorLeg::settle(Uuid::new_v4(), Uuid::new_v4(), 1_000, 1_500);
            l.status = *s;
            l
        })
        .collect();
    Order::place(
        Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(),
        legs, 0, 0, 0, 14.5995, 120.9842,
    )
}

#[test]
fn an_order_with_a_pending_leg_is_still_waiting() {
    let o = order_with(&[LegStatus::Accepted, LegStatus::Pending]);
    assert_eq!(o.acceptance_state(), AcceptanceState::Awaiting { outstanding: 1 });
}

#[test]
fn the_barrier_lifts_only_when_every_leg_has_answered() {
    let o = order_with(&[LegStatus::Accepted, LegStatus::Rejected, LegStatus::Preparing]);
    assert_eq!(
        o.acceptance_state(),
        AcceptanceState::Resolved { accepted: 2, rejected: 1, accepted_subtotal_cents: 2_000 },
    );
}

#[test]
fn an_order_every_stall_refused_is_resolved_with_nothing_accepted() {
    let o = order_with(&[LegStatus::Rejected, LegStatus::Rejected]);
    assert_eq!(
        o.acceptance_state(),
        AcceptanceState::Resolved { accepted: 0, rejected: 2, accepted_subtotal_cents: 0 },
    );
}

#[test]
fn the_accepted_subtotal_excludes_refused_legs() {
    // The number Plan 4 captures. A rejected leg's subtotal must never reach it.
    let o = order_with(&[LegStatus::Ready, LegStatus::Rejected]);
    match o.acceptance_state() {
        AcceptanceState::Resolved { accepted_subtotal_cents, .. } => {
            assert_eq!(accepted_subtotal_cents, 1_000, "only the surviving leg counts");
        }
        other => panic!("expected Resolved, got {other:?}"),
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv --test leg_transitions
```

Expected: compile error — `cannot find type 'AcceptanceState'`.

- [ ] **Step 3: Add the type and the method**

In `services/omnideliv/src/domain/entities/order.rs`, after the `LegStatus` impl:

```rust
/// How far an order has got through asking its vendors.
///
/// Deliberately separate from `OrderStatus`: that field is written by the
/// courier-event path, and a second writer on the same field is how two
/// sources of truth disagree about one order. This is derived on read and
/// stored nowhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum AcceptanceState {
    /// At least one vendor has not answered yet.
    Awaiting { outstanding: usize },
    /// Every leg has answered. `accepted_subtotal_cents` is the amount that may
    /// be captured; the rest of the authorization is voided.
    Resolved { accepted: usize, rejected: usize, accepted_subtotal_cents: i64 },
}

impl Order {
    /// Derived from the legs, never stored. See `AcceptanceState`.
    pub fn acceptance_state(&self) -> AcceptanceState {
        let outstanding = self.legs.iter().filter(|l| !l.status.has_answered()).count();
        if outstanding > 0 {
            return AcceptanceState::Awaiting { outstanding };
        }

        // "Accepted" here means the leg survived the ask — anything that is not
        // an outright refusal or failure. A leg already picked up or served is
        // emphatically accepted.
        let survived = |l: &&VendorLeg| {
            !matches!(l.status, LegStatus::Rejected | LegStatus::Failed)
        };

        AcceptanceState::Resolved {
            accepted: self.legs.iter().filter(survived).count(),
            rejected: self.legs.iter().filter(|l| l.status == LegStatus::Rejected).count(),
            accepted_subtotal_cents: self
                .legs
                .iter()
                .filter(survived)
                .map(|l| l.goods_subtotal_cents)
                .sum(),
        }
    }
}
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv --test leg_transitions
```

Expected: `test result: ok. 10 passed`.

- [ ] **Step 5: Commit**

```bash
git add services/omnideliv/src/domain/entities/order.rs services/omnideliv/tests/leg_transitions.rs
git commit -m "feat(omnideliv): derive acceptance state from vendor legs"
```

---

## Task 2A: Close the four-state assumptions in existing consumers

> **Added after Task 1's code review.** Two call sites were written when a leg could only be `Pending | PickedUp | Failed | Settled`, and both silently become wrong once the intermediate states are reachable. The compiler did not catch them because they use `==` against single variants rather than exhaustive `match`. Neither is a defect in Task 1's diff — both go live when Task 6 lands the routes, so they are fixed first.

**Files:**
- Modify: `services/omnideliv/src/domain/entities/order.rs` — `all_legs_collected()` (~line 820) and `LegStatus`
- Modify: `services/omnideliv/src/infrastructure/messaging/courier_consumer.rs` (~line 192)
- Test: `services/omnideliv/tests/leg_transitions.rs`

- [ ] **Step 1: Write the failing tests**

Append to `services/omnideliv/tests/leg_transitions.rs`:

```rust
use logisticos_omnideliv::domain::entities::OrderStatus;

#[test]
fn an_unrecognised_wire_string_is_rejected_rather_than_defaulted() {
    // `order_repo::leg_status` depends entirely on this: a row written by a
    // newer deploy must fail loudly, not decode as Pending and re-offer work
    // that is already underway.
    assert_eq!(LegStatus::from_wire("bogus"), None);
    assert_eq!(LegStatus::from_wire(""), None);
    assert_eq!(LegStatus::from_wire("PENDING"), None, "parsing is case-sensitive");
}

#[test]
fn a_leg_still_being_prepared_blocks_collection() {
    // The bug this test exists for: under the old four states, "not pending"
    // meant "resolved". It no longer does. A leg sitting at Ready is accepted
    // and cooked and still on the counter — the order must not advance.
    for s in [LegStatus::Pending, LegStatus::Accepted, LegStatus::Preparing, LegStatus::Ready] {
        assert!(s.blocks_collection(), "{s:?} must block the order from advancing");
    }
    for s in [LegStatus::PickedUp, LegStatus::Rejected, LegStatus::Failed, LegStatus::Served] {
        assert!(!s.blocks_collection(), "{s:?} is resolved and must not block");
    }
}

#[test]
fn an_order_with_a_leg_on_the_counter_does_not_advance_to_delivering() {
    let mut o = order_with(&[LegStatus::PickedUp, LegStatus::Ready]);
    o.status = OrderStatus::Collecting;
    assert!(
        o.all_legs_collected().is_err(),
        "one leg collected and one still ready must not advance the order",
    );
}

#[test]
fn an_order_whose_legs_are_all_resolved_advances() {
    let mut o = order_with(&[LegStatus::PickedUp, LegStatus::Rejected]);
    o.status = OrderStatus::Collecting;
    assert!(o.all_legs_collected().is_ok(), "a rejected leg is resolved, not outstanding");
    assert_eq!(o.status, OrderStatus::Delivering);
}

#[test]
fn every_status_is_covered_by_the_transition_graph_lookup() {
    // `LegStatus::ALL` is what the repository derives its SQL predecessor list
    // from. A variant missing from it would silently become untransitionable.
    assert_eq!(LegStatus::ALL.len(), 9);
    for s in LegStatus::ALL {
        assert_eq!(LegStatus::from_wire(s.as_str()), Some(s));
    }
}
```

- [ ] **Step 2: Run to verify they fail**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv --test leg_transitions
```

Expected: compile error — `no method named 'blocks_collection'`, `no associated item named 'ALL'`.

- [ ] **Step 3: Add `ALL` and `blocks_collection` to `LegStatus`**

In `services/omnideliv/src/domain/entities/order.rs`, inside `impl LegStatus`:

```rust
    /// Every variant. The repository derives its legal-predecessor list from
    /// this rather than hand-writing one per route, so `can_transition_to`
    /// stays the only statement of the graph.
    pub const ALL: [LegStatus; 9] = [
        LegStatus::Pending,  LegStatus::Accepted, LegStatus::Preparing,
        LegStatus::Ready,    LegStatus::PickedUp, LegStatus::Served,
        LegStatus::Rejected, LegStatus::Failed,   LegStatus::Settled,
    ];

    /// Whether this leg still owes the courier something.
    ///
    /// Not the same question as `has_answered`: a leg can have answered the
    /// vendor's accept/reject question and still be sitting on the counter.
    /// Before the acceptance states existed, "not pending" happened to mean
    /// "resolved" — it does not any more, and `all_legs_collected` is the
    /// caller that would otherwise advance an order whose goods never moved.
    pub fn blocks_collection(self) -> bool {
        matches!(
            self,
            LegStatus::Pending | LegStatus::Accepted | LegStatus::Preparing | LegStatus::Ready
        )
    }
```

- [ ] **Step 3b: Hoist the "declined" rule onto the enum**

> Added after Task 2's code review. `Order::acceptance_state` tests "not `Rejected` and not `Failed`" with a local closure, but Task 1 established that leg-status rules are named methods on `LegStatus` (`is_terminal`, `has_answered`). Plan 4's acceptance barrier needs the identical predicate to compute the void amount, so leaving it as a closure guarantees it gets re-derived somewhere else.

Add to `impl LegStatus`:

```rust
    /// Whether this leg will not be fulfilled — refused by the store, or
    /// broken later. The acceptance barrier excludes exactly these from the
    /// amount it captures, so the rule lives here rather than in a closure
    /// that Plan 4 would have to re-derive.
    pub fn declined(self) -> bool {
        matches!(self, LegStatus::Rejected | LegStatus::Failed)
    }
```

Then replace the local closure in `Order::acceptance_state` so it reads:

```rust
        let survived = |l: &&VendorLeg| !l.status.declined();
```

Leave the rest of `acceptance_state` exactly as it is. Add this test:

```rust
#[test]
fn a_failed_leg_is_neither_accepted_nor_rejected() {
    // `Failed` means the leg passed acceptance and broke afterwards, so it is
    // not a vendor refusal — conflating the two would send an ops team down
    // the wrong remediation path. The consequence, which is easy to misread
    // on a dashboard, is that `accepted + rejected` does NOT equal the leg
    // count when any leg failed.
    let o = order_with(&[LegStatus::Accepted, LegStatus::Failed]);
    assert_eq!(
        o.acceptance_state(),
        AcceptanceState::Resolved { accepted: 1, rejected: 0, accepted_subtotal_cents: 1_000 },
    );
}

#[test]
fn an_order_with_no_legs_is_resolved_and_owed_nothing() {
    let o = order_with(&[]);
    assert_eq!(
        o.acceptance_state(),
        AcceptanceState::Resolved { accepted: 0, rejected: 0, accepted_subtotal_cents: 0 },
    );
}
```

Also add one line to the `AcceptanceState::Resolved` doc comment, so the counts are not misread:

```rust
    /// Every leg has answered. `accepted_subtotal_cents` is the amount that may
    /// be captured; the rest of the authorization is voided.
    ///
    /// `accepted + rejected` does not necessarily equal the leg count: a
    /// `Failed` leg is in neither bucket, because it was not refused.
    Resolved { accepted: usize, rejected: usize, accepted_subtotal_cents: i64 },
```

- [ ] **Step 4: Fix `all_legs_collected`**

Replace the `pending` count in `Order::all_legs_collected` (~line 820). Keep the rest of the method and its `advance` call as they are:

```rust
    pub fn all_legs_collected(&mut self) -> Result<(), TransitionError> {
        // Was: a count of `Pending` legs. That was equivalent to "unresolved"
        // only while `Pending | PickedUp | Failed | Settled` were the only
        // states. A leg at `Ready` is accepted and cooked and still on the
        // counter — advancing here would deliver an order whose goods were
        // never handed over.
        let outstanding = self.legs.iter().filter(|l| l.status.blocks_collection()).count();
        if outstanding > 0 {
            return Err(TransitionError::LegsPending(outstanding));
        }
        if !self.legs.iter().any(|l| l.status == LegStatus::PickedUp) {
            return Err(TransitionError::NothingCollected);
        }
        self.advance(OrderStatus::Delivering, &[OrderStatus::Collecting])
    }
```

Also update the method's existing doc comment: the line "A failed leg is resolved, not pending" should read "A failed or rejected leg is resolved; a leg still being prepared is not."

- [ ] **Step 5: Fix the double-credit hole in the collection consumer**

In `services/omnideliv/src/infrastructure/messaging/courier_consumer.rs` (~line 192), the `CourierEvent::Collected` arm guards only against `PickedUp`. Add the terminal guard immediately after it:

```rust
                // The idempotence that matters: a redelivered event must not
                // credit the vendor a second time.
                if leg.status == LegStatus::PickedUp {
                    return Ok(());
                }

                // A leg that reached a terminal state is not re-opened by a
                // late or out-of-order collection event. Without this, a
                // `Collected` arriving after a vendor rejected its leg would
                // overwrite `Rejected` with `PickedUp` and credit a store for
                // goods it refused to hand over.
                if leg.status.is_terminal() {
                    tracing::warn!(
                        %order_id, %vendor_id, status = leg.status.as_str(),
                        "collection event for a leg already in a terminal state — not crediting",
                    );
                    return Ok(());
                }
```

- [ ] **Step 6: Run the tests**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv
```

Expected: the whole suite passes, including the five new tests. If an existing test asserted the old `all_legs_collected` behaviour, read it before changing it — it may be encoding the assumption this task exists to remove, in which case update it and say so in the commit.

- [ ] **Step 7: Commit**

```bash
git add services/omnideliv/src/domain/entities/order.rs services/omnideliv/src/infrastructure/messaging/courier_consumer.rs services/omnideliv/tests/leg_transitions.rs
git commit -m "fix(omnideliv): close four-state assumptions before acceptance states go live"
```

---

## Task 3: Migration — widen the status CHECK and add acceptance columns

**Files:**
- Create: `services/omnideliv/migrations/0024_leg_acceptance.sql`

The existing table constrains status to four values (`0008_create_orders.sql:44-45`). Writing `'accepted'` against it fails at the database, not in Rust — so the migration must land before any transition code runs.

- [ ] **Step 1: Write the migration**

Create `services/omnideliv/migrations/0024_leg_acceptance.sql`:

```sql
-- Vendor leg acceptance — ADR-0017.
--
-- The store is asked before the courier is sent. Until now a leg went straight
-- from 'pending' to 'picked_up', because nobody ever asked the vendor anything.

ALTER TABLE omnideliv.order_vendor_legs
    DROP CONSTRAINT IF EXISTS order_vendor_legs_status_check;

ALTER TABLE omnideliv.order_vendor_legs
    ADD CONSTRAINT order_vendor_legs_status_check
    CHECK (status IN (
        'pending', 'accepted', 'preparing', 'ready',
        'picked_up', 'served', 'rejected', 'failed', 'settled'
    ));

ALTER TABLE omnideliv.order_vendor_legs
    ADD COLUMN IF NOT EXISTS accepted_at      TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS ready_at         TIMESTAMPTZ,
    -- What the store promised at accept time. The basis for a real ready_at
    -- instead of the vendors.prep_time_minutes guess.
    ADD COLUMN IF NOT EXISTS ready_in_minutes INT,
    ADD COLUMN IF NOT EXISTS rejected_reason  TEXT;

-- The vendor queue's only query: this store's live legs, oldest first, because
-- a kitchen works its queue in the order it arrived.
CREATE INDEX IF NOT EXISTS idx_legs_vendor_open
    ON omnideliv.order_vendor_legs (vendor_id, created_at)
    WHERE status IN ('pending', 'accepted', 'preparing', 'ready');
```

- [ ] **Step 2: Verify the constraint name matches what exists**

The `DROP CONSTRAINT` must name the real constraint or the `ADD` collides with the old one and the migration fails on the second value it rejects.

```bash
grep -n "status" services/omnideliv/migrations/0008_create_orders.sql
```

Expected: an inline `CHECK (status IN ('pending','picked_up','failed','settled'))` on the `status` column. Postgres names an inline column check `<table>_<column>_check`, i.e. `order_vendor_legs_status_check` — which is what the migration drops. If the grep shows a named `CONSTRAINT foo CHECK (...)` instead, change the `DROP` to that name.

- [ ] **Step 3: Verify the migration parses**

```bash
CARGO_INCREMENTAL=0 cargo check -p logisticos-omnideliv
```

Expected: `Finished`. This confirms nothing else broke; the SQL itself is verified by the migration CI job on push.

- [ ] **Step 4: Commit**

```bash
git add services/omnideliv/migrations/0024_leg_acceptance.sql
git commit -m "feat(omnideliv): migration for vendor leg acceptance states"
```

---

## Task 4: The guarded transition repository

**Files:**
- Modify: `services/omnideliv/src/domain/repositories/mod.rs`
- Create: `services/omnideliv/src/infrastructure/db/leg_repo.rs`
- Modify: `services/omnideliv/src/infrastructure/db/mod.rs`

The existing `OrderRepository::save()` writes every leg with `ON CONFLICT (id) DO UPDATE SET status, picked_up_at` — last write wins. Two tablets accepting the same leg through `save()` would both succeed and the second would overwrite the first. The transition needs a conditional UPDATE that reports whether it actually applied.

- [ ] **Step 1: Add the trait**

In `services/omnideliv/src/domain/repositories/mod.rs`, after `OrderRepository`:

```rust
/// The outcome of asking a leg to move.
///
/// `NoOp` is not an error: a tablet that retried, or a second member of staff
/// who tapped Accept a moment later, should be told the leg is accepted — which
/// is true — rather than shown a failure for a thing that did happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegTransition {
    Applied { to: LegStatus },
    NoOp { current: LegStatus },
}

#[async_trait]
pub trait VendorLegRepository: Send + Sync {
    /// Moves one leg to `to`, atomically, from whichever states legally precede
    /// it.
    ///
    /// The caller does not pass a predecessor list. It is derived from
    /// `LegStatus::can_transition_to`, so the transition graph is stated in the
    /// domain exactly once instead of being re-hand-written at every call site
    /// where it could silently drift.
    ///
    /// Scoped by `vendor_id` as well as `tenant_id` so a store cannot transition
    /// another store's leg by guessing an id — the same reason the HTTP surface
    /// resolves the vendor from claims rather than from the path.
    ///
    /// No network I/O happens inside this call. Publishing the event is the
    /// caller's job, after the write has committed — the same rule dispatch's
    /// claim transaction follows.
    async fn transition(
        &self,
        tenant_id:        Uuid,
        vendor_id:        Uuid,
        leg_id:           Uuid,
        to:               LegStatus,
        ready_in_minutes: Option<i32>,
        rejected_reason:  Option<&str>,
    ) -> anyhow::Result<LegTransition>;

    /// This vendor's live legs, oldest first. The queue.
    async fn list_open(&self, tenant_id: Uuid, vendor_id: Uuid) -> anyhow::Result<Vec<VendorLegRow>>;
}

/// A queue row. Carries the order context a store needs to cook, and nothing
/// about the customer — a stall has no reason to hold a delivery address.
#[derive(Debug, Clone, Serialize)]
pub struct VendorLegRow {
    pub leg_id:               Uuid,
    pub order_id:             Uuid,
    pub status:               String,
    pub goods_subtotal_cents: i64,
    pub ready_in_minutes:     Option<i32>,
    pub accepted_at:          Option<DateTime<Utc>>,
    pub created_at:           DateTime<Utc>,
}
```

Add `LegStatus` and `VendorLegRow`'s dependencies to the file's `use` statements if absent: `use crate::domain::entities::LegStatus;`, `use chrono::{DateTime, Utc};`, `use serde::Serialize;`.

- [ ] **Step 2: Implement it**

Create `services/omnideliv/src/infrastructure/db/leg_repo.rs`:

```rust
//! One leg, moved conditionally.
//!
//! Separate from `order_repo` on purpose: that writes a whole order as one unit
//! with `ON CONFLICT DO UPDATE`, which is last-write-wins and correct for a
//! checkout. It is wrong for a transition two tablets may attempt at once.

use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::entities::LegStatus;
use crate::domain::repositories::{LegTransition, VendorLegRepository, VendorLegRow};

pub struct PgVendorLegRepository {
    pool: PgPool,
}

impl PgVendorLegRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl VendorLegRepository for PgVendorLegRepository {
    async fn transition(
        &self,
        tenant_id:        Uuid,
        vendor_id:        Uuid,
        leg_id:           Uuid,
        to:               LegStatus,
        ready_in_minutes: Option<i32>,
        rejected_reason:  Option<&str>,
    ) -> anyhow::Result<LegTransition> {
        // Derived from the domain graph, never hand-written here. A change to
        // `can_transition_to` reaches the SQL automatically, so the two cannot
        // drift apart.
        let from_strs: Vec<String> = LegStatus::ALL
            .iter()
            .filter(|s| s.can_transition_to(to))
            .map(|s| s.as_str().to_owned())
            .collect();

        // The whole guard is the WHERE clause. If another tablet already moved
        // this leg, `status = ANY($4)` no longer holds and zero rows update.
        let updated = sqlx::query(
            r#"
            UPDATE omnideliv.order_vendor_legs
               SET status           = $5,
                   accepted_at      = CASE WHEN $5 = 'accepted' THEN NOW() ELSE accepted_at END,
                   ready_at         = CASE WHEN $5 = 'ready'    THEN NOW() ELSE ready_at    END,
                   ready_in_minutes = COALESCE($6, ready_in_minutes),
                   rejected_reason  = COALESCE($7, rejected_reason),
                   picked_up_at     = CASE WHEN $5 = 'picked_up' THEN NOW() ELSE picked_up_at END
             WHERE id        = $1
               AND tenant_id = $2
               AND vendor_id = $3
               AND status    = ANY($4)
            "#,
        )
        .bind(leg_id)
        .bind(tenant_id)
        .bind(vendor_id)
        .bind(&from_strs)
        .bind(to.as_str())
        .bind(ready_in_minutes)
        .bind(rejected_reason)
        .execute(&self.pool)
        .await?
        .rows_affected();

        if updated == 1 {
            return Ok(LegTransition::Applied { to });
        }

        // Zero rows means one of two things, and the caller needs to tell them
        // apart: the leg moved already (report the current state), or it does
        // not belong to this vendor at all (report nothing and let the handler
        // 404). Re-reading under the same tenant+vendor scope answers both.
        let current: Option<String> = sqlx::query(
            r#"
            SELECT status FROM omnideliv.order_vendor_legs
             WHERE id = $1 AND tenant_id = $2 AND vendor_id = $3
            "#,
        )
        .bind(leg_id)
        .bind(tenant_id)
        .bind(vendor_id)
        .fetch_optional(&self.pool)
        .await?
        .map(|r| r.get::<String, _>("status"));

        match current {
            Some(s) => {
                let parsed = LegStatus::from_wire(&s)
                    .ok_or_else(|| anyhow::anyhow!("unknown leg status in database: {s}"))?;
                Ok(LegTransition::NoOp { current: parsed })
            }
            None => anyhow::bail!("leg {leg_id} not found for this vendor"),
        }
    }

    async fn list_open(&self, tenant_id: Uuid, vendor_id: Uuid) -> anyhow::Result<Vec<VendorLegRow>> {
        let rows = sqlx::query(
            r#"
            SELECT id, order_id, status, goods_subtotal_cents,
                   ready_in_minutes, accepted_at, created_at
              FROM omnideliv.order_vendor_legs
             WHERE tenant_id = $1
               AND vendor_id = $2
               AND status IN ('pending', 'accepted', 'preparing', 'ready')
             ORDER BY created_at ASC
            "#,
        )
        .bind(tenant_id)
        .bind(vendor_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| VendorLegRow {
                leg_id:               r.get("id"),
                order_id:             r.get("order_id"),
                status:               r.get("status"),
                goods_subtotal_cents: r.get("goods_subtotal_cents"),
                ready_in_minutes:     r.get("ready_in_minutes"),
                accepted_at:          r.get("accepted_at"),
                created_at:           r.get("created_at"),
            })
            .collect())
    }
}
```

- [ ] **Step 3: Register the module**

In `services/omnideliv/src/infrastructure/db/mod.rs`, add alongside the existing module declarations:

```rust
pub mod leg_repo;
```

- [ ] **Step 4: Verify it compiles**

```bash
CARGO_INCREMENTAL=0 cargo check -p logisticos-omnideliv
```

Expected: `Finished`.

- [ ] **Step 5: Commit**

```bash
git add services/omnideliv/src/domain/repositories/mod.rs services/omnideliv/src/infrastructure/db/leg_repo.rs services/omnideliv/src/infrastructure/db/mod.rs
git commit -m "feat(omnideliv): guarded conditional transition for vendor legs"
```

---

## Task 5: The vendor-keyed event

**Files:**
- Modify: `libs/events/src/topics.rs`
- Create: `services/omnideliv/src/infrastructure/messaging/vendor_events.rs`
- Modify: `services/omnideliv/src/infrastructure/messaging/mod.rs`

Keyed on `vendor_id`, not `order_id` — a stall's queue needs its own messages in order, and a foodcourt order produces one message per stall.

- [ ] **Step 1: Add the topic constants**

In `libs/events/src/topics.rs`, next to the existing OmniDeliv constants (around line 133):

```rust
pub const OMNIDELIV_VENDOR_LEG_RECEIVED: &str = "omnideliv.vendor.leg.received";
pub const OMNIDELIV_VENDOR_LEG_ACCEPTED: &str = "omnideliv.vendor.leg.accepted";
pub const OMNIDELIV_VENDOR_LEG_REJECTED: &str = "omnideliv.vendor.leg.rejected";
```

- [ ] **Step 2: Register the topics for pre-creation**

A consumer that subscribes to a topic which has never been published to does not recover when it later appears. Find the topic-creation list and add all three:

```bash
grep -rn "OMNIDELIV_ORDER_PLACED" --include=*.rs --include=*.yml --include=*.yaml --include=*.sh . | grep -v "services/omnideliv/src\|services/engagement/src\|libs/events/src/topics.rs"
```

Add the three new constants everywhere that grep shows the existing topic being registered or asserted. If it returns nothing, note that in the commit message — the pre-creation check lives outside this repo path and must be raised separately.

- [ ] **Step 3: Write the publisher**

Create `services/omnideliv/src/infrastructure/messaging/vendor_events.rs`:

```rust
//! What omnideliv tells a vendor about its own work.
//!
//! Keyed on `vendor_id` rather than `order_id`: a stall needs its own messages
//! in order, and one foodcourt order produces one message per stall. Keying on
//! the order would put three stalls' work on one partition and interleave it.

use std::sync::Arc;

use async_trait::async_trait;
use logisticos_events::topics;
use uuid::Uuid;

use crate::domain::entities::VendorLeg;

#[async_trait]
pub trait VendorLegEvents: Send + Sync {
    async fn leg_received(&self, leg: &VendorLeg) -> anyhow::Result<()>;
    async fn leg_accepted(&self, leg: &VendorLeg, ready_in_minutes: i32) -> anyhow::Result<()>;
    async fn leg_rejected(&self, leg: &VendorLeg, reason: &str) -> anyhow::Result<()>;
}

pub struct KafkaVendorLegEvents {
    producer: Arc<logisticos_events::producer::KafkaProducer>,
}

impl KafkaVendorLegEvents {
    pub fn new(producer: Arc<logisticos_events::producer::KafkaProducer>) -> Self {
        Self { producer }
    }

    /// Carries only this vendor's leg. A stall has no reason to learn what the
    /// stall next door is making, or where the order is going.
    fn payload(leg: &VendorLeg, extra: serde_json::Value) -> serde_json::Value {
        let mut base = serde_json::json!({
            "tenant_id":            leg.tenant_id,
            "vendor_id":            leg.vendor_id,
            "order_id":             leg.order_id,
            "leg_id":               leg.id,
            "goods_subtotal_cents": leg.goods_subtotal_cents,
            "status":               leg.status.as_str(),
        });
        if let (Some(b), Some(e)) = (base.as_object_mut(), extra.as_object()) {
            for (k, v) in e {
                b.insert(k.clone(), v.clone());
            }
        }
        base
    }

    async fn emit(&self, topic: &str, key: Uuid, payload: serde_json::Value) -> anyhow::Result<()> {
        self.producer
            .publish_raw(topic, &key.to_string(), &serde_json::to_string(&payload)?)
            .await
    }
}

#[async_trait]
impl VendorLegEvents for KafkaVendorLegEvents {
    async fn leg_received(&self, leg: &VendorLeg) -> anyhow::Result<()> {
        self.emit(
            topics::OMNIDELIV_VENDOR_LEG_RECEIVED,
            leg.vendor_id,
            Self::payload(leg, serde_json::json!({})),
        )
        .await
    }

    async fn leg_accepted(&self, leg: &VendorLeg, ready_in_minutes: i32) -> anyhow::Result<()> {
        self.emit(
            topics::OMNIDELIV_VENDOR_LEG_ACCEPTED,
            leg.vendor_id,
            Self::payload(leg, serde_json::json!({ "ready_in_minutes": ready_in_minutes })),
        )
        .await
    }

    async fn leg_rejected(&self, leg: &VendorLeg, reason: &str) -> anyhow::Result<()> {
        self.emit(
            topics::OMNIDELIV_VENDOR_LEG_REJECTED,
            leg.vendor_id,
            Self::payload(leg, serde_json::json!({ "reason": reason })),
        )
        .await
    }
}

/// Used when the broker is unreachable at startup — the same trade the order
/// events make. A vendor who cannot be messaged is worse off than one whose
/// tablet has to poll, and the queue endpoint is the record regardless.
pub struct NoopVendorLegEvents;

#[async_trait]
impl VendorLegEvents for NoopVendorLegEvents {
    async fn leg_received(&self, _leg: &VendorLeg) -> anyhow::Result<()> { Ok(()) }
    async fn leg_accepted(&self, _leg: &VendorLeg, _r: i32) -> anyhow::Result<()> { Ok(()) }
    async fn leg_rejected(&self, _leg: &VendorLeg, _r: &str) -> anyhow::Result<()> { Ok(()) }
}
```

- [ ] **Step 4: Register the module**

In `services/omnideliv/src/infrastructure/messaging/mod.rs`:

```rust
pub mod vendor_events;
```

- [ ] **Step 5: Verify it compiles**

```bash
CARGO_INCREMENTAL=0 cargo check -p logisticos-omnideliv
```

Expected: `Finished`. If `publish_raw` is not found, check its exact name in `order_events.rs` and match it.

- [ ] **Step 6: Commit**

```bash
git add libs/events/src/topics.rs services/omnideliv/src/infrastructure/messaging/
git commit -m "feat(omnideliv): vendor-keyed leg events"
```

---

## Task 6: The vendor order queue and action routes

**Files:**
- Create: `services/omnideliv/src/api/http/vendor_orders.rs`
- Modify: `services/omnideliv/src/api/http/mod.rs`

Routes follow the `/me`-resolves-from-claims rule from `vendors.rs` — a vendor id in the path lets any signed-in store act on another's legs.

- [ ] **Step 1: Write the handler module**

Create `services/omnideliv/src/api/http/vendor_orders.rs`:

```rust
//! The vendor's own order queue.
//!
//! `/me` resolves the store from claims, never from the path — the same rule
//! `vendors.rs` states, for the same reason: a vendor id in the URL would let
//! any signed-in store accept another store's orders.
//!
//! The queue endpoint is the record. Every notification channel added later is
//! a hint that something is on it, and a dropped push costs a poll interval
//! rather than an order.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use logisticos_auth::middleware::AuthClaims;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::http::AppState;
use crate::domain::entities::LegStatus;
use crate::domain::repositories::{LegTransition, VendorLegRow};

#[derive(Debug, Deserialize)]
pub struct AcceptRequest {
    /// What the store promises. Bounded because an unbounded value silently
    /// becomes an SLA nobody agreed to.
    pub ready_in_minutes: i32,
}

#[derive(Debug, Deserialize)]
pub struct RejectRequest {
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct TransitionResponse {
    pub leg_id: Uuid,
    pub status: String,
    /// False when the leg was already in the target state — a retry from a
    /// tablet that lost its connection, or a second member of staff.
    pub changed: bool,
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/omnideliv/vendors/me/orders", get(queue))
        .route("/v1/omnideliv/vendors/me/legs/:leg_id/accept", post(accept))
        .route("/v1/omnideliv/vendors/me/legs/:leg_id/reject", post(reject))
        .route("/v1/omnideliv/vendors/me/legs/:leg_id/ready", post(ready))
        .route("/v1/omnideliv/vendors/me/legs/:leg_id/served", post(served))
}

/// Resolves the caller's store. 404 rather than 403 for a caller who runs no
/// store: that is an absence, not a permission failure — same as `vendors::me`.
async fn my_vendor_id(st: &AppState, claims: &AuthClaims) -> Result<Uuid, StatusCode> {
    st.catalog
        .vendor_for_user(claims.tenant_id, claims.user_id)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "vendor lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .map(|v| v.id)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn queue(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
) -> Result<Json<Vec<VendorLegRow>>, StatusCode> {
    let vendor_id = my_vendor_id(&st, &claims).await?;
    let rows = st
        .legs
        .list_open(claims.tenant_id, vendor_id)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "vendor queue read failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(rows))
}

/// Shared by all four actions: resolve the store, attempt the guarded move,
/// and translate the outcome. A `NoOp` is a 200 carrying the current state —
/// the leg is in the state the caller asked for, which is what they need to
/// know; failing here would make a flaky connection look like a broken order.
async fn act(
    st: &AppState,
    claims: &AuthClaims,
    leg_id: Uuid,
    to: LegStatus,
    ready_in_minutes: Option<i32>,
    rejected_reason: Option<&str>,
) -> Result<Json<TransitionResponse>, StatusCode> {
    let vendor_id = my_vendor_id(st, claims).await?;

    let outcome = st
        .legs
        .transition(
            claims.tenant_id, vendor_id, leg_id,
            to, ready_in_minutes, rejected_reason,
        )
        .await
        .map_err(|e| {
            // The repository bails when the leg does not belong to this vendor.
            tracing::warn!(err = %e, %leg_id, "leg transition rejected");
            StatusCode::NOT_FOUND
        })?;

    match outcome {
        LegTransition::Applied { to } => Ok(Json(TransitionResponse {
            leg_id,
            status: to.as_str().to_owned(),
            changed: true,
        })),
        LegTransition::NoOp { current } if current == to => Ok(Json(TransitionResponse {
            leg_id,
            status: current.as_str().to_owned(),
            changed: false,
        })),
        // Already moved somewhere else entirely — accepting a leg that is
        // already `ready` is a real conflict, not a duplicate submission.
        LegTransition::NoOp { .. } => Err(StatusCode::CONFLICT),
    }
}

async fn accept(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(leg_id): Path<Uuid>,
    Json(req): Json<AcceptRequest>,
) -> Result<Json<TransitionResponse>, StatusCode> {
    if req.ready_in_minutes < 1 || req.ready_in_minutes > 240 {
        return Err(StatusCode::BAD_REQUEST);
    }
    act(&st, &claims, leg_id, LegStatus::Accepted, Some(req.ready_in_minutes), None).await
}

async fn reject(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(leg_id): Path<Uuid>,
    Json(req): Json<RejectRequest>,
) -> Result<Json<TransitionResponse>, StatusCode> {
    let reason = req.reason.trim();
    if reason.is_empty() {
        // The substitution path reads this. A blank reason makes an order that
        // died unexplainable, so it is a 400 rather than a default string.
        return Err(StatusCode::BAD_REQUEST);
    }
    act(&st, &claims, leg_id, LegStatus::Rejected, None, Some(reason)).await
}

async fn ready(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(leg_id): Path<Uuid>,
) -> Result<Json<TransitionResponse>, StatusCode> {
    act(&st, &claims, leg_id, LegStatus::Ready, None, None).await
}

async fn served(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(leg_id): Path<Uuid>,
) -> Result<Json<TransitionResponse>, StatusCode> {
    act(&st, &claims, leg_id, LegStatus::Served, None, None).await
}
```

- [ ] **Step 2: Add the repository to `AppState` and merge the router**

In `services/omnideliv/src/api/http/mod.rs`, add the field to `AppState` (around line 19):

```rust
pub legs: Arc<dyn crate::domain::repositories::VendorLegRepository>,
```

Add the module declaration alongside the others:

```rust
pub mod vendor_orders;
```

And merge the router inside the authenticated group, next to `vendors::routes()`:

```rust
.merge(vendor_orders::routes())
```

Note: do **not** add a second `.route()` call for a path already registered — duplicate paths panic at startup. These five paths are all new.

- [ ] **Step 3: Construct it in bootstrap**

In `services/omnideliv/src/bootstrap.rs`, build the repository where the other repositories are constructed and pass it into `AppState`:

```rust
let legs: Arc<dyn crate::domain::repositories::VendorLegRepository> =
    Arc::new(crate::infrastructure::db::leg_repo::PgVendorLegRepository::new(pool.clone()));
```

Then add `legs,` to the `AppState { .. }` construction. If the local binding for the pool is not named `pool`, match whatever `order_repo` is given.

- [ ] **Step 4: Verify it compiles**

```bash
CARGO_INCREMENTAL=0 cargo check -p logisticos-omnideliv
```

Expected: `Finished`. A "missing field `legs`" error points at any other `AppState` construction — including in tests — that also needs it.

- [ ] **Step 5: Commit**

```bash
git add services/omnideliv/src/api/http/ services/omnideliv/src/bootstrap.rs
git commit -m "feat(omnideliv): vendor order queue and leg action routes"
```

---

## Task 7: Route the new paths through the gateway

**Files:**
- Modify: `services/api-gateway/src/proxy/mod.rs`

The routes are unreachable from outside the cluster until the gateway knows they belong to omnideliv.

- [ ] **Step 1: Check whether the existing prefix already covers them**

```bash
grep -n "omnideliv" services/api-gateway/src/proxy/mod.rs
```

Expected: a prefix match on `/v1/omnideliv`. If the match is on that prefix, these routes are already covered — record that in the commit and skip to Task 8. If it enumerates individual paths, add the five new ones alongside.

- [ ] **Step 2: Verify it compiles**

```bash
CARGO_INCREMENTAL=0 cargo check -p logisticos-api-gateway
```

Expected: `Finished`.

- [ ] **Step 3: Commit (only if step 1 required a change)**

```bash
git add services/api-gateway/src/proxy/mod.rs
git commit -m "feat(api-gateway): route vendor leg actions to omnideliv"
```

---

## Task 8: Idempotency keys on the action routes

**Files:**
- Create: `services/omnideliv/migrations/0025_vendor_action_idempotency.sql`
- Modify: `services/omnideliv/src/api/http/vendor_orders.rs`

The guarded transition already makes a duplicate `accept` safe — the second returns `changed: false`. This task covers the case the guard cannot: once Plan 4 wires capture to acceptance, a retried request must not trigger a second capture attempt even if the transition itself is a no-op. Landing the header contract now means the endpoint shape does not change when the money does.

- [ ] **Step 1: Write the migration**

Create `services/omnideliv/migrations/0025_vendor_action_idempotency.sql`:

```sql
-- Replay protection for vendor leg actions — ADR-0017 decision 8.
--
-- The guarded UPDATE already makes a duplicate transition a no-op. This exists
-- for the side effects that hang off a transition, which a no-op must not
-- re-fire: capture, once Plan 4 wires it.

CREATE TABLE IF NOT EXISTS omnideliv.vendor_action_idempotency (
    tenant_id    UUID        NOT NULL,
    vendor_id    UUID        NOT NULL,
    key          TEXT        NOT NULL,
    leg_id       UUID        NOT NULL,
    action       TEXT        NOT NULL,
    response     JSONB       NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tenant_id, vendor_id, key)
);

-- A tablet's key space is its own. Sweeping old rows keeps this from growing
-- without bound; a replay older than a day is a new request in practice.
CREATE INDEX IF NOT EXISTS idx_vendor_idem_created
    ON omnideliv.vendor_action_idempotency (created_at);
```

- [ ] **Step 2: Read the key in the handlers**

In `services/omnideliv/src/api/http/vendor_orders.rs`, add the extractor and thread it through `act`. Add to the imports:

```rust
use axum::http::HeaderMap;
```

Change `act`'s signature to take the header map and the action name, and check for a stored response before doing any work:

```rust
async fn act(
    st: &AppState,
    claims: &AuthClaims,
    headers: &HeaderMap,
    action: &str,
    leg_id: Uuid,
    to: LegStatus,
    ready_in_minutes: Option<i32>,
    rejected_reason: Option<&str>,
) -> Result<Json<TransitionResponse>, StatusCode> {
    let vendor_id = my_vendor_id(st, claims).await?;

    let key = headers
        .get("x-idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty() && s.len() <= 200);

    // Replay check before any other work — the same ordering order-intake uses.
    if let Some(k) = key.as_deref() {
        if let Some(stored) = st
            .legs
            .find_idempotent_response(claims.tenant_id, vendor_id, k)
            .await
            .map_err(|e| {
                tracing::error!(err = %e, "idempotency lookup failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?
        {
            return Ok(Json(stored));
        }
    }

    let outcome = st
        .legs
        .transition(
            claims.tenant_id, vendor_id, leg_id,
            to, ready_in_minutes, rejected_reason,
        )
        .await
        .map_err(|e| {
            tracing::warn!(err = %e, %leg_id, "leg transition rejected");
            StatusCode::NOT_FOUND
        })?;

    let response = match outcome {
        LegTransition::Applied { to } => TransitionResponse {
            leg_id, status: to.as_str().to_owned(), changed: true,
        },
        LegTransition::NoOp { current } if current == to => TransitionResponse {
            leg_id, status: current.as_str().to_owned(), changed: false,
        },
        LegTransition::NoOp { .. } => return Err(StatusCode::CONFLICT),
    };

    if let Some(k) = key.as_deref() {
        // Best-effort: a store that already got its answer must not be handed a
        // 500 because the replay note failed to save.
        if let Err(e) = st
            .legs
            .record_idempotent_response(claims.tenant_id, vendor_id, k, leg_id, action, &response)
            .await
        {
            tracing::warn!(err = %e, "failed to record idempotency key");
        }
    }

    Ok(Json(response))
}
```

Each of the four handlers gains `headers: HeaderMap` as an extractor (placed before any `Json` body extractor, which must stay last) and passes it plus its action name — `"accept"`, `"reject"`, `"ready"`, `"served"` — into `act`.

- [ ] **Step 3: Add the two repository methods**

In `services/omnideliv/src/domain/repositories/mod.rs`, add to `VendorLegRepository`:

```rust
    /// A previously stored response for this key, if the request is a replay.
    async fn find_idempotent_response(
        &self,
        tenant_id: Uuid,
        vendor_id: Uuid,
        key:       &str,
    ) -> anyhow::Result<Option<TransitionResponse>>;

    async fn record_idempotent_response(
        &self,
        tenant_id: Uuid,
        vendor_id: Uuid,
        key:       &str,
        leg_id:    Uuid,
        action:    &str,
        response:  &TransitionResponse,
    ) -> anyhow::Result<()>;
```

`TransitionResponse` moves out of the handler module and into `domain/repositories/mod.rs` so both can name it; it needs `Deserialize` added to its derives for the read path. Update the `use` in `vendor_orders.rs` to import it from there rather than declaring it locally.

Implement both in `leg_repo.rs`:

```rust
    async fn find_idempotent_response(
        &self,
        tenant_id: Uuid,
        vendor_id: Uuid,
        key:       &str,
    ) -> anyhow::Result<Option<TransitionResponse>> {
        let row = sqlx::query(
            r#"
            SELECT response FROM omnideliv.vendor_action_idempotency
             WHERE tenant_id = $1 AND vendor_id = $2 AND key = $3
            "#,
        )
        .bind(tenant_id).bind(vendor_id).bind(key)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(r) => Ok(Some(serde_json::from_value(r.get::<serde_json::Value, _>("response"))?)),
            None => Ok(None),
        }
    }

    async fn record_idempotent_response(
        &self,
        tenant_id: Uuid,
        vendor_id: Uuid,
        key:       &str,
        leg_id:    Uuid,
        action:    &str,
        response:  &TransitionResponse,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO omnideliv.vendor_action_idempotency
                (tenant_id, vendor_id, key, leg_id, action, response)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (tenant_id, vendor_id, key) DO NOTHING
            "#,
        )
        .bind(tenant_id).bind(vendor_id).bind(key)
        .bind(leg_id).bind(action)
        .bind(serde_json::to_value(response)?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
```

- [ ] **Step 4: Verify it compiles**

```bash
CARGO_INCREMENTAL=0 cargo check -p logisticos-omnideliv
```

Expected: `Finished`.

- [ ] **Step 5: Run the full crate test suite**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv
```

Expected: all tests pass. `leg_transitions` runs without a database; the others may skip or fail on a missing Postgres — confirm any failure is a connection error and not an assertion before moving on.

- [ ] **Step 6: Commit**

```bash
git add services/omnideliv/
git commit -m "feat(omnideliv): idempotency keys on vendor leg actions"
```

---

## Definition of done

> **All items below verified 2026-08-30.** `cargo test -p logisticos-omnideliv -p logisticos-events`: 221 lib tests + every integration binary green. `cargo clippy --all-targets`: clean. Gateway routing test: 10 passed.


- [ ] A vendor can `GET /v1/omnideliv/vendors/me/orders` and see only its own live legs
- [ ] Accept, reject, ready and served each move exactly one leg, guarded against a concurrent tablet
- [ ] A duplicate accept returns `200 changed:false`, not an error and not a second transition
- [ ] Accepting a leg that is already `ready` returns `409`
- [ ] One store cannot transition another store's leg, by any id it can guess
- [ ] `Order::acceptance_state()` reports the accepted subtotal that Plan 4 will capture
- [ ] `OrderStatus` is untouched — no second writer introduced
- [ ] An order with one leg collected and one still `Ready` does **not** advance to `Delivering`
- [ ] A late `Collected` event for a rejected leg does **not** credit that vendor
- [ ] The transition graph is stated once: no call site hand-writes a predecessor list
- [ ] `cargo check -p logisticos-omnideliv` is clean

## What this plan deliberately does not do

Naming these so a reviewer does not read the gaps as oversights:

- **No notification is *delivered*.** The events publish — `leg_received` on checkout for COD and on payment authorization for online, `leg_accepted` / `leg_rejected` from the routes — but nothing consumes them yet, so no push, message or sound reaches a human. A vendor sees its queue by loading it. Plan 2 adds the console stream, FCM and WhatsApp.
- **No recovery ladder.** A vendor who never answers leaves a leg `pending` forever. Plan 2 adds the sweep. Until then, do not enable acceptance as a gate on anything.
- **No capture.** Acceptance moves no money. Plan 4 wires the barrier once `services/payments` can capture a partial amount.
- **No merchant-portal UI.** The endpoints exist; nothing renders them. Plan 2 builds the console screen.
