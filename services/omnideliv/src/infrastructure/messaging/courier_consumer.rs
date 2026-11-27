//! Consumes field-ops courier milestones and advances the order.
//!
//! This is where a placed order stops being paper: a collection credits the
//! vendor's ledger, a delivery credits the courier's, and every transition
//! appends to the order timeline.

use std::sync::Arc;

use serde::Deserialize;
use uuid::Uuid;

use crate::application::services::order_payments::OrderPayments;
use crate::domain::entities::order::TransitionError;
use crate::domain::entities::telemetry::event_type;
use crate::domain::entities::{
    LegStatus, Order, OrderStatus, PaymentMethod, PaymentStatus, TelemetryEvent, VendorLedger,
};
use crate::domain::repositories::{OrderRepository, TelemetryRepository, VendorLedgerRepository};

/// The state change for a `Delivered` milestone, separated from the I/O around
/// it so the idempotence rule is testable without a broker or a database.
///
/// `Ok(false)` means the order was already delivered — a duplicate to ignore,
/// not a failure. The `Collected` branch has early-returned on an
/// already-picked-up leg since it was written; this is the same rule, and its
/// absence here is what turned an ordinary retry into a failed message.
///
/// A duplicate is the normal case, not an exotic one: field-ops publishes on
/// every accepted delivery, and the driver app's outbound queue is
/// at-least-once by construction.
fn apply_delivered(order: &mut Order) -> Result<bool, TransitionError> {
    if order.status == OrderStatus::Delivered {
        return Ok(false);
    }
    order.delivered()?;
    Ok(true)
}

pub const TOPIC_COURIER: &str = "fieldops.courier";

/// Mirrors field-ops' `CourierEvent`. Two services, one wire contract — kept in
/// sync by hand, with the tag names asserted on both sides.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum CourierEvent {
    Assigned  { tenant_id: Uuid, product: String, external_ref: Uuid, courier_id: Uuid, assignment_id: Uuid,
                /// Mirrors field-ops exactly, `serde(default)` included. Without
                /// the default, an `Assigned` published before that field
                /// existed — and they are still inside the retention window —
                /// fails to deserialize, and a failed deserialize takes down
                /// every message on the partition, not just that one.
                #[serde(default)] courier_user_id: Option<Uuid> },
    /// Mirrors field-ops. `stop_ref` is the vendor id for a pickup and the
    /// order id for the dropoff — this service is the only one that knows the
    /// difference, which is why field-ops can stay product-agnostic.
    Arrived   { tenant_id: Uuid, product: String, external_ref: Uuid, courier_id: Uuid,
                stop_ref: Uuid,
                device_timestamp: Option<chrono::DateTime<chrono::Utc>> },
    Collected { tenant_id: Uuid, product: String, external_ref: Uuid, courier_id: Uuid, vendor_id: Uuid,
                device_timestamp: Option<chrono::DateTime<chrono::Utc>> },
    Delivered { tenant_id: Uuid, product: String, external_ref: Uuid, courier_id: Uuid,
                device_timestamp: Option<chrono::DateTime<chrono::Utc>> },
}

impl CourierEvent {
    fn product(&self) -> &str {
        match self {
            CourierEvent::Assigned { product, .. }
            | CourierEvent::Arrived { product, .. }
            | CourierEvent::Collected { product, .. }
            | CourierEvent::Delivered { product, .. } => product,
        }
    }

    fn tenant_id(&self) -> Uuid {
        match self {
            CourierEvent::Assigned { tenant_id, .. }
            | CourierEvent::Arrived { tenant_id, .. }
            | CourierEvent::Collected { tenant_id, .. }
            | CourierEvent::Delivered { tenant_id, .. } => *tenant_id,
        }
    }

    /// The order id. field-ops treats this as opaque; here it is the key.
    fn order_id(&self) -> Uuid {
        match self {
            CourierEvent::Assigned { external_ref, .. }
            | CourierEvent::Arrived { external_ref, .. }
            | CourierEvent::Collected { external_ref, .. }
            | CourierEvent::Delivered { external_ref, .. } => *external_ref,
        }
    }
}

pub struct CourierMilestoneHandler {
    orders:    Arc<dyn OrderRepository>,
    ledgers:   Arc<dyn VendorLedgerRepository>,
    telemetry: Arc<dyn TelemetryRepository>,
    events:    Arc<dyn crate::infrastructure::messaging::OrderEvents>,
    /// Captures a prepaid order's authorization hold the moment a courier
    /// actually accepts the job — see the `Assigned` arm in `handle`.
    payments:  Arc<dyn OrderPayments>,
}

impl CourierMilestoneHandler {
    pub fn new(
        orders: Arc<dyn OrderRepository>,
        ledgers: Arc<dyn VendorLedgerRepository>,
        telemetry: Arc<dyn TelemetryRepository>,
        events: Arc<dyn crate::infrastructure::messaging::OrderEvents>,
        payments: Arc<dyn OrderPayments>,
    ) -> Self {
        Self { orders, ledgers, telemetry, events, payments }
    }

    /// Handle one milestone.
    ///
    /// Every path is idempotent, because Kafka is at-least-once: the state
    /// machine treats a repeat of the current transition as a no-op, and a leg
    /// already marked picked up is not credited twice.
    pub async fn handle(&self, event: CourierEvent) -> anyhow::Result<()> {
        // field-ops publishes for every product on one topic. Anything not ours
        // is another product's business and is skipped without complaint.
        if event.product() != "omnideliv" {
            return Ok(());
        }

        let tenant_id = event.tenant_id();
        let order_id = event.order_id();

        let Some(mut order) = self.orders.find_by_id(tenant_id, order_id).await? else {
            // Not an error: field-ops may be replaying an old partition, or the
            // order was cancelled and purged. Logged so a systematic mismatch
            // is visible rather than silent.
            tracing::warn!(%order_id, "courier milestone for an unknown order");
            return Ok(());
        };

        match event {
            CourierEvent::Assigned { assignment_id, courier_user_id, .. } => {
                order.courier_claimed(assignment_id, courier_user_id)?;

                // The "capture" half of authorize-then-capture-or-void: a
                // courier accepting the job is exactly the signal that turns
                // a ring-fenced hold into money actually taken. Only ever
                // runs for an `Online` order still `Authorized` — a `Cod`
                // order's `payment_status` never leaves `Pending`, so this is
                // a no-op for every order today behaves identically for.
                //
                // Deliberately BEFORE `self.orders.save` below, mirroring the
                // `Collected` arm's credit-before-advance rule: if the
                // gateway call fails, bail out of `handle` before saving so
                // this whole `Assigned` event is redelivered and retried,
                // rather than persisting a `Collecting` order whose money is
                // still merely ring-fenced.
                if order.payment_method == PaymentMethod::Online
                    && order.payment_status == PaymentStatus::Authorized
                {
                    let intent_id = order.payment_intent_id.ok_or_else(|| {
                        anyhow::anyhow!(
                            "order {order_id} is authorized with no payment_intent_id — cannot capture"
                        )
                    })?;
                    // The whole basket. A courier accepting the job commits every leg;
                    // the partial path is the acceptance barrier, not this one.
                    self.payments.capture(intent_id, None).await?;
                    order.payment_captured()?;
                    self.append(tenant_id, order_id, event_type::PAYMENT_CAPTURED, None, None,
                                serde_json::json!({ "intent_id": intent_id })).await;
                }

                self.append(tenant_id, order_id, event_type::COURIER_CLAIMED, None, None,
                            serde_json::json!({ "assignment_id": assignment_id })).await;
            }

            CourierEvent::Arrived { stop_ref, courier_id, device_timestamp, .. } => {
                // No status change. Arrival is progress a tracking screen shows
                // well and a lifecycle transition would show badly — a courier
                // parked outside is not a collection. Recorded so the timeline
                // can render it and so SLA maths has the device clock.
                self.append(tenant_id, order_id, event_type::COURIER_ARRIVED,
                            device_timestamp, Some(courier_id),
                            serde_json::json!({ "stop_ref": stop_ref })).await;
            }

            CourierEvent::Collected { vendor_id, courier_id, device_timestamp, .. } => {
                let Some(leg) = order.legs.iter_mut().find(|l| l.vendor_id == vendor_id) else {
                    tracing::warn!(%order_id, %vendor_id, "collection for a vendor not on this order");
                    return Ok(());
                };

                // Routine: a redelivered event must not credit the vendor twice.
                if leg.status == LegStatus::PickedUp {
                    return Ok(());
                }

                // Anything that is not still awaiting the courier has no
                // collection to record: it was refused, it broke, it was
                // served at a table, or it is already settled. Crediting any
                // of those pays for goods that never passed to a courier.
                if !leg.status.blocks_collection() {
                    tracing::warn!(
                        %order_id, %vendor_id, status = leg.status.as_str(),
                        "collection event for a leg not awaiting collection — not crediting",
                    );
                    return Ok(());
                }

                leg.mark_picked_up();
                let (goods, commission, leg_id) =
                    (leg.goods_subtotal_cents, leg.commission_cents, leg.id);

                // Credit before advancing the order: if the ledger write fails,
                // the order stays in Collecting and the event will be retried.
                // Advancing first would leave a delivered order whose vendor was
                // never paid, which no later replay would fix.
                self.credit_vendor(tenant_id, vendor_id, goods, commission, order_id, leg_id).await?;

                self.append(tenant_id, order_id, event_type::LEG_PICKED_UP,
                            device_timestamp, Some(courier_id),
                            serde_json::json!({ "vendor_id": vendor_id, "leg_id": leg_id })).await;

                // Only advances once every leg is resolved; still-pending legs
                // leave the order in Collecting, which is correct.
                let _ = order.all_legs_collected();
            }

            CourierEvent::Delivered { courier_id, device_timestamp, .. } => {
                // A duplicate is a retry, not a failure — see `apply_delivered`.
                // Returning early also skips the publish below, so a customer
                // cannot be told twice that their order arrived.
                if !apply_delivered(&mut order)? {
                    return Ok(());
                }

                self.append(tenant_id, order_id, event_type::ORDER_DELIVERED,
                            device_timestamp, Some(courier_id), serde_json::json!({})).await;

                // After the state change, before the save below. A publish
                // failure must not stop the order being recorded as delivered:
                // the courier is already paid and the customer already has their
                // food, so losing the notification is the smaller loss.
                if let Err(e) = self.events.order_delivered(&order).await {
                    tracing::error!(err = %e, %order_id, "order.delivered publish failed");
                }
            }
        }

        self.orders.save(&order).await?;
        Ok(())
    }

    async fn credit_vendor(
        &self,
        tenant_id: Uuid,
        vendor_id: Uuid,
        goods_cents: i64,
        commission_cents: i64,
        order_id: Uuid,
        leg_id: Uuid,
    ) -> anyhow::Result<()> {
        let period = current_period();
        let mut ledger = match self.ledgers.find_open(tenant_id, vendor_id, &period).await? {
            Some(l) => l,
            None => VendorLedger::open(tenant_id, vendor_id, period),
        };

        ledger.credit_leg(goods_cents, commission_cents, order_id, leg_id);
        self.ledgers.save(&ledger).await
    }

    /// Telemetry is best-effort: losing a timeline entry is bad, but refusing a
    /// collection because the timeline write failed would strand the order and
    /// leave the vendor credited with no matching state.
    async fn append(
        &self,
        tenant_id: Uuid,
        order_id: Uuid,
        event_type: &str,
        device_timestamp: Option<chrono::DateTime<chrono::Utc>>,
        actor_id: Option<Uuid>,
        payload: serde_json::Value,
    ) {
        let e = TelemetryEvent::new(tenant_id, order_id, event_type, device_timestamp, actor_id, payload);
        if let Err(err) = self.telemetry.append(&e).await {
            tracing::error!(err = %err, %order_id, event_type, "telemetry append failed");
        }
    }
}

/// ISO week, e.g. `2026-W32`. Payout periods are weekly.
use crate::domain::entities::current_period;

#[cfg(test)]
mod tests {
    use super::*;

    /// The wire contract with field-ops. If either side renames a variant the
    /// other stops seeing milestones entirely, and the failure is silent — the
    /// consumer simply never matches.
    #[test]
    fn the_wire_tags_match_field_ops() {
        let raw = serde_json::json!({
            "event": "collected",
            "tenant_id": Uuid::nil(), "product": "omnideliv", "external_ref": Uuid::nil(),
            "courier_id": Uuid::nil(), "vendor_id": Uuid::nil(), "device_timestamp": null
        });
        let e: CourierEvent = serde_json::from_value(raw).expect("must parse");
        assert!(matches!(e, CourierEvent::Collected { .. }));
    }

    #[test]
    fn another_products_events_are_not_ours() {
        let raw = serde_json::json!({
            "event": "delivered",
            "tenant_id": Uuid::nil(), "product": "logistics", "external_ref": Uuid::nil(),
            "courier_id": Uuid::nil(), "device_timestamp": null
        });
        let e: CourierEvent = serde_json::from_value(raw).expect("must parse");
        assert_eq!(e.product(), "logistics");
    }

    use crate::domain::entities::VendorLeg;

    /// An order carried to Delivered entirely by legitimate transitions, so the
    /// duplicate under test is the only irregular thing about it.
    fn delivered_order() -> Order {
        const TENANT: Uuid = Uuid::from_u128(1);

        let leg = VendorLeg::settle(TENANT, Uuid::new_v4(), 10_000, 1_500);
        let mut o = Order::place(
            TENANT,
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            vec![leg],
            4_900,
            0,
            3_500,
            14.5547,
            121.0244,
        );

        o.courier_claimed(Uuid::new_v4(), None).unwrap();
        o.legs[0].mark_picked_up();
        o.all_legs_collected().unwrap();
        o.delivered().unwrap();
        o
    }

    /// The sibling branch, `Collected`, has had this since it was written. A
    /// courier's offline queue retrying a delivery whose response was lost
    /// republishes `Delivered`, and a consumer that errors on it turns an
    /// ordinary retry into a failed message.
    #[test]
    fn a_second_delivered_on_a_delivered_order_is_ignored_not_an_error() {
        let mut order = delivered_order();
        assert_eq!(order.status, OrderStatus::Delivered);

        let outcome = apply_delivered(&mut order);

        assert!(outcome.is_ok(), "a duplicate Delivered must not error");
        assert!(!outcome.unwrap(), "and must report that it changed nothing");
        assert_eq!(order.status, OrderStatus::Delivered);
    }

    /// The first one still has to work — an idempotence guard that swallows
    /// the real transition would pass the test above and break delivery.
    #[test]
    fn the_first_delivered_advances_the_order() {
        let mut order = delivered_order();
        // Rewind to the state field-ops' first Delivered actually arrives in.
        order.status = OrderStatus::Delivering;
        order.delivered_at = None;

        assert!(apply_delivered(&mut order).unwrap(), "the first one changes state");
        assert_eq!(order.status, OrderStatus::Delivered);
        assert!(order.delivered_at.is_some());
    }

    /// Backward compatibility, and it is load-bearing. Messages published
    /// before `courier_user_id` existed are still inside the retention window;
    /// without `serde(default)` they fail to deserialize, and a failed
    /// deserialize takes down every message on that partition, not just the
    /// old one.
    #[test]
    fn an_assigned_event_without_a_courier_user_still_parses() {
        let raw = serde_json::json!({
            "event": "assigned",
            "tenant_id": Uuid::nil(), "product": "omnideliv",
            "external_ref": Uuid::nil(), "courier_id": Uuid::nil(),
            "assignment_id": Uuid::nil()
        });
        let parsed: CourierEvent = serde_json::from_value(raw).expect("must parse without the field");
        match parsed {
            CourierEvent::Assigned { courier_user_id, .. } => assert!(courier_user_id.is_none()),
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn an_assigned_event_carrying_a_courier_user_reads_it() {
        let user = Uuid::from_u128(77);
        let raw = serde_json::json!({
            "event": "assigned",
            "tenant_id": Uuid::nil(), "product": "omnideliv",
            "external_ref": Uuid::nil(), "courier_id": Uuid::nil(),
            "assignment_id": Uuid::nil(), "courier_user_id": user
        });
        let parsed: CourierEvent = serde_json::from_value(raw).expect("must parse");
        match parsed {
            CourierEvent::Assigned { courier_user_id, .. } => {
                assert_eq!(courier_user_id, Some(user));
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    /// Kafka is at-least-once, so a replayed pre-migration `Assigned` must not
    /// erase an identity a later message already established.
    #[test]
    fn a_replayed_event_without_a_courier_does_not_erase_a_known_one() {
        let mut o = delivered_order();
        o.status = OrderStatus::Placed;
        let user = Uuid::from_u128(9);

        o.courier_claimed(Uuid::new_v4(), Some(user)).unwrap();
        assert_eq!(o.courier_user_id, Some(user));

        // The replay: same transition, no user id.
        o.courier_claimed(Uuid::new_v4(), None).unwrap();
        assert_eq!(o.courier_user_id, Some(user), "a replay must not blank the courier");
    }

    #[test]
    fn the_period_is_an_iso_week() {
        let p = current_period();
        assert!(p.contains("-W"), "got {p}");
        assert_eq!(p.len(), 8, "YYYY-Wnn, got {p}");
    }
}

#[cfg(test)]
mod capture_on_acceptance {
    use super::*;
    use std::sync::Mutex;

    use crate::domain::entities::{PaymentMethod, PaymentStatus, VendorLeg, VendorLedger};
    use crate::domain::repositories::LedgerPeriod;

    const TENANT: Uuid = Uuid::from_u128(1);

    #[derive(Default)]
    struct Orders(Mutex<Vec<Order>>);
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
        async fn find_by_id(&self, _t: Uuid, id: Uuid) -> anyhow::Result<Option<Order>> {
            Ok(self.0.lock().unwrap().iter().find(|o| o.id == id).cloned())
        }
        async fn find_awaiting_courier(&self) -> anyhow::Result<Vec<Order>> { Ok(vec![]) }
        async fn list_summaries_for_customer(&self, _t: Uuid, _c: Uuid, _l: i64)
            -> anyhow::Result<Vec<crate::domain::repositories::OrderSummary>> { Ok(vec![]) }
    }

    struct NoopLedgers;
    #[async_trait::async_trait]
    impl VendorLedgerRepository for NoopLedgers {
        async fn find_open(&self, _t: Uuid, _v: Uuid, _p: &str) -> anyhow::Result<Option<VendorLedger>> { Ok(None) }
        async fn list_recent(&self, _t: Uuid, _v: Uuid, _l: i64) -> anyhow::Result<Vec<LedgerPeriod>> { Ok(vec![]) }
        async fn save(&self, _l: &VendorLedger) -> anyhow::Result<()> { Ok(()) }
    }

    /// Unlike `NoopLedgers`, records every `save` so a test can assert a
    /// vendor was — or, more importantly, was not — credited at all.
    #[derive(Default)]
    struct Ledgers {
        credit_calls: Mutex<Vec<Uuid>>,
    }
    #[async_trait::async_trait]
    impl VendorLedgerRepository for Ledgers {
        async fn find_open(&self, _t: Uuid, _v: Uuid, _p: &str) -> anyhow::Result<Option<VendorLedger>> { Ok(None) }
        async fn list_recent(&self, _t: Uuid, _v: Uuid, _l: i64) -> anyhow::Result<Vec<LedgerPeriod>> { Ok(vec![]) }
        async fn save(&self, l: &VendorLedger) -> anyhow::Result<()> {
            self.credit_calls.lock().unwrap().push(l.vendor_id);
            Ok(())
        }
    }

    struct NoopTelemetry;
    #[async_trait::async_trait]
    impl TelemetryRepository for NoopTelemetry {
        async fn append(&self, _e: &TelemetryEvent) -> anyhow::Result<()> { Ok(()) }
        async fn timeline(&self, _t: Uuid, _o: Uuid) -> anyhow::Result<Vec<TelemetryEvent>> { Ok(vec![]) }
    }

    struct NoopEvents;
    #[async_trait::async_trait]
    impl crate::infrastructure::messaging::OrderEvents for NoopEvents {
        async fn order_placed(&self, _o: &Order) -> anyhow::Result<()> { Ok(()) }
        async fn order_delivered(&self, _o: &Order) -> anyhow::Result<()> { Ok(()) }
    }

    #[derive(Default)]
    struct Payments {
        capture_calls: Mutex<Vec<Uuid>>,
    }
    #[async_trait::async_trait]
    impl crate::application::services::order_payments::OrderPayments for Payments {
        async fn authorize(&self, _t: Uuid, _o: Uuid, _a: i64, _c: &str, _r: &str)
            -> anyhow::Result<crate::application::services::order_payments::AuthorizedIntent> {
            unreachable!("this handler never opens a new authorization")
        }
        async fn capture(&self, intent_id: Uuid, _amount_cents: Option<i64>) -> anyhow::Result<()> {
            self.capture_calls.lock().unwrap().push(intent_id);
            Ok(())
        }
        async fn void(&self, _intent_id: Uuid) -> anyhow::Result<()> {
            unreachable!("this handler never voids")
        }
    }

    fn authorized_online_order() -> (Order, Uuid) {
        let leg = VendorLeg::settle(TENANT, Uuid::new_v4(), 34_000, 1_500);
        let mut o = Order::place(
            TENANT, Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(),
            vec![leg], 7_900, 4_000, 5_800, 14.5995, 120.9842,
        );
        let prepaid = o.grand_total_cents;
        o = o.with_payment(PaymentMethod::Online, prepaid);
        let intent_id = Uuid::new_v4();
        o.payment_authorized(intent_id).unwrap();
        (o, intent_id)
    }

    fn handler(orders: Arc<Orders>, payments: Arc<Payments>) -> CourierMilestoneHandler {
        CourierMilestoneHandler::new(orders, Arc::new(NoopLedgers), Arc::new(NoopTelemetry),
                                      Arc::new(NoopEvents), payments)
    }

    /// The signal this whole feature turns on: field-ops' `Assigned` event —
    /// not `dispatch.offer`'s return value — is what actually means a courier
    /// took the job. That is exactly when a prepaid order's ring-fenced hold
    /// must become a real capture.
    #[tokio::test]
    async fn a_courier_accepting_an_authorized_order_captures_the_hold_exactly_once() {
        let (order, intent_id) = authorized_online_order();
        let order_id = order.id;
        let orders = Arc::new(Orders(Mutex::new(vec![order])));
        let payments = Arc::new(Payments::default());
        let h = handler(orders.clone(), payments.clone());

        h.handle(CourierEvent::Assigned {
            tenant_id: TENANT, product: "omnideliv".into(), external_ref: order_id,
            courier_id: Uuid::new_v4(), assignment_id: Uuid::new_v4(), courier_user_id: None,
        }).await.unwrap();

        assert_eq!(payments.capture_calls.lock().unwrap().as_slice(), &[intent_id]);
        let saved = orders.0.lock().unwrap().iter().find(|o| o.id == order_id).cloned().unwrap();
        assert_eq!(saved.payment_status, PaymentStatus::Captured);
        assert_eq!(saved.status, OrderStatus::Collecting);
    }

    /// Kafka redelivers `Assigned` at least once — a second delivery for the
    /// same order must not call the gateway a second time. The guard is
    /// `payment_status == Authorized`, which is already `Captured` by then.
    #[tokio::test]
    async fn a_redelivered_assigned_event_does_not_capture_twice() {
        let (order, _intent_id) = authorized_online_order();
        let order_id = order.id;
        let assignment_id = Uuid::new_v4();
        let orders = Arc::new(Orders(Mutex::new(vec![order])));
        let payments = Arc::new(Payments::default());
        let h = handler(orders.clone(), payments.clone());

        let event = || CourierEvent::Assigned {
            tenant_id: TENANT, product: "omnideliv".into(), external_ref: order_id,
            courier_id: Uuid::new_v4(), assignment_id, courier_user_id: None,
        };

        h.handle(event()).await.unwrap();
        h.handle(event()).await.unwrap();

        assert_eq!(payments.capture_calls.lock().unwrap().len(), 1, "capture must fire exactly once");
    }

    /// A COD order's `payment_status` never leaves `Pending` — the capture
    /// branch must not fire, and the gateway must never be called.
    #[tokio::test]
    async fn a_cod_order_never_touches_the_gateway_on_assignment() {
        let leg = VendorLeg::settle(TENANT, Uuid::new_v4(), 34_000, 1_500);
        let order = Order::place(
            TENANT, Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(),
            vec![leg], 7_900, 4_000, 5_800, 14.5995, 120.9842,
        );
        let order_id = order.id;
        let orders = Arc::new(Orders(Mutex::new(vec![order])));
        let payments = Arc::new(Payments::default());
        let h = handler(orders.clone(), payments.clone());

        h.handle(CourierEvent::Assigned {
            tenant_id: TENANT, product: "omnideliv".into(), external_ref: order_id,
            courier_id: Uuid::new_v4(), assignment_id: Uuid::new_v4(), courier_user_id: None,
        }).await.unwrap();

        assert!(payments.capture_calls.lock().unwrap().is_empty(), "COD must never call the gateway");
        let saved = orders.0.lock().unwrap().iter().find(|o| o.id == order_id).cloned().unwrap();
        assert_eq!(saved.status, OrderStatus::Collecting, "the courier claim itself is unaffected");
    }

    /// The guard `Collected` relies on: a late or out-of-order collection
    /// event for a leg the vendor already `Rejected` must not resurrect it as
    /// `PickedUp` and must not credit the vendor's ledger. This is the
    /// highest-stakes assertion in the file — a regression here is a store
    /// paid for goods it refused to hand over.
    #[tokio::test]
    async fn a_collection_event_for_a_rejected_leg_is_not_credited() {
        let vendor_id = Uuid::new_v4();
        let mut leg = VendorLeg::settle(TENANT, vendor_id, 1_000, 1_500);
        leg.status = LegStatus::Rejected;
        let order = Order::place(
            TENANT, Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(),
            vec![leg], 0, 0, 0, 14.5995, 120.9842,
        );
        let order_id = order.id;
        let orders = Arc::new(Orders(Mutex::new(vec![order])));
        let ledgers = Arc::new(Ledgers::default());
        let h = CourierMilestoneHandler::new(
            orders.clone(), ledgers.clone(), Arc::new(NoopTelemetry),
            Arc::new(NoopEvents), Arc::new(Payments::default()),
        );

        h.handle(CourierEvent::Collected {
            tenant_id: TENANT, product: "omnideliv".into(), external_ref: order_id,
            courier_id: Uuid::new_v4(), vendor_id, device_timestamp: None,
        }).await.unwrap();

        assert!(
            ledgers.credit_calls.lock().unwrap().is_empty(),
            "a rejected leg must never be credited",
        );
        let saved = orders.0.lock().unwrap().iter().find(|o| o.id == order_id).cloned().unwrap();
        assert_eq!(
            saved.legs[0].status, LegStatus::Rejected,
            "the leg status must not be overwritten by a stray collection event",
        );
    }
}
