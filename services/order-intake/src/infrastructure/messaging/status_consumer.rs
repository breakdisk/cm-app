//! Kafka consumer that updates canonical shipment status when downstream
//! services report progress:
//!
//!   logisticos.dispatch.driver.assigned   → pickup_assigned
//!   logisticos.driver.pickup.completed    → picked_up (driver-ops task completion)
//!   logisticos.pod.pickup.captured        → picked_up (POP submission — authoritative)
//!   logisticos.driver.delivery.completed  → delivered
//!   logisticos.driver.delivery.failed     → failed
//!
//! Both PICKUP_COMPLETED and PICKUP_CAPTURED advance the shipment to `picked_up`.
//! PICKUP_CAPTURED is the canonical chain-of-custody signal (POP submitted) — it
//! fires even when driver-ops `complete_task` is rejected (e.g. task wasn't in
//! IN_PROGRESS state) or when the task-completion call is dropped on a flaky
//! network. The shipment status must reflect the physical pickup regardless.
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
    /// Driver device/processing time the task was completed (ISO-8601).
    /// Stamped into the timeline event metadata for SLA + audit.
    #[serde(default)]
    completed_at: Option<String>,
}

/// POP-submitted payload from the pod service. Beyond `shipment_id` we also
/// capture the chain-of-custody timestamps and geofence verdict so the
/// timeline event carries the same evidence the POP card shows. The full
/// `PickupCaptured` event in libs/events carries more (driver_id,
/// photo_s3_key, weight, etc.) but those aren't needed for the timeline.
#[derive(Serialize, Deserialize)]
struct PickupCapturedEvt {
    shipment_id: Uuid,
    #[serde(default)]
    captured_at: Option<String>,
    #[serde(default)]
    device_timestamp: Option<String>,
    #[serde(default)]
    geofence_verified: Option<bool>,
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
        topics::PICKUP_CAPTURED,
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

/// Statuses past which a shipment can no longer be moved by a downstream event.
const TERMINAL_STATUSES: &[&str] = &["delivered", "cancelled", "returned", "failed"];

async fn handle(pool: &PgPool, topic: &str, payload: &[u8]) -> anyhow::Result<()> {
    // Cross-border hub transfer events share a uniform forward-only update path.
    if let Some((new_status, key)) = hub_status_mapping(topic) {
        return apply_hub_status(pool, topic, payload, new_status, key).await;
    }
    match topic {
        topics::DRIVER_ASSIGNED => {
            let evt = serde_json::from_slice::<Event<DriverAssignedEvt>>(payload)?.data;
            let n = apply_transition(
                pool, evt.shipment_id, "pickup_assigned",
                &[], &["delivered", "cancelled", "returned"],
                "assigned", "system", serde_json::json!({}),
            ).await?;
            warn_if_no_op(n, topic, evt.shipment_id);
        }
        topics::PICKUP_COMPLETED => {
            let evt = serde_json::from_slice::<Event<PickupCompletedEvt>>(payload)?.data;
            let n = apply_transition(
                pool, evt.shipment_id, "picked_up",
                &["pending", "confirmed", "pickup_assigned"], &[],
                "status_changed", "driver",
                serde_json::json!({ "device_timestamp": evt.completed_at }),
            ).await?;
            warn_if_no_op(n, topic, evt.shipment_id);
        }
        topics::PICKUP_CAPTURED => {
            // POP submission — authoritative chain-of-custody open. Advances the
            // shipment regardless of whether driver-ops also published
            // PICKUP_COMPLETED (it may not if the task wasn't IN_PROGRESS, or the
            // network dropped the completeTask call).
            let evt = serde_json::from_slice::<Event<PickupCapturedEvt>>(payload)?.data;
            let n = apply_transition(
                pool, evt.shipment_id, "picked_up",
                &["pending", "confirmed", "pickup_assigned"], &[],
                "status_changed", "driver",
                serde_json::json!({
                    "captured_at":       evt.captured_at,
                    "device_timestamp":  evt.device_timestamp,
                    "geofence_verified": evt.geofence_verified,
                    "via":               "pop",
                }),
            ).await?;
            warn_if_no_op(n, topic, evt.shipment_id);
        }
        topics::DELIVERY_COMPLETED => {
            let evt = serde_json::from_slice::<Event<DeliveryCompletedEvt>>(payload)?.data;
            let n = apply_transition(
                pool, evt.shipment_id, "delivered",
                &[], &[],
                "status_changed", "driver", serde_json::json!({}),
            ).await?;
            warn_if_no_op(n, topic, evt.shipment_id);
        }
        topics::DELIVERY_FAILED => {
            let evt = serde_json::from_slice::<Event<DeliveryFailedEvt>>(payload)?.data;
            let n = apply_transition(
                pool, evt.shipment_id, "failed",
                &[], &["delivered", "cancelled"],
                "status_changed", "driver", serde_json::json!({}),
            ).await?;
            warn_if_no_op(n, topic, evt.shipment_id);
        }
        topics::DELIVERY_ATTEMPTED => {
            let evt = serde_json::from_slice::<Event<DeliveryAttemptedEvt>>(payload)?.data;
            let n = apply_transition(
                pool, evt.shipment_id, "delivery_attempted",
                &[], &["delivered", "cancelled", "returned"],
                "status_changed", "driver", serde_json::json!({}),
            ).await?;
            warn_if_no_op(n, topic, evt.shipment_id);
        }
        _ => {}
    }
    Ok(())
}

fn warn_if_no_op(rows: u64, topic: &str, shipment_id: Uuid) {
    if rows == 0 {
        tracing::warn!(
            topic, %shipment_id,
            "status consumer: no shipment advanced (unknown id or already past this status)"
        );
    }
}

/// Atomically advance a single shipment's status **and** append an immutable
/// `shipment_events` row stamped with the server time (`created_at`) and a
/// city-level `location`. Forward-only: the event is written only when the
/// UPDATE actually changes a row, so out-of-order Kafka events never produce
/// phantom timeline entries.
///
/// * `allow` — if non-empty, the current status must be one of these.
/// * `deny`  — the current status must not be one of these.
///
/// Returns the number of shipments advanced (0 or 1).
async fn apply_transition(
    pool: &PgPool,
    shipment_id: Uuid,
    new_status: &str,
    allow: &[&str],
    deny: &[&str],
    event_type: &str,
    actor_type: &str,
    extra_metadata: serde_json::Value,
) -> anyhow::Result<u64> {
    let allow: Vec<String> = allow.iter().map(|s| s.to_string()).collect();
    let deny: Vec<String> = deny.iter().map(|s| s.to_string()).collect();
    let rows = sqlx::query(TRANSITION_BY_ID_SQL)
        .bind(shipment_id)
        .bind(new_status)
        .bind(&allow)
        .bind(&deny)
        .bind(event_type)
        .bind(actor_type)
        .bind(&extra_metadata)
        .execute(pool)
        .await?
        .rows_affected();
    Ok(rows)
}

/// `apply_transition` keyed by a set of master AWBs (cross-border container
/// events that fan out to every shipment they carry). Advances all matching
/// non-terminal shipments and writes one timeline event per shipment.
async fn apply_transition_by_awbs(
    pool: &PgPool,
    awbs: &[String],
    new_status: &str,
    deny: &[&str],
    event_type: &str,
    extra_metadata: serde_json::Value,
) -> anyhow::Result<u64> {
    let deny: Vec<String> = deny.iter().map(|s| s.to_string()).collect();
    let rows = sqlx::query(TRANSITION_BY_AWBS_SQL)
        .bind(awbs)
        .bind(new_status)
        .bind(&deny)
        .bind(event_type)
        .bind(&extra_metadata)
        .execute(pool)
        .await?
        .rows_affected();
    Ok(rows)
}

// City-level location label for a transition: origin city for the pickup leg,
// destination city for the delivery leg. Precise GPS lives in the POP/POD
// evidence records, surfaced separately on the detail/tracking pages.

/// $1 id, $2 new_status, $3 allow[], $4 deny[], $5 event_type, $6 actor_type, $7 metadata
const TRANSITION_BY_ID_SQL: &str = concat!(
    "WITH cur AS (\
        SELECT id, tenant_id, status AS from_status, origin_city, dest_city \
        FROM order_intake.shipments WHERE id = $1 FOR UPDATE\
     ), upd AS (\
        UPDATE order_intake.shipments s SET status = $2, updated_at = NOW() \
        FROM cur WHERE s.id = cur.id \
          AND (cardinality($3::text[]) = 0 OR cur.from_status = ANY($3)) \
          AND NOT (cur.from_status = ANY($4)) \
        RETURNING s.id\
     ) \
     INSERT INTO order_intake.shipment_events \
        (tenant_id, shipment_id, event_type, from_status, to_status, actor_type, metadata) \
     SELECT cur.tenant_id, cur.id, $5, cur.from_status, $2, $6, \
        jsonb_build_object('location', ",
    "CASE WHEN $2 IN ('pending','confirmed','pickup_assigned','picked_up') \
         THEN cur.origin_city ELSE cur.dest_city END",
    ") || $7::jsonb \
     FROM cur JOIN upd ON upd.id = cur.id",
);

/// $1 awbs[], $2 new_status, $3 deny[], $4 event_type, $5 metadata
const TRANSITION_BY_AWBS_SQL: &str = concat!(
    "WITH cur AS (\
        SELECT id, tenant_id, status AS from_status, origin_city, dest_city \
        FROM order_intake.shipments WHERE awb = ANY($1) FOR UPDATE\
     ), upd AS (\
        UPDATE order_intake.shipments s SET status = $2, updated_at = NOW() \
        FROM cur WHERE s.id = cur.id AND NOT (cur.from_status = ANY($3)) \
        RETURNING s.id\
     ) \
     INSERT INTO order_intake.shipment_events \
        (tenant_id, shipment_id, event_type, from_status, to_status, actor_type, metadata) \
     SELECT cur.tenant_id, cur.id, $4, cur.from_status, $2, 'system', \
        jsonb_build_object('location', ",
    "CASE WHEN $2 IN ('pending','confirmed','pickup_assigned','picked_up') \
         THEN cur.origin_city ELSE cur.dest_city END",
    ") || $5::jsonb \
     FROM cur JOIN upd ON upd.id = cur.id",
);

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
    let metadata = serde_json::json!({ "source": topic });
    let rows = match key {
        MatchKey::ShipmentId => {
            let envelope: Event<HubShipmentEvt> = serde_json::from_slice(payload)?;
            apply_transition(
                pool, envelope.data.shipment_id, new_status,
                &[], TERMINAL_STATUSES,
                "status_changed", "system", metadata,
            ).await?
        }
        MatchKey::MasterAwbs => {
            let envelope: Event<HubAwbsEvt> = serde_json::from_slice(payload)?;
            let awbs = envelope.data.master_awbs;
            if awbs.is_empty() {
                tracing::warn!(topic, "hub event carried no master_awbs — nothing to update");
                return Ok(());
            }
            apply_transition_by_awbs(
                pool, &awbs, new_status, TERMINAL_STATUSES,
                "status_changed", metadata,
            ).await?
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
