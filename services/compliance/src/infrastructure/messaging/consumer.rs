use std::sync::Arc;
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    config::ClientConfig,
    message::Message,
};
use tokio::sync::watch;
use logisticos_events::envelope::Event;
use crate::domain::events::{TOPIC_DRIVER, DriverRegisteredPayload};
use crate::application::services::ComplianceService;

pub async fn start_driver_consumer(
    brokers: &str,
    group_id: &str,
    compliance_service: Arc<ComplianceService>,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", group_id)
        .set("enable.auto.commit", "false")
        .set("auto.offset.reset", "earliest")
        .create()?;

    consumer.subscribe(&[TOPIC_DRIVER])?;

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    tracing::info!("Compliance Kafka consumer shutting down");
                    break;
                }
            }
            result = consumer.recv() => {
                match result {
                    Err(e) => tracing::warn!("Kafka error: {e}"),
                    Ok(msg) => {
                        match msg.payload_view::<str>() {
                            None => tracing::warn!("Received Kafka message with no payload — skipping"),
                            Some(Err(e)) => tracing::warn!("Kafka message payload is not valid UTF-8: {e}"),
                            Some(Ok(payload)) => {
                                match serde_json::from_str::<Event<DriverRegisteredPayload>>(payload) {
                                    Err(e) => tracing::warn!("Failed to deserialize driver event: {e}"),
                                    Ok(event) => {
                                        if event.event_type == "driver.registered" {
                                            if let Err(e) = compliance_service
                                                .create_profile_for_driver(
                                                    event.tenant_id,
                                                    event.data.driver_id,
                                                    &event.data.jurisdiction,
                                                )
                                                .await
                                            {
                                                tracing::error!("Failed to create compliance profile: {e}");
                                            }
                                        } else {
                                            tracing::warn!("Unrecognised driver event type: {}", event.event_type);
                                        }
                                    }
                                }
                            }
                        }
                        if let Err(e) = consumer.commit_message(&msg, CommitMode::Async) {
                            tracing::error!("Failed to commit Kafka offset: {e}");
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
