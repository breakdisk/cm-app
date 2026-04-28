use std::sync::Arc;
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    config::ClientConfig,
    message::Message,
};
use tokio::sync::watch;
use logisticos_events::envelope::Event;
use crate::domain::events::{
    TOPIC_DRIVER, DriverRegisteredPayload,
    TOPIC_CARRIER, CarrierOnboardedPayload,
    TOPIC_FLEET, VehicleRegisteredPayload,
};
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

/// Subscribes to logisticos.carrier.onboarded; creates a compliance profile per carrier.
pub async fn start_carrier_consumer(
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

    consumer.subscribe(&[TOPIC_CARRIER])?;
    tracing::info!("Compliance carrier consumer subscribed to {}", TOPIC_CARRIER);

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    tracing::info!("Compliance carrier consumer shutting down");
                    break;
                }
            }
            result = consumer.recv() => {
                match result {
                    Err(e) => tracing::warn!("Kafka error: {e}"),
                    Ok(msg) => {
                        match msg.payload_view::<str>() {
                            None => tracing::warn!("Empty carrier Kafka payload — skipping"),
                            Some(Err(e)) => tracing::warn!("Non-UTF-8 carrier Kafka payload: {e}"),
                            Some(Ok(payload)) => {
                                match serde_json::from_str::<Event<CarrierOnboardedPayload>>(payload) {
                                    Err(e) => tracing::warn!("Failed to deserialize carrier event: {e}"),
                                    Ok(event) => {
                                        if event.event_type == "carrier.onboarded" {
                                            if let Err(e) = compliance_service
                                                .create_profile_for_carrier(
                                                    event.tenant_id,
                                                    event.data.carrier_id,
                                                )
                                                .await
                                            {
                                                tracing::error!("Failed to create carrier compliance profile: {e}");
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if let Err(e) = consumer.commit_message(&msg, CommitMode::Async) {
                            tracing::error!("Failed to commit carrier Kafka offset: {e}");
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// Subscribes to logisticos.fleet.vehicle.registered; creates a compliance profile per vehicle.
pub async fn start_vehicle_consumer(
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

    consumer.subscribe(&[TOPIC_FLEET])?;
    tracing::info!("Compliance vehicle consumer subscribed to {}", TOPIC_FLEET);

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    tracing::info!("Compliance vehicle consumer shutting down");
                    break;
                }
            }
            result = consumer.recv() => {
                match result {
                    Err(e) => tracing::warn!("Kafka error: {e}"),
                    Ok(msg) => {
                        match msg.payload_view::<str>() {
                            None => tracing::warn!("Empty vehicle Kafka payload — skipping"),
                            Some(Err(e)) => tracing::warn!("Non-UTF-8 vehicle Kafka payload: {e}"),
                            Some(Ok(payload)) => {
                                match serde_json::from_str::<Event<VehicleRegisteredPayload>>(payload) {
                                    Err(e) => tracing::warn!("Failed to deserialize vehicle event: {e}"),
                                    Ok(event) => {
                                        if event.event_type == "vehicle.registered" {
                                            if let Err(e) = compliance_service
                                                .create_profile_for_vehicle(
                                                    event.tenant_id,
                                                    event.data.vehicle_id,
                                                    &event.data.jurisdiction,
                                                )
                                                .await
                                            {
                                                tracing::error!("Failed to create vehicle compliance profile: {e}");
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if let Err(e) = consumer.commit_message(&msg, CommitMode::Async) {
                            tracing::error!("Failed to commit vehicle Kafka offset: {e}");
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
