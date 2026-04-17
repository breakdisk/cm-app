// Identity
pub const TENANT_CREATED:            &str = "logisticos.identity.tenant.created";
pub const USER_INVITED:              &str = "logisticos.identity.user.invited";
pub const USER_CREATED:              &str = "logisticos.identity.user.created";

// Task
pub const TASK_ASSIGNED:             &str = "logisticos.task.assigned";

// Order / Shipment
pub const SHIPMENT_CREATED:          &str = "logisticos.order.shipment.created";
pub const SHIPMENT_CONFIRMED:        &str = "logisticos.order.shipment.confirmed";
pub const SHIPMENT_CANCELLED:        &str = "logisticos.order.shipment.cancelled";

// AWB / Piece
pub const AWB_ISSUED:                &str = "logisticos.order.awb.issued";
pub const PIECE_SCANNED:             &str = "logisticos.hub.piece.scanned";
pub const WEIGHT_DISCREPANCY_FOUND:  &str = "logisticos.hub.piece.weight_discrepancy";

// Pallet / Container
pub const PALLET_SEALED:             &str = "logisticos.hub.pallet.sealed";
pub const CONTAINER_DEPARTED:        &str = "logisticos.fleet.container.departed";
pub const CONTAINER_ARRIVED:         &str = "logisticos.fleet.container.arrived";

// Invoice / Billing
pub const INVOICE_FINALIZED:         &str = "logisticos.payments.invoice.finalized";
pub const COD_REMITTANCE_READY:      &str = "logisticos.payments.cod.remittance_ready";
pub const WEIGHT_ADJUSTMENT_INVOICED: &str = "logisticos.payments.invoice.weight_adjustment";

// Dispatch
pub const ROUTE_CREATED:             &str = "logisticos.dispatch.route.created";
pub const DRIVER_ASSIGNED:           &str = "logisticos.dispatch.driver.assigned";
pub const ROUTE_OPTIMIZED:           &str = "logisticos.dispatch.route.optimized";

// Driver / Field
pub const PICKUP_COMPLETED:          &str = "logisticos.driver.pickup.completed";
pub const DELIVERY_ATTEMPTED:        &str = "logisticos.driver.delivery.attempted";
pub const DELIVERY_COMPLETED:        &str = "logisticos.driver.delivery.completed";
pub const DELIVERY_FAILED:           &str = "logisticos.driver.delivery.failed";
pub const LOCATION_UPDATED:          &str = "logisticos.driver.location.updated";
pub const DRIVER_LOCATION_UPDATED:   &str = "logisticos.driver.location.updated";

// POD
pub const POD_CAPTURED:              &str = "logisticos.pod.captured";

// Payments
pub const INVOICE_GENERATED:         &str = "logisticos.payments.invoice.generated";
pub const PAYMENT_RECEIVED:          &str = "logisticos.payments.payment.received";
pub const COD_COLLECTED:             &str = "logisticos.payments.cod.collected";
pub const COD_REMITTED:              &str = "logisticos.payments.cod.remitted";

// Engagement
pub const NOTIFICATION_QUEUED:       &str = "logisticos.engagement.notification.queued";
pub const CAMPAIGN_TRIGGERED:        &str = "logisticos.marketing.campaign.triggered";
pub const CUSTOMER_SEGMENT_UPDATED:  &str = "logisticos.cdp.segment.updated";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_topics_are_lowercase_dot_separated() {
        let topics: &[&str] = &[
            TENANT_CREATED, USER_CREATED, USER_INVITED,
            SHIPMENT_CREATED, SHIPMENT_CONFIRMED, SHIPMENT_CANCELLED,
            AWB_ISSUED, PIECE_SCANNED, WEIGHT_DISCREPANCY_FOUND,
            PALLET_SEALED, CONTAINER_DEPARTED, CONTAINER_ARRIVED,
            ROUTE_CREATED, DRIVER_ASSIGNED, ROUTE_OPTIMIZED,
            PICKUP_COMPLETED, DELIVERY_ATTEMPTED, DELIVERY_COMPLETED, DELIVERY_FAILED,
            LOCATION_UPDATED, DRIVER_LOCATION_UPDATED,
            POD_CAPTURED,
            INVOICE_GENERATED, INVOICE_FINALIZED, PAYMENT_RECEIVED,
            COD_COLLECTED, COD_REMITTANCE_READY, WEIGHT_ADJUSTMENT_INVOICED,
            NOTIFICATION_QUEUED, CAMPAIGN_TRIGGERED, CUSTOMER_SEGMENT_UPDATED,
            TASK_ASSIGNED,
        ];
        for t in topics {
            assert!(t.chars().all(|c: char| c.is_ascii_lowercase() || c == '.' || c == '_'),
                "Topic '{}' has invalid chars", t);
            assert!(t.starts_with("logisticos."), "Topic '{}' must start with logisticos.", t);
        }
    }
}
