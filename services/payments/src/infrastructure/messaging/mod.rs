// Kafka consumers for payments service.
// Subscribes to: pod.captured (triggers COD reconciliation), delivery.completed.
pub use logisticos_events::consumer::KafkaConsumer;
pub mod pod_consumer;
pub use pod_consumer::PodConsumer;
