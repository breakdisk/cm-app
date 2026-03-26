// Domain events emitted by the order-intake bounded context.
// These are published to Kafka and consumed by other services.
// The event payloads are defined in logisticos-events (shared library).

pub use logisticos_events::payloads::ShipmentCreated;
pub use logisticos_events::topics;
