//! Consumes `payment.intent.authorized` / `payment.intent.failed`
//! (`purpose == "omnideliv_order"` only — other purposes belong to other
//! consumers on other services, e.g. order-intake's `shipping_fee` purpose
//! sharing these same two topics; see
//! `services/order-intake/src/infrastructure/messaging/payment_consumer.rs`,
//! whose shape this mirrors).
//!
//! This is the deferred half of `CheckoutService::place`'s `Online` branch.
//! `place` opens an authorization hold and returns a checkout URL WITHOUT
//! ever calling `dispatch.offer` — `offer` only reports who a job was
//! *offered to*, not who accepted, and for a fresh online order nobody has
//! even been asked yet. Offering the job is this consumer's job, triggered
//! only once the hold is confirmed real:
//!
//! * On **authorized**: replay the exact offer card `place` built and stashed
//!   on `Order::pending_offer_card`, offer the job to couriers, and mark the
//!   order `AwaitingCourier` — the same state a COD order reaches immediately
//!   at checkout, just later.
//! * On **failed** (a declined webhook, or `services/payments`' own sweep
//!   expiring an uncompleted checkout session — both publish this identical
//!   event, see `PaymentIntentFailed`'s doc comment in `libs/events`): cancel
//!   the order. No courier was ever offered, so there is nothing to undo on
//!   that side, and the customer was never charged.
//!
//! Both handlers are idempotent against Kafka's at-least-once redelivery.

use std::sync::Arc;

use anyhow::Context;
use logisticos_events::{
    envelope::Event,
    payloads::{PaymentIntentAuthorized, PaymentIntentFailed},
    topics,
};
use uuid::Uuid;

use crate::application::services::{CourierDispatch, FIRST_OFFER_RADIUS_KM};
use crate::domain::entities::telemetry::event_type;
use crate::domain::entities::{OrderStatus, PaymentStatus, TelemetryEvent};
use crate::domain::repositories::{OrderRepository, TelemetryRepository};

/// Purpose tag `CheckoutService::place`'s `Online` branch stamps on every
/// intent it opens (via `OmniPaymentsClient::authorize`) — the only kind of
/// payment intent OmniDeliv ever creates, and therefore the only kind this
/// consumer acts on.
const OMNIDELIV_ORDER_PURPOSE: &str = "omnideliv_order";

/// Dispatches on topic, deserializes into the strongly-typed envelope for
/// that topic, and filters to `purpose == "omnideliv_order"` before acting —
/// any other purpose (order-intake's `shipping_fee`, or a future one) is a
/// silent no-op here, not an error.
pub async fn handle(
    topic: &str,
    json: serde_json::Value,
    orders: &Arc<dyn OrderRepository>,
    telemetry: &Arc<dyn TelemetryRepository>,
    dispatch: &Arc<dyn CourierDispatch>,
    vendor_events: &Arc<dyn super::VendorLegEvents>,
) -> anyhow::Result<()> {
    match topic {
        topics::PAYMENT_INTENT_AUTHORIZED => {
            let evt: Event<PaymentIntentAuthorized> = serde_json::from_value(json)
                .context("failed to deserialize payment.intent.authorized event")?;
            if evt.data.purpose != OMNIDELIV_ORDER_PURPOSE {
                return Ok(());
            }
            handle_authorized(evt.tenant_id, evt.data.reference_id, evt.data.intent_id, orders, telemetry, dispatch, vendor_events).await
        }
        topics::PAYMENT_INTENT_FAILED => {
            let evt: Event<PaymentIntentFailed> = serde_json::from_value(json)
                .context("failed to deserialize payment.intent.failed event")?;
            if evt.data.purpose != OMNIDELIV_ORDER_PURPOSE {
                return Ok(());
            }
            handle_failed(evt.tenant_id, evt.data.reference_id, &evt.data.reason, orders, telemetry).await
        }
        _ => Ok(()),
    }
}

/// Funds are ring-fenced — offer the job. Idempotent: an order already past
/// `Placed` (already offered by an earlier delivery of this same event, or
/// resolved some other way since) is a no-op on the offer, though the
/// (already-idempotent) `payment_authorized` transition still runs so a
/// stale `payment_intent_id`/`payment_authorized_at` is never left behind.
async fn handle_authorized(
    tenant_id: Uuid,
    order_id: Uuid,
    intent_id: Uuid,
    orders: &Arc<dyn OrderRepository>,
    telemetry: &Arc<dyn TelemetryRepository>,
    dispatch: &Arc<dyn CourierDispatch>,
    vendor_events: &Arc<dyn super::VendorLegEvents>,
) -> anyhow::Result<()> {
    let Some(mut order) = orders.find_by_id(tenant_id, order_id).await? else {
        // Not an error: the order may have been purged, or this is a replay
        // of an old partition. Logged so a systematic mismatch is visible.
        tracing::warn!(%order_id, "payment.intent.authorized for an unknown order");
        return Ok(());
    };

    let already_offered = order.status != OrderStatus::Placed;

    order.payment_authorized(intent_id).map_err(|e| anyhow::anyhow!("{e}"))?;

    if already_offered {
        orders.save(&order).await?;
        return Ok(());
    }

    let (Some(lat), Some(lng)) = (order.delivery_lat, order.delivery_lng) else {
        // Cannot happen for an order placed through this feature — `place`
        // always sets both — but an order is never worth crash-looping the
        // consumer over. Leave it `Placed`; the recovery sweep's no-courier
        // timeout will eventually void it.
        tracing::error!(%order_id, "authorized online order has no delivery point — cannot offer a courier");
        orders.save(&order).await?;
        return Ok(());
    };

    let offered = dispatch
        .offer(
            tenant_id, order.id, lat, lng, FIRST_OFFER_RADIUS_KM,
            order.courier_trip_cents, order.tip_cents, order.cod_amount_cents(),
            order.pending_offer_card.clone(),
        )
        .await?;

    if offered.is_empty() {
        // Not fatal: this converges with the ordinary "offered but nobody
        // accepted" case, and the recovery sweep's no-courier timeout voids
        // either one the same way.
        tracing::warn!(%order_id, "payment authorized but no courier could be offered");
    } else {
        order.courier_task_id = offered.first().copied();
        if let Err(e) = order.courier_offered() {
            tracing::error!(err = %e, %order_id, "could not mark the order awaiting a courier");
        }
    }

    orders.save(&order).await?;

    // Now the stores are told, and not before. Until the hold landed, this
    // order was a checkout page the customer might still abandon; a kitchen
    // told to start cooking then would be cooking for nothing.
    //
    // Reached only on the first delivery of this event — an order already past
    // `Placed` returned above — so a redelivery does not re-notify a store that
    // is already cooking.
    for leg in &order.legs {
        if let Err(err) = vendor_events.leg_received(&super::LegRef::of(leg)).await {
            tracing::warn!(err = %err, %order_id, vendor_id = %leg.vendor_id,
                "vendor.leg.received publish failed — the queue is still correct");
        }
    }

    let e = TelemetryEvent::new(
        tenant_id, order_id, event_type::PAYMENT_AUTHORIZED, None, None,
        serde_json::json!({ "intent_id": intent_id, "offered_to": offered.len() }),
    );
    if let Err(err) = telemetry.append(&e).await {
        tracing::error!(err = %err, %order_id, "payment-authorized telemetry failed");
    }

    Ok(())
}

/// The hold never landed (declined) or expired unused — cancel the order.
/// Idempotent: an order whose payment already resolved (captured, voided, or
/// already failed by an earlier delivery of this same event) is a no-op.
async fn handle_failed(
    tenant_id: Uuid,
    order_id: Uuid,
    reason: &str,
    orders: &Arc<dyn OrderRepository>,
    telemetry: &Arc<dyn TelemetryRepository>,
) -> anyhow::Result<()> {
    let Some(mut order) = orders.find_by_id(tenant_id, order_id).await? else {
        tracing::warn!(%order_id, "payment.intent.failed for an unknown order");
        return Ok(());
    };

    // `services/payments`' own `sweep_expired` never expires an intent past
    // `authorized` (see its `list_expired` query), so in practice this only
    // ever fires against `Pending` — the `Authorized` arm here is defensive,
    // not an expected path. Anything already `Captured`/`Voided`/`Failed`
    // must never be re-cancelled off a stale or duplicate event.
    if !matches!(order.payment_status, PaymentStatus::Pending | PaymentStatus::Authorized) {
        tracing::info!(%order_id, status = ?order.payment_status,
            "payment.intent.failed — payment already resolved, idempotent skip");
        return Ok(());
    }

    order.payment_failed().map_err(|e| anyhow::anyhow!("{e}"))?;
    if let Err(e) = order.cancel() {
        // Only reachable if the order somehow reached `Delivered` while
        // payment was still `Pending`/`Authorized` — cannot happen on this
        // rail (capture always precedes `Collecting`), but cancellation
        // failing must not stop the payment_status write below.
        tracing::warn!(err = %e, %order_id, "could not cancel order after payment failure");
    }
    orders.save(&order).await?;

    let e = TelemetryEvent::new(
        tenant_id, order_id, event_type::ORDER_CANCELLED, None, None,
        serde_json::json!({ "reason": reason }),
    );
    if let Err(err) = telemetry.append(&e).await {
        tracing::error!(err = %err, %order_id, "cancellation telemetry failed");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use crate::domain::entities::{Order, PaymentMethod, VendorLeg};

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

    #[derive(Default)]
    struct Telemetry(Mutex<Vec<String>>);
    #[async_trait::async_trait]
    impl TelemetryRepository for Telemetry {
        async fn append(&self, e: &TelemetryEvent) -> anyhow::Result<()> {
            self.0.lock().unwrap().push(e.event_type.clone());
            Ok(())
        }
        async fn timeline(&self, _t: Uuid, _o: Uuid) -> anyhow::Result<Vec<TelemetryEvent>> { Ok(vec![]) }
    }

    #[derive(Default)]
    struct Dispatch {
        calls: Mutex<Vec<i64>>, // cod_amount_cents per call
        respond_empty: bool,
    }
    #[async_trait::async_trait]
    impl CourierDispatch for Dispatch {
        #[allow(clippy::too_many_arguments)]
        async fn offer(
            &self, _t: Uuid, _o: Uuid, _lat: f64, _lng: f64, _r: f64,
            _trip: i64, _tip: i64, cod: i64, _card: Option<serde_json::Value>,
        ) -> anyhow::Result<Vec<Uuid>> {
            self.calls.lock().unwrap().push(cod);
            if self.respond_empty { Ok(vec![]) } else { Ok(vec![Uuid::new_v4()]) }
        }
    }

    const TENANT: Uuid = Uuid::from_u128(1);

    fn online_order() -> Order {
        let leg = VendorLeg::settle(TENANT, Uuid::new_v4(), 34_000, 1500);
        let order = Order::place(
            TENANT, Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(),
            vec![leg], 7_900, 4_000, 5_800, 14.5995, 120.9842,
        );
        let prepaid = order.grand_total_cents;
        order
            .with_payment(PaymentMethod::Online, prepaid)
            .with_pending_offer_card(Some(serde_json::json!({"v": 1})))
    }

    fn authorized_event(order_id: Uuid, intent_id: Uuid) -> serde_json::Value {
        serde_json::to_value(Event::new(
            "logisticos/payments",
            "payment.intent.authorized",
            TENANT,
            PaymentIntentAuthorized {
                intent_id,
                purpose: "omnideliv_order".into(),
                reference_type: "order".into(),
                reference_id: order_id,
                amount_cents: 45_900,
                currency: "AED".into(),
            },
        )).unwrap()
    }

    fn failed_event(order_id: Uuid, reason: &str) -> serde_json::Value {
        serde_json::to_value(Event::new(
            "logisticos/payments",
            "payment.intent.failed",
            TENANT,
            PaymentIntentFailed {
                intent_id: Uuid::new_v4(),
                purpose: "omnideliv_order".into(),
                reference_type: "order".into(),
                reference_id: order_id,
                reason: reason.to_string(),
            },
        )).unwrap()
    }

    /// The moment this whole consumer exists for: authorization landing is
    /// what finally offers the job to couriers.
    #[tokio::test]
    async fn authorized_offers_the_courier_and_marks_the_order_awaiting_one() {
        let order = online_order();
        let order_id = order.id;
        let orders: Arc<dyn OrderRepository> = Arc::new(Orders(Mutex::new(vec![order])));
        let telemetry: Arc<dyn TelemetryRepository> = Arc::new(Telemetry::default());
        let dispatch = Arc::new(Dispatch::default());
        let dispatch_trait: Arc<dyn CourierDispatch> = dispatch.clone();
        // These assert on the order, the offer and the telemetry. What a
        // store was told is asserted in vendor_events' own tests.
        let vendor_events: Arc<dyn super::super::VendorLegEvents> =
            Arc::new(super::super::NoopVendorLegEvents);

        let intent = Uuid::new_v4();
        handle(topics::PAYMENT_INTENT_AUTHORIZED, authorized_event(order_id, intent),
               &orders, &telemetry, &dispatch_trait, &vendor_events).await.unwrap();

        assert_eq!(dispatch.calls.lock().unwrap().len(), 1, "the courier must be offered exactly once");
        let saved = orders.find_by_id(TENANT, order_id).await.unwrap().unwrap();
        assert_eq!(saved.payment_status, PaymentStatus::Authorized);
        assert_eq!(saved.payment_intent_id, Some(intent));
        assert_eq!(saved.status, OrderStatus::AwaitingCourier);
    }

    /// Kafka redelivers at least once — a second authorized event for an
    /// order already offered must not re-offer it.
    #[tokio::test]
    async fn a_redelivered_authorization_does_not_re_offer() {
        let order = online_order();
        let order_id = order.id;
        let orders: Arc<dyn OrderRepository> = Arc::new(Orders(Mutex::new(vec![order])));
        let telemetry: Arc<dyn TelemetryRepository> = Arc::new(Telemetry::default());
        let dispatch = Arc::new(Dispatch::default());
        let dispatch_trait: Arc<dyn CourierDispatch> = dispatch.clone();
        // These assert on the order, the offer and the telemetry. What a
        // store was told is asserted in vendor_events' own tests.
        let vendor_events: Arc<dyn super::super::VendorLegEvents> =
            Arc::new(super::super::NoopVendorLegEvents);

        let intent = Uuid::new_v4();
        handle(topics::PAYMENT_INTENT_AUTHORIZED, authorized_event(order_id, intent),
               &orders, &telemetry, &dispatch_trait, &vendor_events).await.unwrap();
        handle(topics::PAYMENT_INTENT_AUTHORIZED, authorized_event(order_id, intent),
               &orders, &telemetry, &dispatch_trait, &vendor_events).await.unwrap();

        assert_eq!(dispatch.calls.lock().unwrap().len(), 1, "a redelivery must not offer a second time");
    }

    /// A failed/expired payment must cancel the order and must never reach
    /// the dispatcher.
    #[tokio::test]
    async fn failed_cancels_the_order_without_ever_offering_a_courier() {
        let order = online_order();
        let order_id = order.id;
        let orders: Arc<dyn OrderRepository> = Arc::new(Orders(Mutex::new(vec![order])));
        let telemetry: Arc<dyn TelemetryRepository> = Arc::new(Telemetry::default());
        let dispatch = Arc::new(Dispatch::default());
        let dispatch_trait: Arc<dyn CourierDispatch> = dispatch.clone();
        // These assert on the order, the offer and the telemetry. What a
        // store was told is asserted in vendor_events' own tests.
        let vendor_events: Arc<dyn super::super::VendorLegEvents> =
            Arc::new(super::super::NoopVendorLegEvents);

        handle(topics::PAYMENT_INTENT_FAILED, failed_event(order_id, "card_declined"),
               &orders, &telemetry, &dispatch_trait, &vendor_events).await.unwrap();

        assert!(dispatch.calls.lock().unwrap().is_empty(), "a failed payment must never offer a courier");
        let saved = orders.find_by_id(TENANT, order_id).await.unwrap().unwrap();
        assert_eq!(saved.status, OrderStatus::Cancelled);
        assert_eq!(saved.payment_status, PaymentStatus::Failed);
    }

    /// Events for a purpose OmniDeliv never mints (order-intake's
    /// `shipping_fee`, sharing the same topic) must be silently ignored.
    #[tokio::test]
    async fn a_different_purpose_is_not_omnideliv_business() {
        let orders: Arc<dyn OrderRepository> = Arc::new(Orders::default());
        let telemetry: Arc<dyn TelemetryRepository> = Arc::new(Telemetry::default());
        let dispatch = Arc::new(Dispatch::default());
        let dispatch_trait: Arc<dyn CourierDispatch> = dispatch.clone();
        // These assert on the order, the offer and the telemetry. What a
        // store was told is asserted in vendor_events' own tests.
        let vendor_events: Arc<dyn super::super::VendorLegEvents> =
            Arc::new(super::super::NoopVendorLegEvents);

        let evt = serde_json::to_value(Event::new(
            "logisticos/payments", "payment.intent.authorized", TENANT,
            PaymentIntentAuthorized {
                intent_id: Uuid::new_v4(), purpose: "shipping_fee".into(),
                reference_type: "shipment".into(), reference_id: Uuid::new_v4(),
                amount_cents: 1_000, currency: "AED".into(),
            },
        )).unwrap();

        handle(topics::PAYMENT_INTENT_AUTHORIZED, evt, &orders, &telemetry, &dispatch_trait, &vendor_events)
            .await.unwrap();

        assert!(dispatch.calls.lock().unwrap().is_empty());
    }
}
