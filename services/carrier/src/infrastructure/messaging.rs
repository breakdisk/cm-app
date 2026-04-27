use std::sync::Arc;

use chrono::DateTime;
use logisticos_events::{
    envelope::Event,
    payloads::{CarrierAllocated, CarrierOnboarded, CarrierStatusChanged, DeliveryCompleted, DeliveryFailed},
    producer::KafkaProducer,
    topics::{CARRIER_ALLOCATED, CARRIER_ONBOARDED, CARRIER_STATUS_CHANGED, DELIVERY_COMPLETED, DELIVERY_FAILED},
};
use rdkafka::{
    config::ClientConfig,
    consumer::{CommitMode, Consumer, StreamConsumer},
    message::Message,
};
use tokio::sync::watch;
use uuid::Uuid;

use crate::{
    domain::{entities::CarrierId, repositories::{CarrierRepository, SlaRecordRepository}},
    infrastructure::db::PgCarrierRepository,
};

// ── Publisher ─────────────────────────────────────────────────────────────────

pub struct CarrierPublisher {
    kafka: Arc<KafkaProducer>,
}

impl CarrierPublisher {
    pub fn new(kafka: Arc<KafkaProducer>) -> Self { Self { kafka } }

    pub async fn carrier_onboarded(
        &self,
        carrier_id: Uuid,
        tenant_id: Uuid,
        name: String,
        code: String,
        contact_email: String,
    ) -> anyhow::Result<()> {
        let payload = CarrierOnboarded { carrier_id, tenant_id, name, code, contact_email };
        let event = Event::new("logisticos/carrier", "carrier.onboarded", tenant_id, payload);
        self.kafka.publish_event(CARRIER_ONBOARDED, &event).await
    }

    pub async fn carrier_status_changed(
        &self,
        carrier_id: Uuid,
        tenant_id: Uuid,
        old_status: String,
        new_status: String,
        reason: String,
    ) -> anyhow::Result<()> {
        let payload = CarrierStatusChanged { carrier_id, tenant_id, old_status, new_status, reason };
        let event = Event::new("logisticos/carrier", "carrier.status_changed", tenant_id, payload);
        self.kafka.publish_event(CARRIER_STATUS_CHANGED, &event).await
    }

    pub async fn carrier_allocated(
        &self,
        tenant_id: Uuid,
        payload: CarrierAllocated,
    ) -> anyhow::Result<()> {
        let event = Event::new("logisticos/carrier", "carrier.allocated", tenant_id, payload);
        self.kafka.publish_event(CARRIER_ALLOCATED, &event).await
    }
}

// ── Delivery outcome consumer ─────────────────────────────────────────────────

/// Subscribes to `delivery.completed` and `delivery.failed` driver events.
/// On each message:
///   1. Looks up the SLA record by shipment_id to resolve the carrier.
///   2. Updates the SLA record (mark_delivered / mark_failed + save_outcome).
///   3. Calls carrier.record_delivery(on_time) and saves the updated carrier.
pub async fn start_delivery_consumer(
    brokers: &str,
    group_id: &str,
    carrier_repo: Arc<PgCarrierRepository>,
    sla_repo: Arc<dyn SlaRecordRepository>,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", group_id)
        .set("enable.auto.commit", "false")
        .set("auto.offset.reset", "earliest")
        .create()?;

    consumer.subscribe(&[DELIVERY_COMPLETED, DELIVERY_FAILED])?;
    tracing::info!("Carrier delivery consumer subscribed to {} / {}", DELIVERY_COMPLETED, DELIVERY_FAILED);

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    tracing::info!("Carrier delivery consumer shutting down");
                    break;
                }
            }
            result = consumer.recv() => {
                match result {
                    Err(e) => tracing::warn!("Kafka recv error: {e}"),
                    Ok(msg) => {
                        let topic = msg.topic();
                        match msg.payload_view::<str>() {
                            None => tracing::warn!(topic, "Empty Kafka payload — skipping"),
                            Some(Err(e)) => tracing::warn!(topic, "Non-UTF-8 payload: {e}"),
                            Some(Ok(raw)) => {
                                if topic == DELIVERY_COMPLETED {
                                    if let Err(e) = handle_delivery_completed(
                                        raw, &*carrier_repo, &*sla_repo,
                                    ).await {
                                        tracing::error!("handle_delivery_completed error: {e}");
                                    }
                                } else if topic == DELIVERY_FAILED {
                                    if let Err(e) = handle_delivery_failed(
                                        raw, &*carrier_repo, &*sla_repo,
                                    ).await {
                                        tracing::error!("handle_delivery_failed error: {e}");
                                    }
                                }
                            }
                        }
                        if let Err(e) = consumer.commit_message(&msg, CommitMode::Async) {
                            tracing::error!("Kafka commit error: {e}");
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

async fn handle_delivery_completed(
    raw: &str,
    carrier_repo: &dyn CarrierRepository,
    sla_repo: &dyn SlaRecordRepository,
) -> anyhow::Result<()> {
    let event: Event<DeliveryCompleted> = serde_json::from_str(raw)?;
    let payload = &event.data;

    let delivered_at = DateTime::parse_from_rfc3339(&payload.delivered_at)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::Utc::now());

    let Some(mut sla) = sla_repo.find_by_shipment(payload.shipment_id).await? else {
        tracing::warn!(shipment_id = %payload.shipment_id, "No SLA record for delivered shipment — skipping");
        return Ok(());
    };
    sla.mark_delivered(delivered_at);
    sla_repo.save_outcome(&sla).await?;

    let on_time = sla.on_time.unwrap_or(false);
    update_carrier_metrics(carrier_repo, sla.carrier_id, on_time).await
}

async fn handle_delivery_failed(
    raw: &str,
    carrier_repo: &dyn CarrierRepository,
    sla_repo: &dyn SlaRecordRepository,
) -> anyhow::Result<()> {
    let event: Event<DeliveryFailed> = serde_json::from_str(raw)?;
    let payload = &event.data;

    // Only count as a final failure on the last attempt or explicit failure (no next attempt).
    if payload.next_attempt_scheduled.is_some() {
        tracing::debug!(shipment_id = %payload.shipment_id, "Delivery failed but has next attempt — deferring SLA verdict");
        return Ok(());
    }

    let Some(mut sla) = sla_repo.find_by_shipment(payload.shipment_id).await? else {
        tracing::warn!(shipment_id = %payload.shipment_id, "No SLA record for failed shipment — skipping");
        return Ok(());
    };
    sla.mark_failed(&payload.reason);
    sla_repo.save_outcome(&sla).await?;

    update_carrier_metrics(carrier_repo, sla.carrier_id, false).await
}

async fn update_carrier_metrics(
    carrier_repo: &dyn CarrierRepository,
    carrier_id: Uuid,
    on_time: bool,
) -> anyhow::Result<()> {
    let Some(mut carrier) = carrier_repo
        .find_by_id(&CarrierId::from_uuid(carrier_id))
        .await?
    else {
        tracing::warn!(carrier_id = %carrier_id, "Carrier not found when updating delivery metrics");
        return Ok(());
    };
    carrier.record_delivery(on_time);
    carrier_repo.save(&carrier).await?;
    tracing::debug!(
        carrier_id = %carrier_id,
        on_time,
        total_shipments = carrier.total_shipments,
        grade = ?carrier.performance_grade,
        "Carrier performance updated"
    );
    Ok(())
}
