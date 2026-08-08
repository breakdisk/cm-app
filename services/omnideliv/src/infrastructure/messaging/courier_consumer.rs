//! Consumes field-ops courier milestones and advances the order.
//!
//! This is where a placed order stops being paper: a collection credits the
//! vendor's ledger, a delivery credits the courier's, and every transition
//! appends to the order timeline.

use std::sync::Arc;

use serde::Deserialize;
use uuid::Uuid;

use crate::domain::entities::telemetry::event_type;
use crate::domain::entities::{LegStatus, TelemetryEvent, VendorLedger};
use crate::domain::repositories::{OrderRepository, TelemetryRepository, VendorLedgerRepository};

pub const TOPIC_COURIER: &str = "fieldops.courier";

/// Mirrors field-ops' `CourierEvent`. Two services, one wire contract — kept in
/// sync by hand, with the tag names asserted on both sides.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum CourierEvent {
    Assigned  { tenant_id: Uuid, product: String, external_ref: Uuid, courier_id: Uuid, assignment_id: Uuid },
    Collected { tenant_id: Uuid, product: String, external_ref: Uuid, courier_id: Uuid, vendor_id: Uuid,
                device_timestamp: Option<chrono::DateTime<chrono::Utc>> },
    Delivered { tenant_id: Uuid, product: String, external_ref: Uuid, courier_id: Uuid,
                device_timestamp: Option<chrono::DateTime<chrono::Utc>> },
}

impl CourierEvent {
    fn product(&self) -> &str {
        match self {
            CourierEvent::Assigned { product, .. }
            | CourierEvent::Collected { product, .. }
            | CourierEvent::Delivered { product, .. } => product,
        }
    }

    fn tenant_id(&self) -> Uuid {
        match self {
            CourierEvent::Assigned { tenant_id, .. }
            | CourierEvent::Collected { tenant_id, .. }
            | CourierEvent::Delivered { tenant_id, .. } => *tenant_id,
        }
    }

    /// The order id. field-ops treats this as opaque; here it is the key.
    fn order_id(&self) -> Uuid {
        match self {
            CourierEvent::Assigned { external_ref, .. }
            | CourierEvent::Collected { external_ref, .. }
            | CourierEvent::Delivered { external_ref, .. } => *external_ref,
        }
    }
}

pub struct CourierMilestoneHandler {
    orders:    Arc<dyn OrderRepository>,
    ledgers:   Arc<dyn VendorLedgerRepository>,
    telemetry: Arc<dyn TelemetryRepository>,
}

impl CourierMilestoneHandler {
    pub fn new(
        orders: Arc<dyn OrderRepository>,
        ledgers: Arc<dyn VendorLedgerRepository>,
        telemetry: Arc<dyn TelemetryRepository>,
    ) -> Self {
        Self { orders, ledgers, telemetry }
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
            CourierEvent::Assigned { assignment_id, .. } => {
                order.courier_claimed(assignment_id)?;
                self.append(tenant_id, order_id, event_type::COURIER_CLAIMED, None, None,
                            serde_json::json!({ "assignment_id": assignment_id })).await;
            }

            CourierEvent::Collected { vendor_id, courier_id, device_timestamp, .. } => {
                let Some(leg) = order.legs.iter_mut().find(|l| l.vendor_id == vendor_id) else {
                    tracing::warn!(%order_id, %vendor_id, "collection for a vendor not on this order");
                    return Ok(());
                };

                // The idempotence that matters: a redelivered event must not
                // credit the vendor a second time.
                if leg.status == LegStatus::PickedUp {
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
                order.delivered()?;
                self.append(tenant_id, order_id, event_type::ORDER_DELIVERED,
                            device_timestamp, Some(courier_id), serde_json::json!({})).await;
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

    #[test]
    fn the_period_is_an_iso_week() {
        let p = current_period();
        assert!(p.contains("-W"), "got {p}");
        assert_eq!(p.len(), 8, "YYYY-Wnn, got {p}");
    }
}
