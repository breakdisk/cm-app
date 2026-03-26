// Identity
pub const TENANT_CREATED:            &str = "logisticos.identity.tenant.created";
pub const USER_INVITED:              &str = "logisticos.identity.user.invited";

// Order / Shipment
pub const SHIPMENT_CREATED:          &str = "logisticos.order.shipment.created";
pub const SHIPMENT_CONFIRMED:        &str = "logisticos.order.shipment.confirmed";
pub const SHIPMENT_CANCELLED:        &str = "logisticos.order.shipment.cancelled";

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

// Engagement
pub const NOTIFICATION_QUEUED:       &str = "logisticos.engagement.notification.queued";
pub const CAMPAIGN_TRIGGERED:        &str = "logisticos.marketing.campaign.triggered";
pub const CUSTOMER_SEGMENT_UPDATED:  &str = "logisticos.cdp.segment.updated";
