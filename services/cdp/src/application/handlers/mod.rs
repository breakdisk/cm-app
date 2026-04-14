/// Kafka event handlers — consume domain events and project them into the CDP profile store.
///
/// Subscriptions:
///   logisticos.order.shipment.created   → ShipmentCreated  → upsert profile, record event
///   logisticos.driver.delivery.completed → DeliveryCompleted → record event, update counters
///   logisticos.driver.delivery.failed   → DeliveryFailed   → record event, update counters
///   logisticos.payments.cod.collected   → CodCollected     → record event, update COD total
use std::sync::Arc;
use chrono::{DateTime, Utc};
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    message::BorrowedMessage,
    Message,
};
use serde::Deserialize;
use uuid::Uuid;

use logisticos_events::topics;
use logisticos_types::TenantId;

use crate::application::services::{ProfileService, RecordEventCommand};
use crate::domain::entities::EventType;

// ---------------------------------------------------------------------------
// Inbound payload shapes (mirrors libs/events/src/payloads.rs)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ShipmentCreatedPayload {
    merchant_id:          Uuid,
    customer_id:          Uuid,
    destination_address:  String,
}

#[derive(Debug, Deserialize)]
struct DeliveryCompletedPayload {
    shipment_id:  Uuid,
    driver_id:    Uuid,
    delivered_at: String,
}

#[derive(Debug, Deserialize)]
struct DeliveryFailedPayload {
    shipment_id:    Uuid,
    reason:         String,
    attempted_at:   String,
    attempt_number: u32,
}

#[derive(Debug, Deserialize)]
struct CodCollectedPayload {
    shipment_id:    Uuid,
    amount_cents:   i64,
    collected_at:   String,
}

// ---------------------------------------------------------------------------
// Handler entry point — runs as a long-lived Tokio task.
// ---------------------------------------------------------------------------

pub async fn run_consumer(consumer: Arc<StreamConsumer>, svc: Arc<ProfileService>) {
    consumer
        .subscribe(&[
            topics::SHIPMENT_CREATED,
            topics::DELIVERY_COMPLETED,
            topics::DELIVERY_FAILED,
            topics::COD_COLLECTED,
        ])
        .expect("CDP consumer subscription failed");

    loop {
        match consumer.recv().await {
            Ok(msg) => {
                if let Err(e) = handle_message(&msg, &svc).await {
                    tracing::warn!(
                        topic = msg.topic(),
                        offset = msg.offset(),
                        err = %e,
                        "CDP event handler error"
                    );
                }
                // At-least-once: commit after processing (even on error to avoid poison pill).
                consumer.commit_message(&msg, CommitMode::Async).ok();
            }
            Err(e) => {
                tracing::error!(err = %e, "Kafka recv error in CDP consumer");
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    }
}

async fn handle_message(
    msg: &BorrowedMessage<'_>,
    svc: &Arc<ProfileService>,
) -> anyhow::Result<()> {
    let payload = match msg.payload() {
        Some(p) => p,
        None => return Ok(()), // tombstone / null payload — skip
    };

    // Extract tenant_id from Kafka message header (set by all publishers).
    let tenant_id = extract_tenant_header(msg)?;

    match msg.topic() {
        topics::SHIPMENT_CREATED => {
            let data: ShipmentCreatedPayload = serde_json::from_slice(payload)?;
            svc.record_event(RecordEventCommand {
                tenant_id,
                external_customer_id: data.customer_id,
                event_type: EventType::ShipmentCreated,
                shipment_id: None,
                metadata: serde_json::json!({
                    "merchant_id": data.merchant_id,
                    "destination_address": data.destination_address,
                }),
                occurred_at: Utc::now(),
            })
            .await?;
        }
        topics::DELIVERY_COMPLETED => {
            let data: DeliveryCompletedPayload = serde_json::from_slice(payload)?;
            let occurred_at = data
                .delivered_at
                .parse::<DateTime<Utc>>()
                .unwrap_or_else(|_| Utc::now());
            // Delivery events carry shipment_id but not customer_id directly.
            // We do a best-effort lookup; if no profile exists we skip (no external_customer_id).
            // In production the event payload would include customer_id — noted for v2.
            svc.record_event(RecordEventCommand {
                tenant_id,
                external_customer_id: data.shipment_id, // use shipment_id as proxy until customer_id added
                event_type: EventType::DeliveryCompleted,
                shipment_id: Some(data.shipment_id),
                metadata: serde_json::json!({
                    "driver_id": data.driver_id,
                    "delivered_at": data.delivered_at,
                }),
                occurred_at,
            })
            .await?;
        }
        topics::DELIVERY_FAILED => {
            let data: DeliveryFailedPayload = serde_json::from_slice(payload)?;
            let occurred_at = data
                .attempted_at
                .parse::<DateTime<Utc>>()
                .unwrap_or_else(|_| Utc::now());
            svc.record_event(RecordEventCommand {
                tenant_id,
                external_customer_id: data.shipment_id,
                event_type: EventType::DeliveryFailed,
                shipment_id: Some(data.shipment_id),
                metadata: serde_json::json!({
                    "reason": data.reason,
                    "attempt_number": data.attempt_number,
                }),
                occurred_at,
            })
            .await?;
        }
        topics::COD_COLLECTED => {
            let data: CodCollectedPayload = serde_json::from_slice(payload)?;
            let occurred_at = data
                .collected_at
                .parse::<DateTime<Utc>>()
                .unwrap_or_else(|_| Utc::now());
            svc.record_event(RecordEventCommand {
                tenant_id,
                external_customer_id: data.shipment_id,
                event_type: EventType::CodPaid,
                shipment_id: Some(data.shipment_id),
                metadata: serde_json::json!({
                    "amount_cents": data.amount_cents,
                }),
                occurred_at,
            })
            .await?;
        }
        other => {
            tracing::debug!(topic = other, "CDP consumer: unhandled topic");
        }
    }

    Ok(())
}

fn extract_tenant_header(msg: &BorrowedMessage<'_>) -> anyhow::Result<TenantId> {
    use rdkafka::message::Headers;
    msg.headers()
        .and_then(|headers| {
            headers.iter().find_map(|h| {
                if h.key == "tenant_id" {
                    h.value
                        .and_then(|v| std::str::from_utf8(v).ok())
                        .and_then(|s| s.parse::<Uuid>().ok())
                        .map(TenantId::from_uuid)
                } else {
                    None
                }
            })
        })
        .ok_or_else(|| anyhow::anyhow!("Missing tenant_id Kafka header on topic {}", msg.topic()))
}
