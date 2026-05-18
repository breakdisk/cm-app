// Kafka consumers for payments service.
// Subscribes to:
//   pod.captured              — COD reconciliation + Balikbayan billing
//   hub.piece.scanned         — StandardParcel hub-inbound freight invoice
//   hub.piece.weight_discrepancy — Weight surcharge adjustment invoice
pub use logisticos_events::consumer::KafkaConsumer;
pub mod pod_consumer;
pub use pod_consumer::PodConsumer;
pub mod hub_inbound_consumer;
pub use hub_inbound_consumer::HubInboundConsumer;
pub mod weight_discrepancy_consumer;
pub use weight_discrepancy_consumer::WeightDiscrepancyConsumer;
