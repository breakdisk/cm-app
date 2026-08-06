# OmniDeliv Order Lifecycle & Events Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make an order move. Today it is created in `Placed` and stays there forever — nothing marks a leg picked up, nothing sets `delivered_at`, and Screen D polls an endpoint that does not exist.

**Architecture:** Kafka in both directions per ADR-0002 and ADR-0006. `field-ops` publishes courier milestones; `services/omnideliv` consumes them and advances an explicit order state machine, appending to `order_telemetry_logs` at every transition. A paid order that never gets a courier retries, then escalates — rather than sitting silently in a state nobody watches.

---

## Why this plan exists

Tracing the post-checkout path found nothing driving it:

| Should happen | Today |
|---|---|
| Courier accepts → order leaves `Placed` | `Order.status` is set once and never changes |
| Courier collects → leg marked picked up | `VendorLeg::mark_picked_up()` exists; nothing calls it |
| Delivery → `delivered_at`, vendor payout | `delivered_at` is never written; the vendor ledger is never credited |
| Screen D shows progress | `GET /v1/orders/:id/track` does not exist |
| Spec §5.5's 5 published + 3 consumed events | No producer, no consumer |

`AwaitingCourier` is an unreachable enum variant. The order lifecycle is a state machine with no transitions.

---

## Dependencies

**Requires Plans 2 and 5.** Verify:

```bash
CARGO_INCREMENTAL=0 cargo check -p logisticos-field-ops -p logisticos-omnideliv
```

---

## Task 1: The state machine

Transitions first, wiring second — the rules are worth pinning before anything can fire them.

**Files:**
- Modify: `services/omnideliv/src/domain/entities/order.rs`

- [ ] **Step 1: Write the failing test**

```rust
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
        o.cancel();
        assert!(o.courier_offered().is_err());
        assert!(o.delivered().is_err());
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv order::the_lifecycle`
Expected: FAIL to compile — `no method named 'courier_offered'`.

- [ ] **Step 3: Implement**

```rust
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TransitionError {
    #[error("cannot go from {from:?} to {to:?}")]
    Illegal { from: OrderStatus, to: OrderStatus },
    #[error("{0} leg(s) still pending collection")]
    LegsPending(usize),
    #[error("no leg was collected — nothing to deliver")]
    NothingCollected,
}

impl Order {
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

    /// Terminal from any state. Refunds are a separate concern.
    pub fn cancel(&mut self) {
        self.status = OrderStatus::Cancelled;
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv order::`
Expected: PASS — 15 passed (8 from Plan 5 plus 7 new).

- [ ] **Step 5: Commit**

```bash
git add services/omnideliv/src/domain/entities/order.rs
git commit -m "feat(omnideliv): explicit order state machine

Kafka is at-least-once, so repeating the current transition is a no-op rather
than an error; anything else is refused. A failed leg is resolved rather than
pending, so it does not block delivery of what was collected — but a still-
pending leg does, because delivering then would pay a vendor whose goods were
never picked up."
```

---

## Task 2: Publishing courier milestones from field-ops

**Files:**
- Create: `services/field-ops/src/infrastructure/messaging/mod.rs`
- Modify: `services/field-ops/src/application/services/dispatch_service.rs`, `src/bootstrap.rs`

- [ ] **Step 1: Write the producer**

```rust
// services/field-ops/src/infrastructure/messaging/mod.rs
//! Courier milestone events.
//!
//! field-ops publishes what happened to a courier. It does not know what the
//! consuming product does with it — `external_ref` is opaque, which is what
//! keeps this a platform tier rather than a LogisticOS or OmniDeliv service.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const TOPIC_COURIER: &str = "fieldops.courier";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum CourierEvent {
    Assigned  { tenant_id: Uuid, product: String, external_ref: Uuid, courier_id: Uuid, assignment_id: Uuid },
    Collected { tenant_id: Uuid, product: String, external_ref: Uuid, courier_id: Uuid, vendor_id: Uuid,
                device_timestamp: Option<chrono::DateTime<chrono::Utc>> },
    Delivered { tenant_id: Uuid, product: String, external_ref: Uuid, courier_id: Uuid,
                device_timestamp: Option<chrono::DateTime<chrono::Utc>> },
}

impl CourierEvent {
    /// Partition key. Keying by `external_ref` means every event for one job
    /// lands on one partition and therefore arrives in order — without that,
    /// `Delivered` can overtake `Collected` and the state machine refuses it.
    pub fn key(&self) -> Uuid {
        match self {
            CourierEvent::Assigned { external_ref, .. }
            | CourierEvent::Collected { external_ref, .. }
            | CourierEvent::Delivered { external_ref, .. } => *external_ref,
        }
    }
}
```

- [ ] **Step 2: Publish on claim**

In `DispatchService::claim`, after a won claim, publish `CourierEvent::Assigned`. Publish is fire-and-forget with an error log — a courier who won a claim must not have it rolled back because Kafka was briefly unavailable:

```rust
    pub async fn claim(&self, tenant_id: Uuid, assignment_id: Uuid) -> anyhow::Result<bool> {
        match self.assignments.try_claim(tenant_id, assignment_id).await? {
            ClaimOutcome::Lost => Ok(false),
            ClaimOutcome::Won => {
                let a = self.assignments.find_by_id(tenant_id, assignment_id).await?;
                if let Some(a) = a {
                    // Fire-and-forget. The claim is already committed; failing
                    // it because the broker hiccupped would hand the job to
                    // nobody. A missed event is recoverable via reconciliation;
                    // a lost claim is not.
                    if let Err(e) = self.events.publish(TOPIC_COURIER, a.external_ref, &CourierEvent::Assigned {
                        tenant_id, product: a.product.as_str().to_string(),
                        external_ref: a.external_ref, courier_id: a.courier_id, assignment_id: a.id,
                    }).await {
                        tracing::error!(err = %e, assignment_id = %a.id, "courier.assigned publish failed");
                    }
                }
                Ok(true)
            }
        }
    }
```

Add `find_by_id` to `AssignmentRepository`, and `POST /v1/assignments/:id/collected` and `/delivered` routes publishing the other two variants.

- [ ] **Step 3: Verify and commit**

Run: `CARGO_INCREMENTAL=0 cargo check -p logisticos-field-ops`

```bash
git add services/field-ops/
git commit -m "feat(field-ops): publish courier milestones

Keyed by external_ref so every event for one job lands on one partition and
arrives in order — otherwise Delivered can overtake Collected and the
consumer's state machine correctly refuses it. Publishing is fire-and-forget:
a committed claim must not be rolled back because the broker hiccupped."
```

---

## Task 3: Consuming them in OmniDeliv

**Files:**
- Create: `services/omnideliv/src/infrastructure/messaging/courier_consumer.rs`

- [ ] **Step 1: Write the consumer**

```rust
// services/omnideliv/src/infrastructure/messaging/courier_consumer.rs
//! Advances orders from field-ops courier milestones.
//!
//! Every handler is idempotent: Kafka is at-least-once, and the state machine's
//! no-op-on-repeat rule is what makes redelivery safe. Every transition also
//! appends to order_telemetry_logs — the append-only timeline is what a dispute
//! is reconstructed from.

use std::sync::Arc;

use uuid::Uuid;

use crate::domain::entities::{LegStatus, TelemetryEvent};
use crate::infrastructure::db::OrderRepository;

pub struct CourierConsumer {
    orders:    Arc<dyn OrderRepository>,
    telemetry: Arc<dyn TelemetryRepository>,
    ledgers:   Arc<dyn VendorLedgerRepository>,
}

impl CourierConsumer {
    pub async fn on_assigned(
        &self,
        tenant_id: Uuid,
        order_id: Uuid,
        assignment_id: Uuid,
    ) -> anyhow::Result<()> {
        let Some(mut order) = self.orders.find_by_id(tenant_id, order_id).await? else {
            // An event for an order we do not have is not an error worth
            // retrying — most likely another product's job on a shared topic.
            tracing::debug!(%order_id, "courier.assigned for an unknown order, ignoring");
            return Ok(());
        };

        order.courier_claimed(assignment_id)?;
        self.orders.save(&order).await?;

        self.telemetry.append(&TelemetryEvent::new(
            tenant_id, order_id, "courier.claimed", None, None,
            serde_json::json!({ "assignment_id": assignment_id }),
        )).await?;

        Ok(())
    }

    pub async fn on_collected(
        &self,
        tenant_id: Uuid,
        order_id: Uuid,
        vendor_id: Uuid,
        device_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    ) -> anyhow::Result<()> {
        let Some(mut order) = self.orders.find_by_id(tenant_id, order_id).await? else {
            return Ok(());
        };

        let Some(leg) = order.legs.iter_mut().find(|l| l.vendor_id == vendor_id) else {
            tracing::warn!(%order_id, %vendor_id, "collected event for a vendor not on this order");
            return Ok(());
        };

        // Idempotent: a redelivered event finds the leg already picked up.
        if leg.status == LegStatus::Pending {
            leg.mark_picked_up();

            // Credit the vendor at pickup, not at delivery. The vendor has
            // handed over the goods; whether the courier completes the trip is
            // not their risk to carry.
            self.ledgers.credit_leg(
                tenant_id, vendor_id, leg.goods_subtotal_cents, leg.commission_cents,
                order.id, leg.id,
            ).await?;
        }

        // Advance to Delivering once every leg is resolved. The error is
        // expected while legs remain and is not propagated.
        if let Err(e) = order.all_legs_collected() {
            tracing::debug!(%order_id, "not yet ready to deliver: {e}");
        }

        self.orders.save(&order).await?;

        self.telemetry.append(&TelemetryEvent::new(
            tenant_id, order_id, "vendor_leg.picked_up", device_timestamp, None,
            serde_json::json!({ "vendor_id": vendor_id }),
        )).await?;

        Ok(())
    }

    pub async fn on_delivered(
        &self,
        tenant_id: Uuid,
        order_id: Uuid,
        device_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    ) -> anyhow::Result<()> {
        let Some(mut order) = self.orders.find_by_id(tenant_id, order_id).await? else {
            return Ok(());
        };

        order.delivered()?;
        self.orders.save(&order).await?;

        self.telemetry.append(&TelemetryEvent::new(
            tenant_id, order_id, "order.delivered", device_timestamp, None,
            serde_json::json!({}),
        )).await?;

        Ok(())
    }
}
```

> **On `?` after `order.delivered()`.** A genuinely out-of-order `Delivered` returns `TransitionError::Illegal` and the message is not committed, so it redelivers. Because the topic is keyed by `external_ref`, the `Collected` that should have preceded it is on the same partition and will have been processed first on the retry. Propagating the error rather than swallowing it is what makes that recovery happen.

- [ ] **Step 2: Wire the Kafka loop**

Follow the consumer pattern in `services/order-intake/src/infrastructure/messaging/`: subscribe to `fieldops.courier`, deserialise `CourierEvent`, filter on `product == "omnideliv"`, dispatch to the handlers, commit on `Ok`.

- [ ] **Step 3: Commit**

```bash
git add services/omnideliv/src/infrastructure/messaging/
git commit -m "feat(omnideliv): consume courier milestones to advance orders

Vendors are credited at pickup, not at delivery: they have handed over the
goods, and whether the courier completes the trip is not their risk. An
out-of-order Delivered is propagated rather than swallowed so the message
redelivers after its Collected — same partition, so ordering is recoverable."
```

---

## Task 4: Publishing OmniDeliv's own events

**Files:**
- Create: `services/omnideliv/src/domain/events.rs`

- [ ] **Step 1: Write the events**

```rust
// services/omnideliv/src/domain/events.rs
//! The five events spec §5.5 says OmniDeliv publishes.
//!
//! Downstream consumers — engagement for notifications, analytics for BI — are
//! not built by this plan. Publishing regardless is deliberate: an event nobody
//! consumes yet costs a topic, while a missing event costs a backfill.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const TOPIC_ORDERS:  &str = "omnideliv.orders";
pub const TOPIC_BASKETS: &str = "omnideliv.baskets";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum OmniDelivEvent {
    BasketProposed   { tenant_id: Uuid, basket_id: Uuid, customer_id: Uuid, needs_review: usize },
    OrderPlaced      { tenant_id: Uuid, order_id: Uuid, customer_id: Uuid, grand_total_cents: i64, stops: usize },
    VendorLegPickedUp{ tenant_id: Uuid, order_id: Uuid, vendor_id: Uuid, payout_cents: i64 },
    OrderDelivered   { tenant_id: Uuid, order_id: Uuid, customer_id: Uuid },
    VendorPayoutAccrued { tenant_id: Uuid, vendor_id: Uuid, period: String, amount_cents: i64 },
}
```

Publish `OrderPlaced` from the checkout route after the order persists, and the rest from the consumer alongside each transition.

- [ ] **Step 2: Commit**

```bash
git add services/omnideliv/src/domain/events.rs
git commit -m "feat(omnideliv): publish the five order lifecycle events

No consumer is built yet. An event nobody reads costs a topic; a missing event
costs a backfill."
```

---

## Task 5: The tracking endpoint

**Files:**
- Create: `services/omnideliv/src/api/http/track.rs`

- [ ] **Step 1: Write the route**

Shape matches what Screen D already renders.

```rust
// services/omnideliv/src/api/http/track.rs
use std::sync::Arc;

use axum::{extract::{Path, State}, http::StatusCode, routing::get, Json, Router};
use serde::Serialize;
use uuid::Uuid;

use logisticos_auth::claims::Claims;

use crate::api::http::AppState;
use crate::domain::entities::{LegStatus, Order, OrderStatus};

#[derive(Debug, Serialize)]
pub struct TimelineStep { pub label: String, pub detail: String, pub state: &'static str }

#[derive(Debug, Serialize)]
pub struct OrderTrack {
    pub eta_minutes: i32,
    pub on_time: bool,
    pub steps: Vec<TimelineStep>,
    pub courier: Option<CourierSummary>,
}

#[derive(Debug, Serialize)]
pub struct CourierSummary { pub name: String, pub vehicle: String }

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/v1/orders/:id/track", get(track))
}

async fn track(
    State(st): State<Arc<AppState>>,
    claims: Claims,
    Path(id): Path<Uuid>,
) -> Result<Json<OrderTrack>, StatusCode> {
    let order = st.orders
        .find_by_id(claims.tenant_id, id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(OrderTrack {
        eta_minutes: eta_minutes(&order),
        // Honest default: without a promised-by time there is nothing to be
        // late against. Populated once the tariff/SLA work lands.
        on_time: true,
        steps: build_steps(&order),
        courier: None,
    }))
}

/// One step per vendor pickup, then the drop. Built from leg state rather than
/// from `status` alone so a customer with two pickups sees both.
fn build_steps(o: &Order) -> Vec<TimelineStep> {
    let mut steps: Vec<TimelineStep> = o.legs.iter().map(|l| TimelineStep {
        label: "Collecting your items".into(),
        detail: match l.status {
            LegStatus::PickedUp => "Collected".into(),
            LegStatus::Failed   => "Couldn't collect — you won't be charged for this".into(),
            _ => "Waiting to collect".into(),
        },
        state: match l.status {
            LegStatus::PickedUp | LegStatus::Settled => "done",
            LegStatus::Failed => "done",
            LegStatus::Pending if o.status == OrderStatus::Collecting => "current",
            _ => "pending",
        },
    }).collect();

    steps.push(TimelineStep {
        label: "Arriving at you".into(),
        detail: if o.status == OrderStatus::Delivered { "Delivered".into() } else { "On the way".into() },
        state: match o.status {
            OrderStatus::Delivered  => "done",
            OrderStatus::Delivering => "current",
            _ => "pending",
        },
    });

    steps
}

/// Coarse estimate from order state. Deliberately not a promise — a real ETA
/// needs live courier position and the routing engine, and showing a confident
/// wrong number is worse than showing a rough right one.
fn eta_minutes(o: &Order) -> i32 {
    match o.status {
        OrderStatus::Placed | OrderStatus::AwaitingCourier => 40,
        OrderStatus::Collecting => 25,
        OrderStatus::Delivering => 12,
        _ => 0,
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add services/omnideliv/src/api/http/track.rs
git commit -m "feat(omnideliv): order tracking endpoint for Screen D

Steps are built from leg state rather than order status alone, so a customer
with two pickups sees both. The ETA is coarse and deliberately not a promise —
a confident wrong number is worse than a rough right one."
```

---

## Task 6: Stuck-order recovery

Spec §7: paid but no courier → `AwaitingCourier`, auto-retry for 5 minutes, then ops escalation. Money is already committed, so this path cannot be left to silence.

**Files:**
- Create: `services/omnideliv/src/application/services/recovery_service.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn placed_at(mins_ago: i64) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc::now() - Duration::minutes(mins_ago)
    }

    #[test]
    fn a_fresh_order_is_left_alone() {
        assert_eq!(decide(OrderStatus::AwaitingCourier, placed_at(1)), Recovery::Wait);
    }

    #[test]
    fn an_order_without_a_courier_is_retried_within_the_window() {
        assert_eq!(decide(OrderStatus::AwaitingCourier, placed_at(3)), Recovery::Retry);
    }

    /// Past the window, stop retrying and put it in front of a human. Money is
    /// already committed; silent retry forever is the failure mode that turns
    /// into a support call the customer makes first.
    #[test]
    fn past_the_window_it_escalates() {
        assert_eq!(decide(OrderStatus::AwaitingCourier, placed_at(6)), Recovery::Escalate);
    }

    #[test]
    fn an_order_that_found_a_courier_needs_nothing() {
        assert_eq!(decide(OrderStatus::Collecting, placed_at(30)), Recovery::None);
        assert_eq!(decide(OrderStatus::Delivered, placed_at(90)), Recovery::None);
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv recovery`
Expected: FAIL to compile — `cannot find function 'decide'`.

- [ ] **Step 3: Implement**

```rust
//! Stuck-order recovery.
//!
//! An order that took payment and never found a courier must not sit silently.
//! A sweep decides what each one needs; the decision is a pure function so the
//! policy is testable without a database or a clock.

use chrono::{DateTime, Duration, Utc};

use crate::domain::entities::OrderStatus;

/// How long to keep re-offering before handing it to a human.
const RETRY_WINDOW_MINUTES: i64 = 5;
/// Below this, the offer may simply not have been seen yet.
const GRACE_MINUTES: i64 = 2;

#[derive(Debug, PartialEq, Eq)]
pub enum Recovery {
    /// Not stuck.
    None,
    /// Stuck but still fresh — give the offer time to land.
    Wait,
    /// Re-offer to a wider radius.
    Retry,
    /// Out of time. Alert ops and tell the customer.
    Escalate,
}

pub fn decide(status: OrderStatus, placed_at: DateTime<Utc>) -> Recovery {
    if status != OrderStatus::AwaitingCourier && status != OrderStatus::Placed {
        return Recovery::None;
    }

    let age = Utc::now() - placed_at;
    if age < Duration::minutes(GRACE_MINUTES) {
        Recovery::Wait
    } else if age < Duration::minutes(RETRY_WINDOW_MINUTES) {
        Recovery::Retry
    } else {
        Recovery::Escalate
    }
}
```

The sweep itself runs on a `tokio::interval`, loads orders in `Placed`/`AwaitingCourier`, applies `decide`, and on `Escalate` publishes an ops alert and appends `order.escalated` telemetry.

- [ ] **Step 4: Run the tests and commit**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv`
Expected: PASS.

```bash
git add services/omnideliv/src/application/services/recovery_service.rs
git commit -m "feat(omnideliv): stuck-order recovery sweep

The decision is a pure function so the policy is testable without a database
or a clock. Past the retry window it escalates rather than retrying forever —
money is already committed, and silent retry is the failure mode the customer
notices before we do."
```

---

## Definition of done

- [ ] `cargo test -p logisticos-omnideliv` — 15 order tests plus the recovery tests pass
- [ ] `cargo check --workspace` — clean
- [ ] A claim in field-ops moves the matching order to `Collecting` within one consumer poll
- [ ] `GET /v1/orders/:id/track` returns one step per leg plus the drop
- [ ] `rg -n "AwaitingCourier" services/omnideliv/src --type rust` shows it both set and read — no longer an unreachable variant

## Still open after this plan

- **No consumer for OmniDeliv's own events.** Notifications need the engagement integration; that is its own plan.
- **Refunding a failed leg.** The leg is marked and the customer is told in the timeline, but no money moves back — it needs the payment capture that Plan 5 also defers.
- **ETA is coarse.** Live courier position and the routing engine would make it real.
