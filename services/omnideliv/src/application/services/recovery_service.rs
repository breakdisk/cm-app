//! Stuck-order recovery.
//!
//! An order that took payment and never found a courier must not sit silently.
//! A sweep decides what each one needs; the decision is a pure function of the
//! order's state and two timestamps, so the policy is testable without a
//! database and without a clock.

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};

use crate::domain::entities::{telemetry::event_type, OrderStatus, TelemetryEvent};
use crate::domain::repositories::{OrderRepository, TelemetryRepository};

/// How long to keep re-offering before handing it to a human.
const RETRY_WINDOW_MINUTES: i64 = 5;
/// Below this, the offer may simply not have been seen yet.
const GRACE_MINUTES: i64 = 2;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
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

/// What a given order needs, as of `now`.
///
/// `now` is a parameter rather than an internal `Utc::now()` so the boundaries
/// can be tested exactly. A function that reads the clock itself can only be
/// tested by constructing timestamps relative to the real clock, which cannot
/// pin the boundary and goes flaky near it.
pub fn decide(status: OrderStatus, placed_at: DateTime<Utc>, now: DateTime<Utc>) -> Recovery {
    if status != OrderStatus::AwaitingCourier && status != OrderStatus::Placed {
        return Recovery::None;
    }

    let age = now - placed_at;
    if age < Duration::minutes(GRACE_MINUTES) {
        Recovery::Wait
    } else if age < Duration::minutes(RETRY_WINDOW_MINUTES) {
        Recovery::Retry
    } else {
        Recovery::Escalate
    }
}

/// The periodic sweep.
///
/// Deliberately separate from the consumer: a stuck order is defined by an
/// event that never arrived, so nothing event-driven can notice it. Only a
/// timer can.
pub struct RecoveryService {
    orders:    Arc<dyn OrderRepository>,
    telemetry: Arc<dyn TelemetryRepository>,
}

impl RecoveryService {
    pub fn new(orders: Arc<dyn OrderRepository>, telemetry: Arc<dyn TelemetryRepository>) -> Self {
        Self { orders, telemetry }
    }

    /// One pass. Returns how many orders were escalated, so the caller can log
    /// a number that means something rather than "sweep ran".
    pub async fn sweep(&self) -> anyhow::Result<usize> {
        let now = Utc::now();
        let stuck = self.orders.find_awaiting_courier().await?;
        let mut escalated = 0;

        for order in stuck {
            match decide(order.status, order.placed_at, now) {
                Recovery::None | Recovery::Wait => {}

                Recovery::Retry => {
                    // Re-offering needs the dispatch port and the delivery
                    // address, neither of which is on the order today — the
                    // address lives on the basket. Recorded rather than
                    // silently skipped, so the timeline shows the order was
                    // seen and what was not done about it.
                    tracing::warn!(order_id = %order.id, "order awaiting a courier; re-offer not yet wired");
                }

                Recovery::Escalate => {
                    escalated += 1;
                    tracing::error!(
                        order_id = %order.id,
                        grand_total_cents = order.grand_total_cents,
                        "order paid but no courier past the retry window — needs a human",
                    );

                    // The customer's timeline is the record that someone knew.
                    // Appended best-effort: failing the sweep on a telemetry
                    // error would stop every later order being looked at.
                    let e = TelemetryEvent::new(
                        order.tenant_id,
                        order.id,
                        event_type::ORDER_ESCALATED,
                        None,
                        None,
                        serde_json::json!({
                            "reason": "no courier accepted within the retry window",
                            "minutes_waiting": (now - order.placed_at).num_minutes(),
                        }),
                    );
                    if let Err(err) = self.telemetry.append(&e).await {
                        tracing::error!(err = %err, order_id = %order.id, "escalation telemetry failed");
                    }
                }
            }
        }

        Ok(escalated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(mins_ago: i64) -> (DateTime<Utc>, DateTime<Utc>) {
        let now = Utc::now();
        (now - Duration::minutes(mins_ago), now)
    }

    #[test]
    fn a_fresh_order_is_left_alone() {
        let (placed, now) = at(1);
        assert_eq!(decide(OrderStatus::AwaitingCourier, placed, now), Recovery::Wait);
    }

    #[test]
    fn an_order_without_a_courier_is_retried_within_the_window() {
        let (placed, now) = at(3);
        assert_eq!(decide(OrderStatus::AwaitingCourier, placed, now), Recovery::Retry);
    }

    /// Past the window, stop retrying and put it in front of a human. Money is
    /// already committed; silent retry forever is the failure mode that turns
    /// into a support call the customer makes first.
    #[test]
    fn past_the_window_it_escalates() {
        let (placed, now) = at(6);
        assert_eq!(decide(OrderStatus::AwaitingCourier, placed, now), Recovery::Escalate);
    }

    #[test]
    fn an_order_that_found_a_courier_needs_nothing() {
        let (placed, now) = at(30);
        assert_eq!(decide(OrderStatus::Collecting, placed, now), Recovery::None);
        assert_eq!(decide(OrderStatus::Delivered, placed, now), Recovery::None);
        assert_eq!(decide(OrderStatus::Cancelled, placed, now), Recovery::None);
    }

    /// An order still in `Placed` is as stuck as one in `AwaitingCourier` — the
    /// offer may have failed before the status ever advanced, and that order
    /// has taken payment too.
    #[test]
    fn a_placed_order_is_also_swept() {
        let (placed, now) = at(6);
        assert_eq!(decide(OrderStatus::Placed, placed, now), Recovery::Escalate);
    }

    /// The boundaries exactly. Testable only because `now` is a parameter: with
    /// an internal clock these would be racy and would be written as ranges,
    /// which is how an off-by-one in a retry window survives.
    #[test]
    fn the_boundaries_are_exact() {
        let now = Utc::now();
        let s = OrderStatus::AwaitingCourier;

        assert_eq!(decide(s, now - Duration::seconds(119), now), Recovery::Wait);
        assert_eq!(decide(s, now - Duration::minutes(2), now), Recovery::Retry,
                   "the grace window is exclusive at its upper edge");
        assert_eq!(decide(s, now - Duration::seconds(299), now), Recovery::Retry);
        assert_eq!(decide(s, now - Duration::minutes(5), now), Recovery::Escalate,
                   "the retry window is exclusive at its upper edge");
    }

    /// Clock skew between services can put `placed_at` slightly in the future.
    /// That must read as fresh, not wrap into an escalation.
    #[test]
    fn an_order_from_the_future_is_treated_as_fresh() {
        let now = Utc::now();
        let placed = now + Duration::seconds(30);
        assert_eq!(decide(OrderStatus::AwaitingCourier, placed, now), Recovery::Wait);
    }
}
