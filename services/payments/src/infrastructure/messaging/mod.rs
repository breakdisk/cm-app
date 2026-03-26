// Kafka consumer for payments.
// Subscribes to: pod.captured (triggers COD reconciliation), delivery.completed.
pub use logisticos_events::consumer::KafkaConsumer;
