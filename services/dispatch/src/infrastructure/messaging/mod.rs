// Kafka consumer for dispatch.
// Subscribes to:
//   - shipment.created  → auto-batch shipments into route candidates
//   - driver.location_updated → update driver position cache for proximity scoring
pub use logisticos_events::consumer::KafkaConsumer;

pub mod compliance_consumer;
pub mod shipment_consumer;
pub mod user_consumer;

pub use shipment_consumer::start_shipment_consumer;
pub use user_consumer::start_user_consumer;
