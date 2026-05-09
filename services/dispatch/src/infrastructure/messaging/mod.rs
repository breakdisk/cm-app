// Kafka consumers for dispatch.
// Topics consumed:
//   - shipment.created       → insert into dispatch_queue, trigger auto-dispatch
//   - driver.available       → retry pending queue on driver coming online
//   - driver.delivery.failed → reset dispatched shipment to pending for reassignment
//   - compliance.status      → update Redis cache for driver compliance gate
//   - user.created           → cache driver profiles
pub use logisticos_events::consumer::KafkaConsumer;

pub mod compliance_consumer;
pub mod delivery_failed_consumer;
pub mod driver_available_consumer;
pub mod shipment_consumer;
pub mod user_consumer;

pub use delivery_failed_consumer::start_delivery_failed_consumer;
pub use driver_available_consumer::start_driver_available_consumer;
pub use shipment_consumer::start_shipment_consumer;
pub use user_consumer::start_user_consumer;
