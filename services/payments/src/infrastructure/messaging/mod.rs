// Kafka consumers for payments service.
// Subscribes to: pod.captured (triggers COD reconciliation), delivery.completed,
//                hub.weight_discrepancy_found (triggers weight surcharge adjustment).
pub use logisticos_events::consumer::KafkaConsumer;
pub mod pod_consumer;
pub use pod_consumer::PodConsumer;
pub mod weight_discrepancy_consumer;
pub use weight_discrepancy_consumer::WeightDiscrepancyConsumer;
