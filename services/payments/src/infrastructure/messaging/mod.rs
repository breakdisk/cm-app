// Kafka consumers for payments service.
// Subscribes to: pod.captured (triggers COD reconciliation), delivery.completed,
//                hub.weight_discrepancy_found (triggers weight surcharge adjustment),
//                pod.pickup.captured (Track A driver ledger debit).
pub use logisticos_events::consumer::KafkaConsumer;
pub mod pod_consumer;
pub use pod_consumer::PodConsumer;
pub mod weight_discrepancy_consumer;
pub use weight_discrepancy_consumer::WeightDiscrepancyConsumer;
pub mod pickup_consumer;
pub use pickup_consumer::PickupCapturedConsumer;
pub mod customs_duty_consumer;
pub use customs_duty_consumer::CustomsDutyConsumer;
