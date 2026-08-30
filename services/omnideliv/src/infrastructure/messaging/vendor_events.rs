//! What omnideliv tells a vendor about its own work.
//!
//! Keyed on `vendor_id` rather than `order_id`, which is the whole reason this
//! is a separate topic from `order.placed` instead of a wider payload on it:
//!
//! - A stall needs its own messages in order. One foodcourt order produces one
//!   message per stall, and keying on the order would put three stalls' work on
//!   one partition and interleave it.
//! - `order.placed`'s consumer resolves a *customer* from the payload. Adding a
//!   vendor recipient to it would put two audiences with two different
//!   authorization rules on one message.
//!
//! Nothing consumes these yet. The vendor queue endpoint is the record, and
//! these exist so a later notification transport has something to wake on — a
//! dropped message costs a poll interval, never an order.

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

    /// Carries this vendor's leg and nothing else.
    ///
    /// Deliberately no customer, no address, and no sibling legs: a stall has no
    /// reason to hold a delivery address, and in a foodcourt the neighbouring
    /// stall has no reason to learn what this one was asked to make.
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

/// Used when the broker is unreachable at startup — the same trade
/// `NoopOrderEvents` makes. A vendor whose tablet has to poll is better off than
/// a vendor who cannot be given an order at all, and the queue endpoint is the
/// record regardless of whether any message was delivered.
pub struct NoopVendorLegEvents;

#[async_trait]
impl VendorLegEvents for NoopVendorLegEvents {
    async fn leg_received(&self, _leg: &VendorLeg) -> anyhow::Result<()> {
        Ok(())
    }
    async fn leg_accepted(&self, _leg: &VendorLeg, _ready_in_minutes: i32) -> anyhow::Result<()> {
        Ok(())
    }
    async fn leg_rejected(&self, _leg: &VendorLeg, _reason: &str) -> anyhow::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::LegStatus;

    fn leg() -> VendorLeg {
        let mut l = VendorLeg::settle(Uuid::new_v4(), Uuid::new_v4(), 12_500, 1_500);
        l.order_id = Uuid::new_v4();
        l
    }

    #[test]
    fn the_payload_carries_the_recipient() {
        // The vendor is who this message is for. Without it the message is
        // undeliverable and the partition key is meaningless.
        let l = leg();
        let p = KafkaVendorLegEvents::payload(&l, serde_json::json!({}));

        assert_eq!(p["vendor_id"], serde_json::json!(l.vendor_id));
        assert!(!p["vendor_id"].is_null());
    }

    #[test]
    fn the_payload_carries_what_the_store_needs_to_act() {
        let l = leg();
        let p = KafkaVendorLegEvents::payload(&l, serde_json::json!({}));

        assert_eq!(p["leg_id"], serde_json::json!(l.id));
        assert_eq!(p["order_id"], serde_json::json!(l.order_id));
        assert_eq!(p["goods_subtotal_cents"], serde_json::json!(12_500));
        assert_eq!(p["status"], serde_json::json!("pending"));
    }

    #[test]
    fn the_payload_discloses_nothing_about_the_customer_or_the_money_split() {
        // A stall is told what it must make, not who ordered it, where it is
        // going, or what the platform's cut was.
        let l = leg();
        let p = KafkaVendorLegEvents::payload(&l, serde_json::json!({}));

        for leaked in [
            "customer_id", "customer_name", "customer_phone",
            "delivery_lat", "delivery_lng", "delivery_note",
            "commission_cents", "payout_cents", "grand_total_cents",
        ] {
            assert!(p.get(leaked).is_none(), "payload leaked {leaked}");
        }
    }

    #[test]
    fn extra_fields_are_merged_without_displacing_the_base() {
        let l = leg();
        let p = KafkaVendorLegEvents::payload(&l, serde_json::json!({ "ready_in_minutes": 20 }));

        assert_eq!(p["ready_in_minutes"], serde_json::json!(20));
        assert_eq!(p["leg_id"], serde_json::json!(l.id), "base field survived the merge");
    }

    #[test]
    fn the_status_travels_with_the_leg_rather_than_being_implied_by_the_topic() {
        // A consumer that inferred status from the topic name would be wrong
        // the moment a message is redelivered out of order.
        let mut l = leg();
        l.status = LegStatus::Accepted;
        let p = KafkaVendorLegEvents::payload(&l, serde_json::json!({}));

        assert_eq!(p["status"], serde_json::json!("accepted"));
    }
}
