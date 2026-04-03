/// Kafka consumers — append shipment lifecycle events to the analytics event store.
///
/// Events consumed:
///   shipment.created    → ShipmentEvent{type: "created"}
///   delivery.completed  → ShipmentEvent{type: "delivered", delivery_hours, on_time}
///   delivery.failed     → ShipmentEvent{type: "failed"}
///   shipment.cancelled  → ShipmentEvent{type: "cancelled"}
///   cod.collected       → update cod_amount_cents on existing delivered event
use std::sync::Arc;
use chrono::{DateTime, Utc};
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    message::{BorrowedMessage, Headers},
    Message,
};
use serde::Deserialize;
use uuid::Uuid;

use logisticos_events::topics;

use crate::domain::entities::ShipmentEvent;
use crate::infrastructure::db::AnalyticsDb;

#[derive(Debug, Deserialize)]
struct ShipmentCreated {
    shipment_id:  Uuid,
    merchant_id:  Uuid,
    service_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeliveryCompleted {
    shipment_id:         Uuid,
    driver_id:           Uuid,
    delivered_at:        String,
    /// Present if ETA was set and delivery was on time.
    on_time:             Option<bool>,
    delivery_hours:      Option<f64>,
}

#[derive(Debug, Deserialize)]
struct DeliveryFailed {
    shipment_id: Uuid,
    driver_id:   Uuid,
}

#[derive(Debug, Deserialize)]
struct ShipmentCancelled {
    shipment_id: Uuid,
}

#[derive(Debug, Deserialize)]
struct CodCollected {
    shipment_id:  Uuid,
    amount_cents: i64,
}

pub async fn run_consumer(consumer: Arc<StreamConsumer>, db: Arc<AnalyticsDb>) {
    consumer
        .subscribe(&[
            topics::SHIPMENT_CREATED,
            topics::DELIVERY_COMPLETED,
            topics::DELIVERY_FAILED,
            topics::SHIPMENT_CANCELLED,
            topics::COD_COLLECTED,
        ])
        .expect("Analytics consumer subscription failed");

    loop {
        match consumer.recv().await {
            Ok(msg) => {
                if let Err(e) = handle_message(&msg, &db).await {
                    tracing::warn!(
                        topic = msg.topic(),
                        offset = msg.offset(),
                        err = %e,
                        "Analytics event handler error"
                    );
                }
                consumer.commit_message(&msg, CommitMode::Async).ok();
            }
            Err(e) => {
                tracing::error!(err = %e, "Analytics Kafka recv error");
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    }
}

async fn handle_message(msg: &BorrowedMessage<'_>, db: &Arc<AnalyticsDb>) -> anyhow::Result<()> {
    let payload = match msg.payload() { Some(p) => p, None => return Ok(()) };

    let tenant_id = extract_tenant_id(msg)?;

    match msg.topic() {
        topics::SHIPMENT_CREATED => {
            let data: ShipmentCreated = serde_json::from_slice(payload)?;
            db.insert_event(&ShipmentEvent {
                id:               Uuid::new_v4(),
                tenant_id,
                shipment_id:      data.shipment_id,
                event_type:       "created".into(),
                driver_id:        None,
                service_type:     data.service_type,
                cod_amount_cents: None,
                on_time:          None,
                delivery_hours:   None,
                occurred_at:      Utc::now(),
            })
            .await?;
        }
        topics::DELIVERY_COMPLETED => {
            let data: DeliveryCompleted = serde_json::from_slice(payload)?;
            let occurred_at = data.delivered_at.parse::<DateTime<Utc>>().unwrap_or_else(|_| Utc::now());
            db.insert_event(&ShipmentEvent {
                id:               Uuid::new_v4(),
                tenant_id,
                shipment_id:      data.shipment_id,
                event_type:       "delivered".into(),
                driver_id:        Some(data.driver_id),
                service_type:     None,
                cod_amount_cents: None,
                on_time:          data.on_time,
                delivery_hours:   data.delivery_hours,
                occurred_at,
            })
            .await?;
        }
        topics::DELIVERY_FAILED => {
            let data: DeliveryFailed = serde_json::from_slice(payload)?;
            db.insert_event(&ShipmentEvent {
                id:               Uuid::new_v4(),
                tenant_id,
                shipment_id:      data.shipment_id,
                event_type:       "failed".into(),
                driver_id:        Some(data.driver_id),
                service_type:     None,
                cod_amount_cents: None,
                on_time:          Some(false),
                delivery_hours:   None,
                occurred_at:      Utc::now(),
            })
            .await?;
        }
        topics::SHIPMENT_CANCELLED => {
            let data: ShipmentCancelled = serde_json::from_slice(payload)?;
            db.insert_event(&ShipmentEvent {
                id:               Uuid::new_v4(),
                tenant_id,
                shipment_id:      data.shipment_id,
                event_type:       "cancelled".into(),
                driver_id:        None,
                service_type:     None,
                cod_amount_cents: None,
                on_time:          None,
                delivery_hours:   None,
                occurred_at:      Utc::now(),
            })
            .await?;
        }
        topics::COD_COLLECTED => {
            let data: CodCollected = serde_json::from_slice(payload)?;
            // Update the COD amount on the most recent delivered event for this shipment.
            db.update_cod_amount(data.shipment_id, data.amount_cents).await?;
        }
        _ => {}
    }

    Ok(())
}

fn extract_tenant_id(msg: &BorrowedMessage<'_>) -> anyhow::Result<Uuid> {
    msg.headers()
        .and_then(|h| {
            h.iter().find_map(|header| {
                if header.key == "tenant_id" {
                    header.value
                        .and_then(|v| std::str::from_utf8(v).ok())
                        .and_then(|s: &str| s.parse::<Uuid>().ok())
                } else {
                    None
                }
            })
        })
        .ok_or_else(|| anyhow::anyhow!("Missing tenant_id header on topic {}", msg.topic()))
}
