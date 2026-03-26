//! Kafka consumer for the identity service.
//! Currently subscribes to no topics — identity is a producer only.
//! Reserved for future cross-service events (e.g. subscription tier changes from payments).

pub use logisticos_events::consumer::KafkaConsumer;
