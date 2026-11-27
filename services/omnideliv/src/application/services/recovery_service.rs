//! Stuck-order recovery.
//!
//! An order that took payment and never found a courier must not sit silently.
//! A sweep decides what each one needs; the decision is a pure function of the
//! order's state and two timestamps, so the policy is testable without a
//! database and without a clock.

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};

use crate::application::services::order_payments::OrderPayments;
use crate::application::services::CourierDispatch;
use crate::domain::entities::{
    telemetry::event_type, Order, OrderStatus, PaymentMethod, PaymentStatus, TelemetryEvent,
};
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
    payments:  Arc<dyn OrderPayments>,
    /// How long an `Online` order may sit `Authorized` with no courier before
    /// its hold is voided and the order cancelled. NI's own docs describe
    /// voids as same-day only, so this must stay well inside a day — tens of
    /// minutes, not hours — but it is a policy knob, not a magic literal:
    /// see `Config::online_no_courier_timeout_mins`.
    online_no_courier_timeout_mins: i64,
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
        payments: Arc<dyn OrderPayments>,
        online_no_courier_timeout_mins: i64,
    ) -> Self {
        Self { orders, telemetry, dispatch, payments, online_no_courier_timeout_mins }
    }

    /// One pass. Returns how many orders were escalated, so the caller can log
    /// a number that means something rather than "sweep ran".
    pub async fn sweep(&self) -> anyhow::Result<usize> {
        let now = Utc::now();
        let stuck = self.orders.find_awaiting_courier().await?;
        let mut escalated = 0;

        for order in stuck {
            // `Online` orders follow an entirely different clock and a
            // different terminal action (void the hold, don't just log) than
            // the COD retry/escalate ladder below — see `handle_online`.
            if order.payment_method == PaymentMethod::Online {
                self.handle_online(order, now).await;
                continue;
            }

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
                               order.grand_total_cents,
                               // A retried order is one nobody took, so it needs
                               // a card more than a fresh one does -- but this
                               // service holds no vendor names. Give what the
                               // order itself knows: how many pickups and how
                               // many are already collected. Omitting the names
                               // is honest; omitting the card entirely would make
                               // the retry the blindest offer on the platform.
                               Some(serde_json::json!({
                                   "v": 1,
                                   "stops": order.legs.len() + 1,
                                   "pickups": order.legs.len(),
                                   "vendors": [],
                                   "verticals": [],
                                   "temperature": [],
                                   "retry": true,
                               })))
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

    /// The no-courier timeout for a prepaid order: if nobody has accepted the
    /// job within `online_no_courier_timeout_mins` of the authorization
    /// landing, release the hold and cancel the order rather than continuing
    /// to wait indefinitely — the customer must never be charged for a
    /// delivery nobody made.
    ///
    /// Deliberately its own ladder, not a fold into `decide`'s COD-oriented
    /// Retry/Escalate states: `placed_at` predates authorization by however
    /// long the customer spent on the hosted checkout page (up to the
    /// intent's own TTL), so `payment_authorized_at` — not `placed_at` — is
    /// the clock this counts from.
    async fn handle_online(&self, order: Order, now: DateTime<Utc>) {
        match order.payment_status {
            // Still on the hosted checkout page, or the `payment.intent.authorized`
            // webhook hasn't landed yet — no courier has been offered, so there
            // is nothing here for a courier-retry ladder to act on.
            // `services/payments`' own sweep resolves an abandoned checkout
            // session (30-minute intent TTL) by publishing `payment.intent.failed`,
            // which `infrastructure::messaging::payment_consumer` turns into a
            // cancellation.
            PaymentStatus::Pending => {}
            // Already resolved by an earlier sweep tick or by a courier
            // accepting in the meantime — nothing to do.
            PaymentStatus::Captured | PaymentStatus::Voided | PaymentStatus::Failed => {}
            PaymentStatus::Authorized => {
                let authorized_at = order.payment_authorized_at.unwrap_or(order.placed_at);
                if now - authorized_at < Duration::minutes(self.online_no_courier_timeout_mins) {
                    return; // still inside the window
                }
                self.void_and_cancel(order).await;
            }
        }
    }

    /// Releases the authorization hold and cancels the order. Logged loudly
    /// and left retryable on failure — a failed void leaves funds still
    /// ring-fenced on the customer's card, which is the money-safety-critical
    /// direction (see `payments::PaymentIntentService::void_intent`'s own doc
    /// comment, which this mirrors): the next sweep tick tries again, because
    /// `payment_status` is only advanced past `Authorized` on success.
    async fn void_and_cancel(&self, mut order: Order) {
        let order_id = order.id;
        let Some(intent_id) = order.payment_intent_id else {
            tracing::error!(order_id = %order_id,
                "order authorized with no payment_intent_id — cannot void");
            return;
        };

        if let Err(e) = self.payments.void(intent_id).await {
            tracing::error!(order_id = %order_id, intent_id = %intent_id, err = %e,
                "void_intent failed — funds may still be ring-fenced on the customer's card; \
                 will retry next sweep tick");
            return;
        }

        if let Err(e) = order.payment_voided() {
            tracing::error!(err = %e, order_id = %order_id,
                "gateway void succeeded but the order's payment_status transition was refused");
        }
        // Infallible from every non-Delivered status, and this order cannot
        // be Delivered — a captured, not merely authorized, order is required
        // to ever reach Collecting in the first place.
        let _ = order.cancel();

        if let Err(e) = self.orders.save(&order).await {
            tracing::error!(order_id = %order_id, err = %e,
                "voided and cancelled in memory but failed to persist — will retry next sweep tick");
            return;
        }

        tracing::warn!(order_id = %order_id, intent_id = %intent_id,
            "no courier accepted an authorized order within the timeout — voided and cancelled");

        let e = TelemetryEvent::new(
            order.tenant_id, order_id, event_type::PAYMENT_VOIDED, None, None,
            serde_json::json!({
                "intent_id": intent_id,
                "minutes_waiting": (Utc::now() - order.payment_authorized_at.unwrap_or(order.placed_at)).num_minutes(),
            }),
        );
        if let Err(err) = self.telemetry.append(&e).await {
            tracing::error!(err = %err, order_id = %order_id, "void telemetry failed");
        }
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
    use crate::application::services::order_payments::AuthorizedIntent;
    use crate::application::services::FIRST_OFFER_RADIUS_KM;
    use crate::domain::entities::{Order, TelemetryEvent};
    use std::sync::Mutex;

    /// The timeout used across this module's online-order tests — a stand-in
    /// for `Config::online_no_courier_timeout_mins`, kept small so the tests
    /// stay fast without a mockable clock.
    const TEST_TIMEOUT_MINS: i64 = 30;

    #[derive(Default)]
    struct Orders(Mutex<Vec<Order>>);
    impl Orders {
        fn seeded(orders: Vec<Order>) -> Self { Self(Mutex::new(orders)) }
    }
    #[async_trait::async_trait]
    impl OrderRepository for Orders {
        async fn save(&self, o: &Order) -> anyhow::Result<()> {
            let mut list = self.0.lock().unwrap();
            if let Some(existing) = list.iter_mut().find(|e| e.id == o.id) {
                *existing = o.clone();
            } else {
                list.push(o.clone());
            }
            Ok(())
        }
        async fn find_by_id(&self, _: Uuid, id: Uuid) -> anyhow::Result<Option<Order>> {
            Ok(self.0.lock().unwrap().iter().find(|o| o.id == id).cloned())
        }
        async fn find_awaiting_courier(&self) -> anyhow::Result<Vec<Order>> {
            Ok(self.0.lock().unwrap().clone())
        }
        async fn list_summaries_for_customer(&self, _: Uuid, _: Uuid, _: i64)
            -> anyhow::Result<Vec<crate::domain::repositories::OrderSummary>> { Ok(vec![]) }
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
        /// What each re-offer told the courier about the job.
        cards: std::sync::Mutex<Vec<Option<serde_json::Value>>>,
    }
    #[async_trait::async_trait]
    impl CourierDispatch for Dispatch {
        #[allow(clippy::too_many_arguments)]
        async fn offer(&self, _t: Uuid, order_id: Uuid, lat: f64, lng: f64, radius_km: f64,
                       _trip: i64, _tip: i64, cod: i64,
                       card: Option<serde_json::Value>) -> anyhow::Result<Vec<Uuid>> {
            self.calls.lock().unwrap().push((order_id, lat, lng, radius_km));
            self.cod.lock().unwrap().push(cod);
            self.cards.lock().unwrap().push(card);
            if self.fail { anyhow::bail!("field-ops unreachable") }
            Ok(vec![Uuid::new_v4()])
        }
    }

    #[derive(Default)]
    struct Payments {
        void_calls: Mutex<Vec<Uuid>>,
    }
    #[async_trait::async_trait]
    impl OrderPayments for Payments {
        async fn authorize(&self, _t: Uuid, _o: Uuid, _a: i64, _c: &str, _r: &str)
            -> anyhow::Result<AuthorizedIntent> {
            unreachable!("the recovery sweep never opens a new authorization")
        }
        async fn capture(&self, _intent_id: Uuid, _amount_cents: Option<i64>) -> anyhow::Result<()> {
            unreachable!("the recovery sweep never captures")
        }
        async fn void(&self, intent_id: Uuid) -> anyhow::Result<()> {
            self.void_calls.lock().unwrap().push(intent_id);
            Ok(())
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

    /// An `Online` order, authorized `mins_since_authorized` minutes ago, with
    /// nobody having accepted the job yet.
    fn authorized_online_order(mins_since_authorized: i64) -> (Order, Uuid) {
        let mut o = Order::place(
            Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(),
            vec![], 4_900, 0, 3_500, 14.5995, 120.9842,
        );
        let prepaid = o.grand_total_cents;
        o = o.with_payment(PaymentMethod::Online, prepaid);
        let intent_id = Uuid::new_v4();
        o.payment_authorized(intent_id).unwrap();
        o.payment_authorized_at = Some(Utc::now() - Duration::minutes(mins_since_authorized));
        o.status = OrderStatus::AwaitingCourier;
        (o, intent_id)
    }

    async fn sweep_with(order: Order, fail: bool)
        -> (Arc<Dispatch>, Arc<Telemetry>, usize) {
        let dispatch = Arc::new(Dispatch { fail, ..Default::default() });
        let telemetry = Arc::new(Telemetry::default());
        let payments = Arc::new(Payments::default());
        let svc = RecoveryService::new(
            Arc::new(Orders::seeded(vec![order])), telemetry.clone(), dispatch.clone(),
            payments, TEST_TIMEOUT_MINS,
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

    /// The test the whole no-courier-timeout feature exists for: an
    /// authorized order nobody accepted, well past the window, must have its
    /// hold voided exactly once and the order cancelled — never charging the
    /// customer for a delivery nobody made.
    #[tokio::test]
    async fn an_online_order_past_the_no_courier_timeout_is_voided_and_cancelled() {
        let (order, intent_id) = authorized_online_order(TEST_TIMEOUT_MINS + 1);
        let order_id = order.id;
        let dispatch = Arc::new(Dispatch::default());
        let telemetry = Arc::new(Telemetry::default());
        let payments = Arc::new(Payments::default());
        let orders = Arc::new(Orders::seeded(vec![order]));

        let svc = RecoveryService::new(
            orders.clone(), telemetry.clone(), dispatch.clone(), payments.clone(), TEST_TIMEOUT_MINS,
        );
        svc.sweep().await.unwrap();

        assert_eq!(payments.void_calls.lock().unwrap().as_slice(), &[intent_id],
                   "the hold must be voided exactly once");
        assert!(dispatch.calls.lock().unwrap().is_empty(),
                "an online order never goes through the COD retry ladder");

        let saved = orders.0.lock().unwrap().iter().find(|o| o.id == order_id).cloned().unwrap();
        assert_eq!(saved.payment_status, PaymentStatus::Voided);
        assert_eq!(saved.status, OrderStatus::Cancelled);
    }

    /// Still inside the window — must not void yet, and must not fall through
    /// to the COD retry ladder either.
    #[tokio::test]
    async fn an_online_order_still_inside_the_window_is_left_alone() {
        let (order, _intent_id) = authorized_online_order(TEST_TIMEOUT_MINS - 5);
        let (dispatch, _telemetry, _escalated) = sweep_with(order, false).await;

        assert!(dispatch.calls.lock().unwrap().is_empty());
    }

    /// A voided order must never be re-voided by a later sweep tick — the
    /// gateway call must fire at most once across the order's whole lifetime.
    #[tokio::test]
    async fn a_second_sweep_tick_does_not_re_void_an_already_voided_order() {
        let (order, intent_id) = authorized_online_order(TEST_TIMEOUT_MINS + 1);
        let order_id = order.id;
        let dispatch = Arc::new(Dispatch::default());
        let telemetry = Arc::new(Telemetry::default());
        let payments = Arc::new(Payments::default());
        let orders = Arc::new(Orders::seeded(vec![order]));
        let svc = RecoveryService::new(
            orders.clone(), telemetry.clone(), dispatch.clone(), payments.clone(), TEST_TIMEOUT_MINS,
        );

        svc.sweep().await.unwrap();
        svc.sweep().await.unwrap();

        assert_eq!(payments.void_calls.lock().unwrap().as_slice(), &[intent_id],
                   "a second tick must not void a second time");
        let saved = orders.0.lock().unwrap().iter().find(|o| o.id == order_id).cloned().unwrap();
        assert_eq!(saved.payment_status, PaymentStatus::Voided);
    }

    /// A `Pending` online order — still on the hosted checkout page, or
    /// waiting for the `payment.intent.authorized` webhook — has never had a
    /// courier offered. The COD retry ladder must not touch it, and there is
    /// nothing to void yet either.
    #[tokio::test]
    async fn a_pending_online_order_is_untouched_by_the_sweep() {
        let mut o = Order::place(
            Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(),
            vec![], 4_900, 0, 3_500, 14.5995, 120.9842,
        );
        let prepaid = o.grand_total_cents;
        o = o.with_payment(PaymentMethod::Online, prepaid);
        // Still `Placed` — checkout never calls `courier_offered` on this
        // branch — and old enough that a COD order in the same state would
        // already be escalating.
        o.placed_at = Utc::now() - Duration::minutes(GRACE_MINUTES + 10);

        let (dispatch, telemetry, escalated) = sweep_with(o, false).await;

        assert!(dispatch.calls.lock().unwrap().is_empty(), "no courier offer has happened yet");
        assert!(!telemetry.0.lock().unwrap().iter().any(|e| e == event_type::ORDER_ESCALATED),
                "a pending online order is not a stuck COD order");
        assert_eq!(escalated, 0);
    }
}
