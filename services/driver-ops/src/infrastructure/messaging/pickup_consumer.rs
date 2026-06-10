//! Consumes PICKUP_CAPTURED events → transitions the pickup task to `in_progress`.
//!
//! When a driver submits a Proof of Pickup, the pod service publishes
//! `logisticos.pod.pickup.captured` (see services/pod pod_service.rs). Two
//! services react to that custody-open event:
//!   • payments  — debits the driver cash-flow ledger (Track A) / opens billing.
//!   • driver-ops (this consumer) — marks the pickup task `in_progress` so the
//!     shipment is reflected as "in transit" / custody-opened across the ops
//!     dashboards and the driver app task list.
//!
//! Without this consumer the `pickup.captured` event was published but dropped
//! on the floor: a pickup task stayed `pending` forever even after the driver
//! had physically collected the parcel and the platform had opened liability.
//!
//! Idempotency: the UPDATE only fires for a task still in `pending`. A redelivered
//! event (Kafka at-least-once) or a task already `in_progress`/`completed`/`failed`
//! is a no-op — we never regress a terminal status back to `in_progress`.

use logisticos_events::{envelope::Event, payloads::PickupCaptured, topics};
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    config::ClientConfig,
    Message,
};
use sqlx::PgPool;
use tokio::sync::watch;

pub async fn start_pickup_consumer(
    brokers: &str,
    group_id: &str,
    pool: PgPool,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", &format!("{}-pickup", group_id))
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()?;

    consumer.subscribe(&[topics::PICKUP_CAPTURED])?;

    tracing::info!("pickup consumer: subscribed to {}", topics::PICKUP_CAPTURED);

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    tracing::info!("pickup consumer: shutdown signal received");
                    break;
                }
            }
            result = consumer.recv() => {
                match result {
                    Ok(msg) => {
                        if let Some(payload) = msg.payload() {
                            if let Err(e) = handle_pickup_captured(payload, &pool).await {
                                tracing::warn!(err = %e, "pickup consumer: handler error (skipping)");
                            }
                        }
                        consumer.commit_message(&msg, CommitMode::Async).ok();
                    }
                    Err(e) => {
                        tracing::error!(err = %e, "pickup consumer: recv error");
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                }
            }
        }
    }
    Ok(())
}

async fn handle_pickup_captured(payload: &[u8], pool: &PgPool) -> anyhow::Result<()> {
    let event: Event<PickupCaptured> = serde_json::from_slice(payload)?;
    let p = event.data;

    // Transition the pickup task to in_progress. Scope to task_type = 'pickup'
    // and status = 'pending' so we only advance the leg the POP belongs to and
    // never overwrite a task that has already progressed (idempotent on replay).
    // started_at is stamped only when null so a redelivery keeps the first time.
    let result = sqlx::query(
        r#"
        UPDATE driver_ops.tasks
           SET status     = 'in_progress',
               started_at = COALESCE(started_at, NOW())
         WHERE id = $1
           AND task_type = 'pickup'
           AND status = 'pending'
        "#,
    )
    .bind(p.task_id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        // No pending pickup task matched. Either the TASK_ASSIGNED event has not
        // landed yet (task row absent), the task already advanced, or this leg is
        // not a pickup. All are benign — log at debug and let the offset commit.
        tracing::debug!(
            task_id     = %p.task_id,
            shipment_id = %p.shipment_id,
            "pickup consumer: no pending pickup task to advance (already in_progress / absent)"
        );
    } else {
        tracing::info!(
            task_id     = %p.task_id,
            shipment_id = %p.shipment_id,
            driver_id   = %p.driver_id,
            pop_id      = %p.pop_id,
            "pickup consumer: pickup task marked in_progress (custody opened)"
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn pickup_captured_payload_deserializes() {
        // The JSON uses "data" key (not "payload") — matches Event<T> struct.
        let json = r#"{
            "id":"00000000-0000-0000-0000-000000000099",
            "source":"pod",
            "event_type":"pickup.captured",
            "tenant_id":"00000000-0000-0000-0000-000000000001",
            "time":"2026-01-01T00:00:00Z",
            "data":{
                "pop_id":"00000000-0000-0000-0000-000000000020",
                "shipment_id":"00000000-0000-0000-0000-000000000012",
                "task_id":"00000000-0000-0000-0000-000000000010",
                "tenant_id":"00000000-0000-0000-0000-000000000001",
                "driver_id":"00000000-0000-0000-0000-000000000004",
                "geofence_verified":true,
                "barcode_scanned":true,
                "captured_at":"2026-01-01T00:00:00Z"
            }
        }"#;

        let result: Result<
            logisticos_events::envelope::Event<logisticos_events::payloads::PickupCaptured>,
            _,
        > = serde_json::from_str(json);
        assert!(result.is_ok(), "Deserialization failed: {:?}", result.err());
        let ev = result.unwrap();
        assert_eq!(
            ev.data.task_id.to_string(),
            "00000000-0000-0000-0000-000000000010"
        );
        assert!(ev.data.geofence_verified);
    }
}
