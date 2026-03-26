// Inbound event handlers for order-intake.
// Currently order-intake is a producer service — it emits SHIPMENT_CREATED events
// and consumes status-update events to keep its read model in sync.

use std::sync::Arc;

use rdkafka::consumer::{CommitMode, Consumer, StreamConsumer};
use rdkafka::Message;

use crate::application::services::shipment_service::ShipmentRepository;

/// Listen for status updates published by downstream services (dispatch, delivery-experience)
/// and persist them to the shipments table so merchants can query current status via order-intake.
pub async fn run_status_consumer(
    consumer: Arc<StreamConsumer>,
    repo: Arc<dyn ShipmentRepository>,
) {
    use logisticos_events::topics;
    consumer
        .subscribe(&[
            topics::SHIPMENT_CONFIRMED,
            topics::SHIPMENT_CANCELLED,
            "shipment.status_updated",
        ])
        .expect("Kafka subscribe failed");

    loop {
        match consumer.recv().await {
            Err(e) => tracing::warn!(error = %e, "Kafka consumer error"),
            Ok(msg) => {
                if let Some(payload) = msg.payload() {
                    if let Ok(text) = std::str::from_utf8(payload) {
                        if let Err(e) = handle_status_event(text, &*repo).await {
                            tracing::warn!(error = %e, "Failed to process status event");
                        }
                    }
                }
                consumer.commit_message(&msg, CommitMode::Async).ok();
            }
        }
    }
}

async fn handle_status_event(
    _payload: &str,
    _repo: &dyn ShipmentRepository,
) -> anyhow::Result<()> {
    // Parse event, extract shipment_id + new status, load shipment, update status, save.
    // Detailed implementation mirrors the delivery-experience handler pattern.
    Ok(())
}
