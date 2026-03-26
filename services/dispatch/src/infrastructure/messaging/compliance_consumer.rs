use std::sync::Arc;
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    config::ClientConfig,
    message::Message,
};
use tokio::sync::{watch, Mutex};
use crate::infrastructure::db::ComplianceCache;

#[derive(serde::Deserialize)]
struct StatusChangedPayload {
    entity_id:     uuid::Uuid,
    entity_type:   String,
    new_status:    String,
    is_assignable: bool,
}

#[derive(serde::Deserialize)]
struct ComplianceEvent {
    event_type: String,
    data:       StatusChangedPayload,
}

pub async fn start_compliance_consumer(
    brokers:  &str,
    group_id: &str,
    cache:    Arc<Mutex<ComplianceCache>>,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", group_id)
        .set("enable.auto.commit", "false")
        .create()?;

    consumer.subscribe(&["compliance"])?;

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow_and_update() {
                    tracing::info!("Compliance consumer shutting down");
                    break;
                }
            }
            result = consumer.recv() => {
                match result {
                    Err(e) => tracing::warn!("Compliance Kafka recv error: {e}"),
                    Ok(msg) => {
                        match msg.payload_view::<str>() {
                            None => {
                                tracing::warn!("Compliance event has empty payload — skipping");
                            }
                            Some(Err(e)) => {
                                tracing::warn!("Compliance event payload not UTF-8: {e}");
                            }
                            Some(Ok(payload)) => {
                                match serde_json::from_str::<ComplianceEvent>(payload) {
                                    Err(e) => {
                                        tracing::warn!("Compliance event deserialize error: {e}");
                                    }
                                    Ok(event) => {
                                        if event.event_type == "compliance.status_changed"
                                            && event.data.entity_type == "driver"
                                        {
                                            let mut cache = cache.lock().await;
                                            if let Err(e) = cache.set_status(
                                                event.data.entity_id,
                                                &event.data.new_status,
                                                event.data.is_assignable,
                                            ).await {
                                                tracing::error!(
                                                    entity_id = %event.data.entity_id,
                                                    "Failed to update compliance cache: {e}"
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if let Err(e) = consumer.commit_message(&msg, CommitMode::Async) {
                            tracing::error!("Failed to commit compliance Kafka message: {e}");
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
