//! Stuck-order recovery.
//!
//! An order that took payment and never found a courier must not sit silently.
//! A sweep decides what each one needs; the decision is a pure function of the
//! order's state and two timestamps, so the policy is testable without a
//! database and without a clock.

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};

use crate::domain::entities::{telemetry::event_type, OrderStatus, TelemetryEvent};
use crate::application::services::CourierDispatch;
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
    dispatch:  Arc<dyn CourierDispatch>,
}

/// A retry reaches further than the first offer. Widening is the only lever
/// available before giving up: the pay is already fixed on the order, so the
/// alternative to a bigger circle is a human.
const RETRY_RADIUS_KM: f64 = 12.0;

impl RecoveryService {
    pub fn new(
        orders: Arc<dyn OrderRepository>,
        telemetry: Arc<dyn TelemetryRepository>,
        dispatch: Arc<dyn CourierDispatch>,
    ) -> Self {
        Self { orders, telemetry, dispatch }
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
                    // Orders placed before migration 0013 have no destination.
                    // Re-offering to a guessed point would send a courier to the
                    // wrong address, which is worse than waiting for the
                    // escalation a human can resolve from the basket.
                    let (Some(lat), Some(lng)) = (order.delivery_lat, order.delivery_lng) else {
                        tracing::warn!(order_id = %order.id,
                            "stuck order has no delivery point; leaving it to escalate");
                        continue;
                    };

                    // Re-offering is safe to repeat. field-ops keys the credit
                    // on `external_ref` — the order — not the assignment, so a
                    // courier cannot be paid twice for the same job however many
                    // times it is offered. Offers to couriers who already hold
                    // one are refused by the single-live-claim index.
                    match self
                        .dispatch
                        .offer(order.tenant_id, order.id, lat, lng, RETRY_RADIUS_KM,
                               order.courier_trip_cents, order.tip_cents,
                               order.grand_total_cents)
                        .await
                    {
                        Ok(ids) if ids.is_empty() => {
                            tracing::warn!(order_id = %order.id, radius_km = RETRY_RADIUS_KM,
                                "re-offer reached no couriers");
                        }
                        Ok(ids) => {
                            tracing::info!(order_id = %order.id, offered = ids.len(),
                                radius_km = RETRY_RADIUS_KM, "re-offered a stuck order");

                            let e = TelemetryEvent::new(
                                order.tenant_id, order.id, event_type::COURIER_REOFFERED,
                                None, None,
                                serde_json::json!({
                                    "offered_to": ids.len(),
                                    "radius_km":  RETRY_RADIUS_KM,
                                    "minutes_waiting": (now - order.placed_at).num_minutes(),
                                }),
                            );
                            if let Err(err) = self.telemetry.append(&e).await {
                                tracing::error!(err = %err, order_id = %order.id,
                                    "re-offer telemetry failed");
                            }
                        }
                        Err(err) => {
                            // Logged, not fatal: one unreachable dispatch must
                            // not stop the sweep looking at every later order,
                            // and the next pass will try again.
                            tracing::error!(err = %err, order_id = %order.id, "re-offer failed");
                        }
                    }
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

#[cfg(test)]
mod sweep_tests {
    use super::*;
    use uuid::Uuid;
    use crate::application::services::FIRST_OFFER_RADIUS_KM;
    use crate::domain::entities::{Order, TelemetryEvent};
    use std::sync::Mutex;

    #[derive(Default)]
    struct Orders(Vec<Order>);
    #[async_trait::async_trait]
    impl OrderRepository for Orders {
        async fn save(&self, _: &Order) -> anyhow::Result<()> { Ok(()) }
        async fn find_by_id(&self, _: Uuid, _: Uuid) -> anyhow::Result<Option<Order>> { Ok(None) }
        async fn find_awaiting_courier(&self) -> anyhow::Result<Vec<Order>> { Ok(self.0.clone()) }
    }

    #[derive(Default)]
    struct Telemetry(Mutex<Vec<String>>);
    #[async_trait::async_trait]
    impl TelemetryRepository for Telemetry {
        async fn append(&self, e: &TelemetryEvent) -> anyhow::Result<()> {
            self.0.lock().unwrap().push(e.event_type.clone());
            Ok(())
        }
        async fn timeline(&self, _: Uuid, _: Uuid) -> anyhow::Result<Vec<TelemetryEvent>> { Ok(vec![]) }
    }

    #[derive(Default)]
    struct Dispatch {
        calls: Mutex<Vec<(Uuid, f64, f64, f64)>>,
        cod:   Mutex<Vec<i64>>,
        fail:  bool,
    }
    #[async_trait::async_trait]
    impl CourierDispatch for Dispatch {
        async fn offer(&self, _t: Uuid, order_id: Uuid, lat: f64, lng: f64, radius_km: f64,
                       _trip: i64, _tip: i64, cod: i64) -> anyhow::Result<Vec<Uuid>> {
            self.calls.lock().unwrap().push((order_id, lat, lng, radius_km));
            self.cod.lock().unwrap().push(cod);
            if self.fail { anyhow::bail!("field-ops unreachable") }
            Ok(vec![Uuid::new_v4()])
        }
    }

    /// Old enough to be retried, not old enough to escalate.
    fn stuck_order(with_point: bool) -> Order {
        let mut o = Order::place(
            Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(),
            vec![], 4_900, 0, 3_500, 14.5995, 120.9842,
        );
        o.status = OrderStatus::AwaitingCourier;
        o.placed_at = Utc::now() - Duration::minutes(GRACE_MINUTES + 1);
        if !with_point {
            o.delivery_lat = None;
            o.delivery_lng = None;
        }
        o
    }

    async fn sweep_with(order: Order, fail: bool)
        -> (Arc<Dispatch>, Arc<Telemetry>, usize) {
        let dispatch = Arc::new(Dispatch { fail, ..Default::default() });
        let telemetry = Arc::new(Telemetry::default());
        let svc = RecoveryService::new(
            Arc::new(Orders(vec![order])), telemetry.clone(), dispatch.clone(),
        );
        let escalated = svc.sweep().await.unwrap();
        (dispatch, telemetry, escalated)
    }

    #[tokio::test]
    async fn a_stuck_order_is_re_offered_at_a_wider_radius() {
        let o = stuck_order(true);
        let id = o.id;
        let (dispatch, telemetry, escalated) = sweep_with(o, false).await;

        let calls = dispatch.calls.lock().unwrap().clone();
        assert_eq!(calls.len(), 1, "the stuck order must be re-offered");
        assert_eq!(calls[0].0, id);
        assert_eq!((calls[0].1, calls[0].2), (14.5995, 120.9842),
                   "re-offered to the order's own destination");
        assert!(calls[0].3 > FIRST_OFFER_RADIUS_KM,
                "a retry must reach further than the first offer, got {}", calls[0].3);

        assert!(telemetry.0.lock().unwrap().iter().any(|e| e == event_type::COURIER_REOFFERED),
                "the timeline must record that we tried again");
        assert_eq!(dispatch.cod.lock().unwrap()[0], 4_900,
                   "a re-offer must still declare the cash to collect — a courier                     who takes the retry and collects nothing leaves the order unpaid");
        assert_eq!(escalated, 0, "a retried order is not an escalation");
    }

    /// Orders placed before migration 0013 have no destination. Re-offering to
    /// a guessed point would send a courier to the wrong address.
    #[tokio::test]
    async fn an_order_with_no_delivery_point_is_never_re_offered() {
        let (dispatch, _, _) = sweep_with(stuck_order(false), false).await;
        assert!(dispatch.calls.lock().unwrap().is_empty(),
                "no destination means no re-offer, at any radius");
    }

    /// One unreachable dispatch must not stop the sweep — the next pass retries.
    #[tokio::test]
    async fn a_failed_re_offer_does_not_fail_the_sweep() {
        let (dispatch, telemetry, escalated) = sweep_with(stuck_order(true), true).await;

        assert_eq!(dispatch.calls.lock().unwrap().len(), 1, "it was attempted");
        assert_eq!(escalated, 0);
        assert!(!telemetry.0.lock().unwrap().iter().any(|e| e == event_type::COURIER_REOFFERED),
                "a failed re-offer must not be recorded as a successful one");
    }
}
