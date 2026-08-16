//! Publishes the two order moments a customer is waiting on.
//!
//! Before this, a customer placed an order and heard nothing: no confirmation,
//! no delivery notice. The order status was visible to anyone who kept the
//! tracking screen open and invisible to everyone else.
//!
//! Deliberately only two events, not one per transition. `collecting` and
//! `delivering` are progress a tracking screen shows well and a push
//! notification shows badly — a phone buzzing four times for one dinner is
//! worse than silence. The bookends are what a customer actually needs told.
//!
//! Engagement consumes these through its generic branch, which resolves the
//! recipient from `customer_id`, so that field is required rather than
//! best-effort.

use std::sync::Arc;

use async_trait::async_trait;
use logisticos_events::topics;

use crate::domain::entities::Order;

/// What omnideliv tells the rest of the platform about an order.
///
/// A trait so the checkout and milestone paths depend on the intent rather than
/// on Kafka, and so a broker outage is a no-op rather than a compile-time
/// coupling — see `NoopOrderEvents`.
#[async_trait]
pub trait OrderEvents: Send + Sync {
    async fn order_placed(&self, order: &Order) -> anyhow::Result<()>;
    async fn order_delivered(&self, order: &Order) -> anyhow::Result<()>;
}

pub struct KafkaOrderEvents {
    producer: Arc<logisticos_events::producer::KafkaProducer>,
}

impl KafkaOrderEvents {
    pub fn new(producer: Arc<logisticos_events::producer::KafkaProducer>) -> Self {
        Self { producer }
    }

    fn payload(order: &Order) -> serde_json::Value {
        serde_json::json!({
            "customer_id":       order.customer_id,
            "tenant_id":         order.tenant_id,
            "order_id":          order.id,
            "grand_total_cents": order.grand_total_cents,
            "stops":             order.legs.len(),
        })
    }

    async fn emit(&self, topic: &str, order: &Order) -> anyhow::Result<()> {
        // Keyed on the order, not the tenant: every event about one order lands
        // on one partition and therefore in order, which is what stops a
        // "delivered" push arriving before its "placed".
        self.producer
            .publish_raw(
                topic,
                &order.id.to_string(),
                &serde_json::to_string(&Self::payload(order))?,
            )
            .await
    }
}

#[async_trait]
impl OrderEvents for KafkaOrderEvents {
    async fn order_placed(&self, order: &Order) -> anyhow::Result<()> {
        self.emit(topics::OMNIDELIV_ORDER_PLACED, order).await
    }

    async fn order_delivered(&self, order: &Order) -> anyhow::Result<()> {
        self.emit(topics::OMNIDELIV_ORDER_DELIVERED, order).await
    }
}

/// Used when the broker is unreachable at startup. Orders still place and
/// deliver; only the notifications are lost, which is the right trade — a
/// customer who cannot order is worse off than one who is not messaged.
pub struct NoopOrderEvents;

#[async_trait]
impl OrderEvents for NoopOrderEvents {
    async fn order_placed(&self, _: &Order) -> anyhow::Result<()> { Ok(()) }
    async fn order_delivered(&self, _: &Order) -> anyhow::Result<()> { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::VendorLeg;
    use uuid::Uuid;

    fn order() -> Order {
        Order::place(
            Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(),
            vec![VendorLeg::settle(Uuid::new_v4(), Uuid::new_v4(), 34_000, 1500)],
            4_900, 0, 3_500, 14.5995, 120.9842,
        )
    }

    /// Engagement's generic branch resolves the recipient from `customer_id`
    /// and skips the notification entirely without it. A payload missing this
    /// field is a message nobody receives, with a warning in a log nobody reads.
    #[test]
    fn the_payload_carries_the_recipient() {
        let o = order();
        let p = KafkaOrderEvents::payload(&o);

        assert_eq!(p["customer_id"], serde_json::json!(o.customer_id));
        assert!(!p["customer_id"].is_null());
    }

    #[test]
    fn the_payload_carries_what_the_message_needs_to_say() {
        let o = order();
        let p = KafkaOrderEvents::payload(&o);

        assert_eq!(p["order_id"], serde_json::json!(o.id));
        assert_eq!(p["grand_total_cents"], serde_json::json!(38_900));
        assert_eq!(p["stops"], serde_json::json!(1));
    }
}
