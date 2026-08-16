// Carrier / Partner
pub const CARRIER_ONBOARDED:               &str = "logisticos.carrier.onboarded";
pub const CARRIER_STATUS_CHANGED:          &str = "logisticos.carrier.status_changed";
pub const CARRIER_ALLOCATED:               &str = "logisticos.carrier.allocated";
pub const CARRIER_TRACKING_EVENT:          &str = "logisticos.carrier.tracking.event";
pub const MARKETPLACE_BOOKING_ACCEPTED:    &str = "logisticos.carrier.marketplace.booking.accepted";
pub const MARKETPLACE_BOOKING_REJECTED:    &str = "logisticos.carrier.marketplace.booking.rejected";
pub const MARKETPLACE_PICKUP_RECORDED:     &str = "logisticos.carrier.marketplace.pickup.recorded";

// Identity
pub const TENANT_CREATED:            &str = "logisticos.identity.tenant.created";
pub const TENANT_FINALIZED:          &str = "logisticos.identity.tenant.finalized";
pub const USER_INVITED:              &str = "logisticos.identity.user.invited";
pub const USER_CREATED:              &str = "logisticos.identity.user.created";
/// Emitted when identity generates an OTP for email-based login. Consumed by engagement to send the code via email.
pub const OTP_REQUESTED:             &str = "logisticos.identity.otp.requested";

// Task
pub const TASK_ASSIGNED:             &str = "logisticos.task.assigned";

// Order / Shipment
pub const SHIPMENT_CREATED:          &str = "logisticos.order.shipment.created";
pub const SHIPMENT_CONFIRMED:        &str = "logisticos.order.shipment.confirmed";
pub const SHIPMENT_CANCELLED:        &str = "logisticos.order.shipment.cancelled";
pub const SHIPMENT_RESCHEDULED:      &str = "logisticos.order.shipment.rescheduled";

// AWB / Piece
pub const AWB_ISSUED:                &str = "logisticos.order.awb.issued";
pub const PIECE_SCANNED:             &str = "logisticos.hub.piece.scanned";
pub const WEIGHT_DISCREPANCY_FOUND:  &str = "logisticos.hub.piece.weight_discrepancy";

// Pallet / Container
pub const PALLET_SEALED:             &str = "logisticos.hub.pallet.sealed";
pub const CONTAINER_DEPARTED:        &str = "logisticos.fleet.container.departed";
pub const CONTAINER_ARRIVED:         &str = "logisticos.fleet.container.arrived";

// Cross-border hub transfer (hub-ops emits)
pub const HUB_PIECE_SCANNED_INBOUND:     &str = "logisticos.hub.piece.scanned_inbound";
pub const CONTAINER_ARRIVED_AT_PORT:     &str = "logisticos.hub.container.arrived_at_port";
pub const CONTAINER_CUSTOMS_HOLD:        &str = "logisticos.hub.container.customs_hold";
pub const CONTAINER_CUSTOMS_CLEARED:     &str = "logisticos.hub.container.customs_cleared";
pub const CONTAINER_RELEASED_DOMESTIC:   &str = "logisticos.hub.container.released_domestic";
pub const CONTAINER_DECONSOLIDATED:      &str = "logisticos.hub.container.deconsolidated";
pub const HUB_DISPATCH_REQUESTED:        &str = "logisticos.hub.shipment.dispatch_requested";
pub const HUB_CARRIER_BOOKING_REQUESTED: &str = "logisticos.hub.shipment.carrier_booking_requested";

// Consolidation (hub-ops emits)
/// All pieces scanned and container sealed by the 3D load-planning flow.
/// Payload: `{ "plan_id": uuid, "container_id": uuid, "master_awbs": [str] }`.
/// order-intake subscribes to flip qualifying shipments → `at_hub`.
pub const CONSOLIDATION_PLAN_LOADED:     &str = "logisticos.hub.consolidation.plan_loaded";

// Invoice / Billing
pub const WEIGHT_ADJUSTMENT_INVOICED: &str = "logisticos.payments.invoice.weight_adjustment";

// Dispatch
pub const ROUTE_CREATED:             &str = "logisticos.dispatch.route.created";
pub const DRIVER_ASSIGNED:           &str = "logisticos.dispatch.driver.assigned";
pub const ROUTE_OPTIMIZED:           &str = "logisticos.dispatch.route.optimized";
pub const ASSIGNMENT_REJECTED:       &str = "logisticos.dispatch.assignment.rejected";
pub const TASK_OFFER_CREATED:        &str = "logisticos.dispatch.offer.created";
pub const TASK_OFFER_CLOSED:         &str = "logisticos.dispatch.offer.closed";

// Driver / Field
pub const DRIVER_AVAILABLE:          &str = "logisticos.driver.available";
pub const PICKUP_COMPLETED:          &str = "logisticos.driver.pickup.completed";
pub const DELIVERY_ATTEMPTED:        &str = "logisticos.driver.delivery.attempted";
pub const DELIVERY_COMPLETED:        &str = "logisticos.driver.delivery.completed";
pub const DELIVERY_FAILED:           &str = "logisticos.driver.delivery.failed";
pub const LOCATION_UPDATED:          &str = "logisticos.driver.location.updated";
pub const DRIVER_LOCATION_UPDATED:   &str = "logisticos.driver.location.updated";

// POD / POP
pub const POD_CAPTURED:              &str = "logisticos.pod.captured";
/// Emitted when a driver submits a Proof of Pickup — opens the chain of custody.
pub const PICKUP_CAPTURED:           &str = "logisticos.pod.pickup.captured";

// Payments
pub const INVOICE_GENERATED:              &str = "logisticos.payments.invoice.generated";
pub const PAYMENT_RECEIVED:               &str = "logisticos.payments.payment.received";
pub const COD_COLLECTED:                  &str = "logisticos.payments.cod.collected";
pub const COD_REMITTED:                   &str = "logisticos.payments.cod.remitted";
pub const WALLET_WITHDRAWAL_DISBURSED:    &str = "logisticos.payments.wallet.withdrawal_disbursed";
pub const WALLET_WITHDRAWAL_REJECTED:     &str = "logisticos.payments.wallet.withdrawal_rejected";

// Engagement
pub const NOTIFICATION_QUEUED:       &str = "logisticos.engagement.notification.queued";
pub const CAMPAIGN_TRIGGERED:        &str = "logisticos.marketing.campaign.triggered";
pub const CAMPAIGN_COMPLETED:        &str = "logisticos.marketing.campaign.completed";
pub const CAMPAIGN_OPENED:           &str = "logisticos.engagement.campaign.opened";
pub const CAMPAIGN_CLICKED:          &str = "logisticos.engagement.campaign.clicked";
pub const CUSTOMER_SEGMENT_UPDATED:  &str = "logisticos.cdp.segment.updated";

// Tracking / customer-facing
pub const RECEIPT_EMAIL_REQUESTED:   &str = "logisticos.tracking.receipt.email.requested";

// Support tickets
pub const SUPPORT_TICKET_OPENED:     &str = "logisticos.support.ticket.opened";
pub const SUPPORT_TICKET_CLOSED:     &str = "logisticos.support.ticket.closed";

// AI agent escalations. Distinct from the SUPPORT_TICKET_* pair above, which
// drives campaign suppression in engagement — this one carries the operator's
// written resolution back to the customer who was chatting with the agent.
pub const AGENT_ESCALATION_RESOLVED: &str = "logisticos.ai.escalation.resolved";

// Inbound channel messages (customer → platform)
pub const WHATSAPP_INBOUND:          &str = "logisticos.engagement.whatsapp.inbound";

// ── OmniDeliv (hyperlocal delivery product) ─────────────────────────────────
//
// Namespaced under `omnideliv.` rather than `logisticos.` because it is a
// product tier, not the platform (ADR-0009). Engagement consumes these the same
// way it consumes logistics events — a product's customers are still customers.
pub const OMNIDELIV_ORDER_PLACED:    &str = "omnideliv.order.placed";
pub const OMNIDELIV_ORDER_DELIVERED: &str = "omnideliv.order.delivered";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cross_border_hub_topics_namespaced() {
        assert_eq!(HUB_PIECE_SCANNED_INBOUND,     "logisticos.hub.piece.scanned_inbound");
        assert_eq!(CONTAINER_ARRIVED_AT_PORT,     "logisticos.hub.container.arrived_at_port");
        assert_eq!(CONTAINER_CUSTOMS_HOLD,        "logisticos.hub.container.customs_hold");
        assert_eq!(CONTAINER_CUSTOMS_CLEARED,     "logisticos.hub.container.customs_cleared");
        assert_eq!(CONTAINER_RELEASED_DOMESTIC,   "logisticos.hub.container.released_domestic");
        assert_eq!(CONTAINER_DECONSOLIDATED,      "logisticos.hub.container.deconsolidated");
        assert_eq!(HUB_DISPATCH_REQUESTED,        "logisticos.hub.shipment.dispatch_requested");
        assert_eq!(HUB_CARRIER_BOOKING_REQUESTED, "logisticos.hub.shipment.carrier_booking_requested");
        assert_eq!(CONSOLIDATION_PLAN_LOADED,     "logisticos.hub.consolidation.plan_loaded");
    }

    #[test]
    fn all_topics_are_lowercase_dot_separated() {
        let topics: &[&str] = &[
            CARRIER_ONBOARDED, CARRIER_STATUS_CHANGED, CARRIER_ALLOCATED, CARRIER_TRACKING_EVENT,
            MARKETPLACE_BOOKING_ACCEPTED, MARKETPLACE_BOOKING_REJECTED, MARKETPLACE_PICKUP_RECORDED,
            TENANT_CREATED, TENANT_FINALIZED, USER_CREATED, USER_INVITED, OTP_REQUESTED,
            SHIPMENT_CREATED, SHIPMENT_CONFIRMED, SHIPMENT_CANCELLED, SHIPMENT_RESCHEDULED,
            AWB_ISSUED, PIECE_SCANNED, WEIGHT_DISCREPANCY_FOUND,
            PALLET_SEALED, CONTAINER_DEPARTED, CONTAINER_ARRIVED,
            HUB_PIECE_SCANNED_INBOUND, CONTAINER_ARRIVED_AT_PORT, CONTAINER_CUSTOMS_HOLD,
            CONTAINER_CUSTOMS_CLEARED, CONTAINER_RELEASED_DOMESTIC, CONTAINER_DECONSOLIDATED,
            HUB_DISPATCH_REQUESTED, HUB_CARRIER_BOOKING_REQUESTED,
            CONSOLIDATION_PLAN_LOADED,
            ROUTE_CREATED, DRIVER_ASSIGNED, ROUTE_OPTIMIZED,
            DRIVER_AVAILABLE,
            PICKUP_COMPLETED, DELIVERY_ATTEMPTED, DELIVERY_COMPLETED, DELIVERY_FAILED,
            LOCATION_UPDATED, DRIVER_LOCATION_UPDATED,
            POD_CAPTURED, PICKUP_CAPTURED,
            INVOICE_GENERATED, PAYMENT_RECEIVED,
            COD_COLLECTED, WEIGHT_ADJUSTMENT_INVOICED,
            WALLET_WITHDRAWAL_DISBURSED, WALLET_WITHDRAWAL_REJECTED,
            NOTIFICATION_QUEUED, CAMPAIGN_TRIGGERED, CAMPAIGN_COMPLETED, CUSTOMER_SEGMENT_UPDATED,
            TASK_ASSIGNED,
            RECEIPT_EMAIL_REQUESTED,
            SUPPORT_TICKET_OPENED, SUPPORT_TICKET_CLOSED, AGENT_ESCALATION_RESOLVED,
            WHATSAPP_INBOUND,
        ];
        for t in topics {
            assert!(t.chars().all(|c: char| c.is_ascii_lowercase() || c == '.' || c == '_'),
                "Topic '{}' has invalid chars", t);
            assert!(t.starts_with("logisticos."), "Topic '{}' must start with logisticos.", t);
        }
    }
}
