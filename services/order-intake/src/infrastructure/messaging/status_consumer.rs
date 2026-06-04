//! Kafka consumer that updates canonical shipment status when downstream
//! services report progress:
//!
//!   logisticos.dispatch.driver.assigned   → pickup_assigned
//!   logisticos.driver.pickup.completed    → picked_up
//!   logisticos.driver.delivery.completed  → delivered
//!   logisticos.driver.delivery.failed     → failed
//!
//! All messages are wrapped in Event<T> by KafkaProducer — unwrap `.data` before using payload.

use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    Message,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;
use logisticos_events::{Event, topics};

#[derive(Serialize, Deserialize)]
struct DriverAssignedEvt {
    shipment_id: Uuid,
}

#[derive(Serialize, Deserialize)]
struct PickupCompletedEvt {
    shipment_id: Uuid,
}

#[derive(Serialize, Deserialize)]
struct DeliveryCompletedEvt {
    shipment_id: Uuid,
}

#[derive(Serialize, Deserialize)]
struct DeliveryFailedEvt {
    shipment_id: Uuid,
}

#[derive(Serialize, Deserialize)]
struct DeliveryAttemptedEvt {
    shipment_id: Uuid,
}

/// Cross-border hub event keyed by a single shipment (piece scan, dispatch/carrier request).
#[derive(Serialize, Deserialize)]
struct HubShipmentEvt {
    shipment_id: Uuid,
}

/// Cross-border container event keyed by the master AWBs it carries.
#[derive(Serialize, Deserialize)]
struct HubAwbsEvt {
    #[serde(default)]
    master_awbs: Vec<String>,
}

/// How a cross-border hub event locates the shipments it affects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MatchKey {
    /// Event carries a single `shipment_id`.
    ShipmentId,
    /// Event carries `master_awbs: Vec<String>` — match on `shipments.awb`.
    MasterAwbs,
}

/// Maps a cross-border hub topic to the canonical shipment status it drives and
/// how to locate the affected shipments. Returns `None` for non-hub topics so the
/// caller falls through to the legacy per-topic handling.
fn hub_status_mapping(topic: &str) -> Option<(&'static str, MatchKey)> {
    use MatchKey::*;
    Some(match topic {
        topics::HUB_PIECE_SCANNED_INBOUND     => ("at_hub",           ShipmentId),
        // Consolidation flow: all pieces loaded onto truck via 3D load-plan scan →
        // flip any shipment not yet at_hub (induction-only path, no explicit scan).
        topics::CONSOLIDATION_PLAN_LOADED     => ("at_hub",           MasterAwbs),
        topics::CONTAINER_DEPARTED            => ("in_transit",       MasterAwbs),
        topics::CONTAINER_CUSTOMS_HOLD        => ("customs_hold",     MasterAwbs),
        topics::CONTAINER_CUSTOMS_CLEARED     => ("in_transit",       MasterAwbs),
        topics::CONTAINER_DECONSOLIDATED      => ("at_hub",           MasterAwbs),
        topics::HUB_DISPATCH_REQUESTED        => ("out_for_delivery", ShipmentId),
        topics::HUB_CARRIER_BOOKING_REQUESTED => ("out_for_delivery", ShipmentId),
        _ => return None,
    })
}

pub async fn start_status_consumer(
    brokers: &str,
    group_id: &str,
    pool: PgPool,
) -> anyhow::Result<()> {
    use rdkafka::config::ClientConfig;
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", &format!("{}-status", group_id))
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()?;

    consumer.subscribe(&[
        topics::DRIVER_ASSIGNED,
        topics::PICKUP_COMPLETED,
        topics::DELIVERY_ATTEMPTED,
        topics::DELIVERY_COMPLETED,
        topics::DELIVERY_FAILED,
        // Cross-border hub transfer milestones
        topics::HUB_PIECE_SCANNED_INBOUND,
        topics::CONTAINER_DEPARTED,
        topics::CONTAINER_CUSTOMS_HOLD,
        topics::CONTAINER_CUSTOMS_CLEARED,
        topics::CONTAINER_DECONSOLIDATED,
        topics::HUB_DISPATCH_REQUESTED,
        topics::HUB_CARRIER_BOOKING_REQUESTED,
        // Consolidation milestones
        topics::CONSOLIDATION_PLAN_LOADED,
    ])?;

    loop {
        match consumer.recv().await {
            Ok(msg) => {
                let payload = match msg.payload() {
                    Some(p) => p,
                    None => {
                        consumer.commit_message(&msg, CommitMode::Async).ok();
                        continue;
                    }
                };
                let topic = msg.topic();
                if let Err(e) = handle(&pool, topic, payload).await {
                    tracing::warn!(topic, err = %e, "status consumer: handler error (skipping)");
                }
                consumer.commit_message(&msg, CommitMode::Async).ok();
            }
            Err(e) => {
                tracing::error!(err = %e, "status consumer: recv error");
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    }
}

async fn handle(pool: &PgPool, topic: &str, payload: &[u8]) -> anyhow::Result<()> {
    // Cross-border hub transfer events share a uniform forward-only update path.
    if let Some((new_status, key)) = hub_status_mapping(topic) {
        return apply_hub_status(pool, topic, payload, new_status, key).await;
    }
    match topic {
        topics::DRIVER_ASSIGNED => {
            let envelope: Event<DriverAssignedEvt> = serde_json::from_slice(payload)?;
            let evt = envelope.data;
            let result = sqlx::query(
                "UPDATE order_intake.shipments SET status = 'pickup_assigned', updated_at = NOW()
                 WHERE id = $1 AND status NOT IN ('delivered','cancelled','returned')",
            )
            .bind(evt.shipment_id)
            .execute(pool)
            .await?;
            if result.rows_affected() == 0 {
                tracing::warn!(
                    shipment_id = %evt.shipment_id,
                    "DRIVER_ASSIGNED: no shipment updated (unknown id or already in terminal status)"
                );
            }
        }
        topics::PICKUP_COMPLETED => {
            let envelope: Event<PickupCompletedEvt> = serde_json::from_slice(payload)?;
            let evt = envelope.data;
            // Forward-only: don't overwrite later states if an out-of-order event arrives.
            let result = sqlx::query(
                "UPDATE order_intake.shipments SET status = 'picked_up', updated_at = NOW()
                 WHERE id = $1
                   AND status IN ('pending','confirmed','pickup_assigned')",
            )
            .bind(evt.shipment_id)
            .execute(pool)
            .await?;
            if result.rows_affected() == 0 {
                tracing::warn!(
                    shipment_id = %evt.shipment_id,
                    "PICKUP_COMPLETED: no shipment updated (unknown id or already past pickup)"
                );
            }
        }
        topics::DELIVERY_COMPLETED => {
            let envelope: Event<DeliveryCompletedEvt> = serde_json::from_slice(payload)?;
            let evt = envelope.data;
            let result = sqlx::query(
                "UPDATE order_intake.shipments SET status = 'delivered', updated_at = NOW()
                 WHERE id = $1",
            )
            .bind(evt.shipment_id)
            .execute(pool)
            .await?;
            if result.rows_affected() == 0 {
                tracing::warn!(
                    shipment_id = %evt.shipment_id,
                    "DELIVERY_COMPLETED: no shipment updated (unknown id)"
                );
            }
        }
        topics::DELIVERY_FAILED => {
            let envelope: Event<DeliveryFailedEvt> = serde_json::from_slice(payload)?;
            let evt = envelope.data;
            let result = sqlx::query(
                "UPDATE order_intake.shipments SET status = 'failed', updated_at = NOW()
                 WHERE id = $1 AND status NOT IN ('delivered','cancelled')",
            )
            .bind(evt.shipment_id)
            .execute(pool)
            .await?;
            if result.rows_affected() == 0 {
                tracing::warn!(
                    shipment_id = %evt.shipment_id,
                    "DELIVERY_FAILED: no shipment updated (unknown id or already in terminal status)"
                );
            }
        }
        topics::DELIVERY_ATTEMPTED => {
            let envelope: Event<DeliveryAttemptedEvt> = serde_json::from_slice(payload)?;
            let evt = envelope.data;
            let result = sqlx::query(
                "UPDATE order_intake.shipments SET status = 'delivery_attempted', updated_at = NOW()
                 WHERE id = $1 AND status NOT IN ('delivered','cancelled','returned')",
            )
            .bind(evt.shipment_id)
            .execute(pool)
            .await?;
            if result.rows_affected() == 0 {
                tracing::warn!(
                    shipment_id = %evt.shipment_id,
                    "DELIVERY_ATTEMPTED: no shipment updated (unknown id or already in terminal status)"
                );
            }
        }
        _ => {}
    }
    Ok(())
}

/// Apply a cross-border hub status transition. Forward-only: never overwrites a
/// terminal status. Container events match by `master_awbs`; shipment-scoped
/// events match by `shipment_id`.
async fn apply_hub_status(
    pool: &PgPool,
    topic: &str,
    payload: &[u8],
    new_status: &str,
    key: MatchKey,
) -> anyhow::Result<()> {
    const TERMINAL: &str = "('delivered','cancelled','returned','failed')";
    let rows = match key {
        MatchKey::ShipmentId => {
            let envelope: Event<HubShipmentEvt> = serde_json::from_slice(payload)?;
            sqlx::query(&format!(
                "UPDATE order_intake.shipments SET status = $2, updated_at = NOW()
                 WHERE id = $1 AND status NOT IN {TERMINAL}"
            ))
            .bind(envelope.data.shipment_id)
            .bind(new_status)
            .execute(pool)
            .await?
            .rows_affected()
        }
        MatchKey::MasterAwbs => {
            let envelope: Event<HubAwbsEvt> = serde_json::from_slice(payload)?;
            let awbs = envelope.data.master_awbs;
            if awbs.is_empty() {
                tracing::warn!(topic, "hub event carried no master_awbs — nothing to update");
                return Ok(());
            }
            sqlx::query(&format!(
                "UPDATE order_intake.shipments SET status = $2, updated_at = NOW()
                 WHERE awb = ANY($1) AND status NOT IN {TERMINAL}"
            ))
            .bind(&awbs)
            .bind(new_status)
            .execute(pool)
            .await?
            .rows_affected()
        }
    };
    if rows == 0 {
        tracing::warn!(topic, new_status, "hub status: no shipment updated (unknown id/awb or terminal)");
    } else {
        tracing::info!(topic, new_status, rows, "hub status applied");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_cross_border_hub_topics_to_statuses() {
        assert_eq!(
            hub_status_mapping(topics::HUB_PIECE_SCANNED_INBOUND),
            Some(("at_hub", MatchKey::ShipmentId))
        );
        assert_eq!(
            hub_status_mapping(topics::CONSOLIDATION_PLAN_LOADED),
            Some(("at_hub", MatchKey::MasterAwbs))
        );
        assert_eq!(
            hub_status_mapping(topics::CONTAINER_DEPARTED),
            Some(("in_transit", MatchKey::MasterAwbs))
        );
        assert_eq!(
            hub_status_mapping(topics::CONTAINER_CUSTOMS_HOLD),
            Some(("customs_hold", MatchKey::MasterAwbs))
        );
        assert_eq!(
            hub_status_mapping(topics::CONTAINER_CUSTOMS_CLEARED),
            Some(("in_transit", MatchKey::MasterAwbs))
        );
        assert_eq!(
            hub_status_mapping(topics::CONTAINER_DECONSOLIDATED),
            Some(("at_hub", MatchKey::MasterAwbs))
        );
        assert_eq!(
            hub_status_mapping(topics::HUB_DISPATCH_REQUESTED),
            Some(("out_for_delivery", MatchKey::ShipmentId))
        );
        assert_eq!(
            hub_status_mapping(topics::HUB_CARRIER_BOOKING_REQUESTED),
            Some(("out_for_delivery", MatchKey::ShipmentId))
        );
    }

    #[test]
    fn non_hub_topics_are_unmapped() {
        assert_eq!(hub_status_mapping(topics::PICKUP_COMPLETED), None);
        assert_eq!(hub_status_mapping(topics::DELIVERY_FAILED), None);
    }
}
