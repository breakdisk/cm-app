/// Kafka event consumers — project shipment lifecycle events into the tracking read model.
///
/// Topics consumed:
///   logisticos.order.shipment.created    → create TrackingRecord (uses AWB from event)
///   logisticos.order.shipment.confirmed  → transition to Confirmed
///   logisticos.order.shipment.cancelled  → transition to Cancelled
///   logisticos.dispatch.driver.assigned  → assign driver, transition to AssignedToDriver
///   logisticos.driver.pickup.completed    → transition to PickedUp (driver-ops task completion)
///   logisticos.pod.pickup.captured        → transition to PickedUp (POP submission — authoritative)
///   logisticos.driver.delivery.completed → mark_delivered
///   logisticos.driver.delivery.failed    → mark_failed
///   logisticos.driver.location.updated   → update driver_position (no status transition)
///
/// All events arrive wrapped in Event<T> (CloudEvents envelope):
///   { "id":..., "source":..., "tenant_id":..., "time":..., "data": { ...payload... } }
/// tenant_id is extracted from the envelope root; the Kafka producer does NOT set message
/// headers, so we never call extract_tenant_header — we read from the JSON body.
use std::sync::Arc;
use chrono::{DateTime, Utc};
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    Message,
};
use serde::Deserialize;
use uuid::Uuid;

use logisticos_events::topics;
use logisticos_types::TenantId;

use crate::domain::entities::{TrackingRecord, TrackingStatus};
use crate::domain::repositories::TrackingRepository;

// ---------------------------------------------------------------------------
// Inbound payload shapes (deserialized from Event<T>.data)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ShipmentCreated {
    shipment_id:         Uuid,
    #[allow(dead_code)]
    merchant_id:         Uuid,
    origin_address:      String,
    destination_address: String,
    /// Canonical AWB assigned by order-intake (e.g. "CM-PH1-S0001234X").
    /// Use this as the tracking number; fall back to a computed value only for
    /// events emitted before the tracking_number field existed.
    #[serde(default)]
    tracking_number:     String,
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
    #[allow(dead_code)]
    driver_id:   Uuid,
}

/// POP submission from the pod service (`PickupCaptured`). This is the
/// authoritative chain-of-custody open — it fires even when driver-ops
/// `complete_task` is rejected (task not IN_PROGRESS) or the completeTask call
/// is dropped on a flaky network, so it must also advance the tracking record
/// to PickedUp. Only `shipment_id` is needed here.
#[derive(Debug, Deserialize)]
struct PickupCaptured {
    shipment_id: Uuid,
}

/// DELIVERY_COMPLETED is published by driver-ops as TaskCompleted.
/// Field aliases bridge the driver-ops naming to the tracking domain:
///   completed_at  → delivered_at
///   customer_name → recipient_name
#[derive(Debug, Deserialize)]
struct DeliveryCompleted {
    shipment_id:    Uuid,
    /// pod_id is optional — None for pickup tasks accidentally routed here or
    /// pre-POD legacy completions. Nil UUID stored when absent.
    #[serde(default)]
    pod_id:         Option<Uuid>,
    #[allow(dead_code)]
    driver_id:      Uuid,
    /// "delivered_at" from canonical DeliveryCompleted; "completed_at" from TaskCompleted.
    #[serde(alias = "completed_at")]
    delivered_at:   Option<String>,
    /// "recipient_name" from canonical DeliveryCompleted; "customer_name" from TaskCompleted.
    #[serde(alias = "customer_name")]
    recipient_name: Option<String>,
}

/// Mirrors the canonical `delivery.failed` payload. Fields the handler does
/// not currently branch on are still declared: serde would accept the event
/// without them, and the struct is the only written record of the contract.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct DeliveryFailed {
    shipment_id:            Uuid,
    reason:                 String,
    /// "attempted_at" from canonical; "failed_at" from TaskFailed (driver-ops).
    #[serde(alias = "failed_at")]
    attempted_at:           Option<String>,
    #[serde(default)]
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
            topics::PICKUP_CAPTURED,
            topics::DELIVERY_COMPLETED,
            topics::DELIVERY_FAILED,
            topics::LOCATION_UPDATED,
        ])
        .expect("Tracking consumer subscription failed");

    loop {
        match consumer.recv().await {
            Ok(msg) => {
                if let Err(e) = handle_message(msg.topic(), msg.payload(), &repo).await {
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
    topic:   &str,
    payload: Option<&[u8]>,
    repo:    &Arc<dyn TrackingRepository>,
) -> anyhow::Result<()> {
    let raw = match payload {
        Some(p) => p,
        None    => return Ok(()),
    };

    // All events are wrapped in the CloudEvents envelope published by KafkaProducer:
    //   { "id": ..., "source": ..., "tenant_id": ..., "time": ..., "data": { ...payload... } }
    // The Kafka producer does NOT set message headers, so tenant_id must come from the JSON body.
    let envelope: serde_json::Value = serde_json::from_slice(raw)?;

    let tenant_id = envelope["tenant_id"]
        .as_str()
        .and_then(|s| s.parse::<Uuid>().ok())
        .map(TenantId::from_uuid)
        .ok_or_else(|| anyhow::anyhow!("Missing tenant_id in event envelope on topic {topic}"))?;

    // Unwrap Event<T>.data; fall back to the whole envelope for forward-compat.
    let data = envelope.get("data").cloned().unwrap_or_else(|| envelope.clone());

    match topic {
        topics::SHIPMENT_CREATED => {
            let evt: ShipmentCreated = serde_json::from_value(data)?;
            // Use the canonical AWB carried in the event. Fall back to a
            // position-derived stub only for legacy events without the field.
            let tracking_number = if evt.tracking_number.is_empty() {
                format!(
                    "CM-{}",
                    evt.shipment_id.to_string().replace('-', "")[..8].to_uppercase()
                )
            } else {
                evt.tracking_number
            };
            let record = TrackingRecord::new(
                evt.shipment_id,
                tenant_id,
                tracking_number,
                evt.origin_address,
                evt.destination_address,
            );
            repo.save(&record).await?;
        }

        topics::SHIPMENT_CONFIRMED => {
            let evt: ShipmentConfirmed = serde_json::from_value(data)?;
            let mut record = require_record(repo, evt.shipment_id).await?;
            record.transition(TrackingStatus::Confirmed, "Shipment confirmed".into(), None);
            repo.save(&record).await?;
        }

        topics::SHIPMENT_CANCELLED => {
            let evt: ShipmentCancelled = serde_json::from_value(data)?;
            let mut record = require_record(repo, evt.shipment_id).await?;
            let reason = evt.reason.unwrap_or_else(|| "Cancelled by merchant".into());
            record.transition(TrackingStatus::Cancelled, reason, None);
            repo.save(&record).await?;
        }

        topics::DRIVER_ASSIGNED => {
            let evt: DriverAssigned = serde_json::from_value(data)?;
            let mut record = require_record(repo, evt.shipment_id).await?;
            let eta = evt.estimated_pickup_time
                .as_deref()
                .and_then(|s| s.parse::<DateTime<Utc>>().ok());
            record.assign_driver(evt.driver_id, "Your driver".into(), "".into(), eta);
            repo.save(&record).await?;
        }

        topics::PICKUP_COMPLETED => {
            let evt: PickupCompleted = serde_json::from_value(data)?;
            let mut record = require_record(repo, evt.shipment_id).await?;
            // Stamp the pickup leg with the origin (collection point). `transition`
            // dedupes if PICKUP_CAPTURED already advanced the record.
            let location = Some(record.origin_address.clone());
            record.transition(TrackingStatus::PickedUp, "Package picked up by driver".into(), location);
            repo.save(&record).await?;
        }

        topics::PICKUP_CAPTURED => {
            let evt: PickupCaptured = serde_json::from_value(data)?;
            let mut record = require_record(repo, evt.shipment_id).await?;
            // Authoritative custody-open. Idempotent with PICKUP_COMPLETED via the
            // dedupe guard in `transition` — whichever arrives first wins.
            let location = Some(record.origin_address.clone());
            record.transition(TrackingStatus::PickedUp, "Package picked up by driver".into(), location);
            repo.save(&record).await?;
        }

        topics::DELIVERY_COMPLETED => {
            let evt: DeliveryCompleted = serde_json::from_value(data)?;
            let mut record = require_record(repo, evt.shipment_id).await?;
            let delivered_at = evt.delivered_at
                .as_deref()
                .and_then(|s| s.parse::<DateTime<Utc>>().ok())
                .unwrap_or_else(Utc::now);
            let pod_id   = evt.pod_id.unwrap_or_else(Uuid::nil);
            let recipient = evt.recipient_name.unwrap_or_else(|| "Recipient".into());
            record.mark_delivered(pod_id, recipient, delivered_at);
            repo.save(&record).await?;
        }

        topics::DELIVERY_FAILED => {
            let evt: DeliveryFailed = serde_json::from_value(data)?;
            let mut record = require_record(repo, evt.shipment_id).await?;
            let next = evt.next_attempt_scheduled
                .as_deref()
                .and_then(|s| s.parse::<DateTime<Utc>>().ok());
            record.mark_failed(evt.reason, evt.attempt_number, next);
            repo.save(&record).await?;
        }

        topics::LOCATION_UPDATED => {
            let evt: LocationUpdated = serde_json::from_value(data)?;
            // Real-time position is broadcast from driver-ops via WebSocket hub;
            // the tracking store is a lower-frequency snapshot — update is a no-op here.
            tracing::debug!(driver_id = %evt.driver_id, lat = evt.lat, lng = evt.lng, "location update received");
        }

        other => {
            tracing::debug!(topic = other, "Tracking consumer: unhandled topic");
        }
    }

    Ok(())
}

async fn require_record(
    repo:        &Arc<dyn TrackingRepository>,
    shipment_id: Uuid,
) -> anyhow::Result<TrackingRecord> {
    repo.find_by_shipment_id(shipment_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("TrackingRecord not found for shipment {shipment_id}"))
}
