// Redis cache for dispatch: real-time driver location cache and route state snapshots.
// Driver locations are cached here after being consumed from Kafka `driver.location_updated` events,
// so the spatial query in PgDriverAvailabilityRepository falls back to the cache on DB lag.
pub use logisticos_events::consumer::KafkaConsumer;
