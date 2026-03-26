// Kafka consumer for driver-ops.
// Subscribes to dispatch.driver.assigned to update driver.active_route_id when
// the dispatch service creates an assignment.
pub use logisticos_events::consumer::KafkaConsumer;
