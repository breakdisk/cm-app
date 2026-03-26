// Kafka consumer for dispatch.
// Subscribes to:
//   - shipment.created  → auto-batch shipments into route candidates
//   - driver.location_updated → update driver position cache for proximity scoring
pub use logisticos_events::consumer::KafkaConsumer;

pub mod compliance_consumer;
