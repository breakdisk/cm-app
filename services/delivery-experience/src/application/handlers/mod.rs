/// Kafka event consumers — project shipment lifecycle events into the tracking read model.
///
/// Topics consumed:
///   logisticos.order.shipment.created    → create TrackingRecord
///   logisticos.order.shipment.confirmed  → transition to Confirmed
///   logisticos.order.shipment.cancelled  → transition to Cancelled
///   logisticos.dispatch.driver.assigned  → assign driver, transition to AssignedToDriver
///   logisticos.driver.pickup.completed   → transition to PickedUp
///   logisticos.driver.delivery.completed → mark_delivered
///   logisticos.driver.delivery.failed    → mark_failed
///   logisticos.driver.location.updated   → update driver_position (no status transition)
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

use crate::domain::entities::{TrackingRecord, TrackingStatus};
use crate::domain::repositories::TrackingRepository;

// ---------------------------------------------------------------------------
// Inbound payload shapes
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ShipmentCreated {
    shipment_id:         Uuid,
    merchant_id:         Uuid,
    origin_address:      String,
    destination_address: String,
}

#[derive(Debug, Deserialize)]
struct ShipmentConfirmed {
    shipment_id: Uuid,
}

#[derive(Debug, Deserialize)]
struct ShipmentCancelled {
    shipment_id: Uuid,
    reason:      Option<String>,
}

#[derive(Debug, Deserialize)]
struct DriverAssigned {
    shipment_id:            Uuid,
    driver_id:              Uuid,
    estimated_pickup_time:  Option<String>,
}

#[derive(Debug, Deserialize)]
struct PickupCompleted {
    shipment_id: Uuid,
    driver_id:   Uuid,
}

#[derive(Debug, Deserialize)]
struct DeliveryCompleted {
    shipment_id:    Uuid,
    pod_id:         Uuid,
    driver_id:      Uuid,
    delivered_at:   String,
    recipient_name: String,
}

#[derive(Debug, Deserialize)]
struct DeliveryFailed {
    shipment_id:            Uuid,
    reason:                 String,
    attempted_at:           String,
    attempt_number:         u32,
    next_attempt_scheduled: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LocationUpdated {
    driver_id:  Uuid,
    lat:        f64,
    lng:        f64,
}

// ---------------------------------------------------------------------------
// Consumer entry point
// ---------------------------------------------------------------------------

pub async fn run_consumer(consumer: Arc<StreamConsumer>, repo: Arc<dyn TrackingRepository>) {
    consumer
        .subscribe(&[
            topics::SHIPMENT_CREATED,
            topics::SHIPMENT_CONFIRMED,
            topics::SHIPMENT_CANCELLED,
            topics::DRIVER_ASSIGNED,
            topics::PICKUP_COMPLETED,
            topics::DELIVERY_COMPLETED,
            topics::DELIVERY_FAILED,
            topics::LOCATION_UPDATED,
        ])
        .expect("Tracking consumer subscription failed");

    loop {
        match consumer.recv().await {
            Ok(msg) => {
                if let Err(e) = handle_message(&msg, &repo).await {
                    tracing::warn!(
                        topic = msg.topic(),
                        offset = msg.offset(),
                        err = %e,
                        "Tracking event handler error"
                    );
                }
                consumer.commit_message(&msg, CommitMode::Async).ok();
            }
            Err(e) => {
                tracing::error!(err = %e, "Kafka recv error in tracking consumer");
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    }
}

async fn handle_message(
    msg: &BorrowedMessage<'_>,
    repo: &Arc<dyn TrackingRepository>,
) -> anyhow::Result<()> {
    let payload = match msg.payload() {
        Some(p) => p,
        None => return Ok(()),
    };

    let tenant_id = extract_tenant_header(msg)?;

    match msg.topic() {
        topics::SHIPMENT_CREATED => {
            let data: ShipmentCreated = serde_json::from_slice(payload)?;
            // Generate a human-readable tracking number: LS-<last8 of shipment_id>.
            let tracking_number = format!(
                "LS-{}",
                data.shipment_id.to_string().replace('-', "")[..8].to_uppercase()
            );
            let record = TrackingRecord::new(
                data.shipment_id,
                tenant_id,
                tracking_number,
                data.origin_address,
                data.destination_address,
            );
            repo.save(&record).await?;
        }
        topics::SHIPMENT_CONFIRMED => {
            let data: ShipmentConfirmed = serde_json::from_slice(payload)?;
            let mut record = require_record(repo, data.shipment_id).await?;
            record.transition(TrackingStatus::Confirmed, "Shipment confirmed".into(), None);
            repo.save(&record).await?;
        }
        topics::SHIPMENT_CANCELLED => {
            let data: ShipmentCancelled = serde_json::from_slice(payload)?;
            let mut record = require_record(repo, data.shipment_id).await?;
            let reason = data.reason.unwrap_or_else(|| "Cancelled by merchant".into());
            record.transition(TrackingStatus::Cancelled, reason, None);
            repo.save(&record).await?;
        }
        topics::DRIVER_ASSIGNED => {
            let data: DriverAssigned = serde_json::from_slice(payload)?;
            let mut record = require_record(repo, data.shipment_id).await?;
            let eta = data
                .estimated_pickup_time
                .as_deref()
                .and_then(|s| s.parse::<DateTime<Utc>>().ok());
            // Driver name/phone not in this event; will be enriched later or from identity service.
            record.assign_driver(data.driver_id, "Your driver".into(), "".into(), eta);
            repo.save(&record).await?;
        }
        topics::PICKUP_COMPLETED => {
            let data: PickupCompleted = serde_json::from_slice(payload)?;
            let mut record = require_record(repo, data.shipment_id).await?;
            record.transition(TrackingStatus::PickedUp, "Package picked up by driver".into(), None);
            repo.save(&record).await?;
        }
        topics::DELIVERY_COMPLETED => {
            let data: DeliveryCompleted = serde_json::from_slice(payload)?;
            let mut record = require_record(repo, data.shipment_id).await?;
            let delivered_at = data
                .delivered_at
                .parse::<DateTime<Utc>>()
                .unwrap_or_else(|_| Utc::now());
            record.mark_delivered(data.pod_id, data.recipient_name, delivered_at);
            repo.save(&record).await?;
        }
        topics::DELIVERY_FAILED => {
            let data: DeliveryFailed = serde_json::from_slice(payload)?;
            let mut record = require_record(repo, data.shipment_id).await?;
            let next = data
                .next_attempt_scheduled
                .as_deref()
                .and_then(|s| s.parse::<DateTime<Utc>>().ok());
            record.mark_failed(data.reason, data.attempt_number, next);
            repo.save(&record).await?;
        }
        topics::LOCATION_UPDATED => {
            let data: LocationUpdated = serde_json::from_slice(payload)?;
            // Update all active shipments for this driver that are OutForDelivery / AssignedToDriver.
            // In practice: maintain a driver_id → shipment_id index; here we skip for brevity.
            // The WebSocket hub handles the real-time broadcast from driver-ops service directly.
            tracing::debug!(driver_id = %data.driver_id, "location update (no-op in tracking store)");
        }
        other => {
            tracing::debug!(topic = other, "Tracking consumer: unhandled topic");
        }
    }

    Ok(())
}

async fn require_record(
    repo: &Arc<dyn TrackingRepository>,
    shipment_id: Uuid,
) -> anyhow::Result<TrackingRecord> {
    repo.find_by_shipment_id(shipment_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("TrackingRecord not found for shipment {}", shipment_id))
}

fn extract_tenant_header(msg: &BorrowedMessage<'_>) -> anyhow::Result<TenantId> {
    msg.headers()
        .and_then(|headers| {
            (0..headers.count()).find_map(|i| {
                let h = headers.get(i);
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
        .ok_or_else(|| anyhow::anyhow!("Missing tenant_id header on topic {}", msg.topic()))
}
