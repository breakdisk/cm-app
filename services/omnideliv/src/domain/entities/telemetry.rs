//! Append-only order timeline.
//!
//! Every state transition is a new event. Nothing here is ever updated, so an
//! order's history can always be reconstructed — which is what makes an SLA
//! dispute answerable rather than a matter of opinion.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The event types this service emits. Constants rather than free strings so a
/// typo is a compile error instead of an event nothing queries for.
pub mod event_type {
    pub const ORDER_PLACED:      &str = "order.placed";
    pub const LEG_PICKED_UP:     &str = "vendor_leg.picked_up";
    pub const LEG_FAILED:        &str = "vendor_leg.failed";
    /// The store never answered. Written by the sweep, which deliberately
    /// leaves the leg `pending` rather than rejecting it.
    pub const VENDOR_LEG_UNANSWERED: &str = "vendor_leg.unanswered";
    pub const COURIER_CLAIMED:   &str = "courier.claimed";
    /// The courier is at a stop. Not a lifecycle transition — the order
    /// status is unchanged — but the event a customer most wants pushed.
    pub const COURIER_ARRIVED:   &str = "courier.arrived";
    pub const ORDER_DELIVERED:   &str = "order.delivered";
    pub const ORDER_CANCELLED:   &str = "order.cancelled";
    /// Paid, but no courier accepted within the retry window.
    pub const COURIER_REOFFERED: &str = "courier.reoffered";
    pub const ORDER_ESCALATED:   &str = "order.escalated";
    /// `payment.intent.authorized` landed for an `Online` order — the courier
    /// offer that COD makes immediately at checkout happens here instead.
    pub const PAYMENT_AUTHORIZED: &str = "payment.authorized";
    /// A courier accepted an `Online` order's job and the authorization hold
    /// was captured.
    pub const PAYMENT_CAPTURED:   &str = "payment.captured";
    /// No courier accepted an `Online` order within the no-courier timeout —
    /// the authorization hold was released. The customer was never charged.
    pub const PAYMENT_VOIDED:     &str = "payment.voided";
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEvent {
    pub id:               Uuid,
    pub order_id:         Uuid,
    pub tenant_id:        Uuid,
    pub event_type:       String,
    /// Hardware clock at the physical moment of the event. `None` for
    /// server-generated events, which have no device behind them.
    pub device_timestamp: Option<DateTime<Utc>>,
    pub server_timestamp: DateTime<Utc>,
    pub actor_id:         Option<Uuid>,
    pub payload:          serde_json::Value,
}

impl TelemetryEvent {
    pub fn new(
        tenant_id: Uuid,
        order_id: Uuid,
        event_type: impl Into<String>,
        device_timestamp: Option<DateTime<Utc>>,
        actor_id: Option<Uuid>,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            order_id,
            tenant_id,
            event_type: event_type.into(),
            device_timestamp,
            server_timestamp: Utc::now(),
            actor_id,
            payload,
        }
    }

    /// The timestamp SLA maths uses: the device clock where we have it, backend
    /// receipt time only as a fallback for server-generated events.
    ///
    /// Using `server_timestamp` alone would attribute network latency and queue
    /// depth to the courier, which turns a platform problem into a rider's
    /// performance score.
    pub fn sla_timestamp(&self) -> DateTime<Utc> {
        self.device_timestamp.unwrap_or(self.server_timestamp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sla_timestamp_prefers_the_device_clock() {
        let device = Utc::now() - chrono::Duration::seconds(90);
        let e = TelemetryEvent::new(Uuid::new_v4(), Uuid::new_v4(), event_type::LEG_PICKED_UP,
                                    Some(device), None, serde_json::json!({}));
        assert_eq!(e.sla_timestamp(), device);
    }

    #[test]
    fn sla_timestamp_falls_back_for_server_generated_events() {
        let e = TelemetryEvent::new(Uuid::new_v4(), Uuid::new_v4(), event_type::ORDER_PLACED,
                                    None, None, serde_json::json!({}));
        assert_eq!(e.sla_timestamp(), e.server_timestamp);
    }

    /// The reason the distinction exists: a pickup scanned 90 seconds before the
    /// server heard about it must not be measured as 90 seconds slower. Choosing
    /// server time would move that latency onto the courier's SLA.
    #[test]
    fn a_delayed_upload_does_not_penalise_the_courier() {
        let scanned_at = Utc::now() - chrono::Duration::seconds(90);
        let e = TelemetryEvent::new(Uuid::new_v4(), Uuid::new_v4(), event_type::LEG_PICKED_UP,
                                    Some(scanned_at), Some(Uuid::new_v4()), serde_json::json!({}));

        let attributed_delay = e.server_timestamp - e.sla_timestamp();
        assert!(
            attributed_delay >= chrono::Duration::seconds(89),
            "the gap belongs to the upload, and sla_timestamp must exclude it",
        );
        assert_eq!(e.sla_timestamp(), scanned_at);
    }
}
