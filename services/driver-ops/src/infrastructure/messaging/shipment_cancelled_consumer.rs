//! Consumes SHIPMENT_CANCELLED (order-intake) → cancels the driver's task for
//! it and tells the driver. Before this a cancelled shipment stayed on the
//! driver's list.

use std::sync::Arc;

use logisticos_events::{envelope::Event, topics};
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    config::ClientConfig,
    Message,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tokio::sync::watch;
use uuid::Uuid;

use crate::infrastructure::external::FcmClient;

/// The part of order-intake's `shipment.cancelled` driver-ops reads. The
/// tenant comes from the envelope.
#[derive(Debug, Serialize, Deserialize)]
struct ShipmentCancelledData {
    shipment_id: Uuid,
}

pub async fn start_shipment_cancelled_consumer(
    brokers: &str,
    group_id: &str,
    pool: PgPool,
    fcm: Option<Arc<FcmClient>>,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", format!("{}-shipment-cancelled", group_id))
        // A missed cancellation leaves a driver heading to a job that isn't there.
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()?;

    consumer.subscribe(&[topics::SHIPMENT_CANCELLED])?;
    tracing::info!("shipment-cancelled consumer: subscribed to {}", topics::SHIPMENT_CANCELLED);

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    tracing::info!("shipment-cancelled consumer: shutdown signal received");
                    break;
                }
            }
            result = consumer.recv() => {
                match result {
                    Ok(msg) => {
                        if let Some(payload) = msg.payload() {
                            // Retried in place; the handler is replay-safe.
                            let mut attempt = 0u32;
                            while let Err(e) = handle_shipment_cancelled(payload, &pool, fcm.as_ref()).await {
                                attempt += 1;
                                if attempt >= 6 {
                                    tracing::error!(offset = msg.offset(), err = %e,
                                        "shipment-cancelled consumer: failed 6 times — cancel the task from the ops console");
                                    break;
                                }
                                tracing::warn!(attempt, err = %e, "shipment-cancelled consumer: handler error — retrying");
                                tokio::time::sleep(std::time::Duration::from_secs(2u64.pow(attempt.min(5)))).await;
                            }
                        }
                        consumer.commit_message(&msg, CommitMode::Async).ok();
                    }
                    Err(e) => {
                        tracing::error!(err = %e, "shipment-cancelled consumer: recv error");
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                }
            }
        }
    }
    Ok(())
}

async fn handle_shipment_cancelled(payload: &[u8], pool: &PgPool, fcm: Option<&Arc<FcmClient>>) -> anyhow::Result<()> {
    let event: Event<ShipmentCancelledData> = serde_json::from_slice(payload)?;
    let (tenant_id, shipment_id) = (event.tenant_id, event.data.shipment_id);
    if tenant_id.is_nil() {
        tracing::warn!(shipment_id = %shipment_id, "shipment.cancelled without a tenant — skipped");
        return Ok(());
    }

    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO driver_ops.cancelled_shipments (shipment_id, tenant_id) VALUES ($1, $2)
         ON CONFLICT (shipment_id) DO NOTHING",
    )
    .bind(shipment_id)
    .bind(tenant_id)
    .execute(&mut *tx)
    .await?;
    // Only work not yet done: a completed or failed task is history.
    let cancelled: Vec<(Uuid, Option<String>)> = sqlx::query_as(
        r#"UPDATE driver_ops.tasks t
              SET status = 'cancelled'
             FROM driver_ops.drivers d
            WHERE t.driver_id = d.id
              AND d.tenant_id = $2
              AND t.shipment_id = $1
              AND t.status IN ('pending', 'in_progress')
            RETURNING d.user_id, t.tracking_number"#,
    )
    .bind(shipment_id)
    .bind(tenant_id)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;

    tracing::info!(shipment_id = %shipment_id, tasks = cancelled.len(), "Tasks cancelled with their shipment");

    // One push per driver, however many legs they held. A dispatch_message
    // refreshes the app's task list and shows the notice.
    if let Some(fcm) = fcm {
        let mut told: Vec<Uuid> = Vec::new();
        for (driver_user_id, tracking) in cancelled {
            if told.contains(&driver_user_id) {
                continue;
            }
            told.push(driver_user_id);
            let data = cancelled_push(tracking.as_deref());
            let fcm = Arc::clone(fcm);
            tokio::spawn(async move { fcm.notify_data(driver_user_id, &data).await });
        }
    }
    Ok(())
}

/// The driver's notice: the job is off their list.
fn cancelled_push(tracking: Option<&str>) -> serde_json::Value {
    let what = tracking.filter(|t| !t.is_empty()).map_or_else(|| "a job".to_string(), |t| format!("job {t}"));
    serde_json::json!({
        "type":  "dispatch_message",
        "title": "Job cancelled",
        "body":  format!("The customer cancelled {what}. It's off your list — don't go."),
    })
}

/// A shipment cancelled upstream. The task consumer asks before creating a
/// task, since task.assigned can arrive after the cancellation.
pub async fn is_cancelled(pool: &PgPool, shipment_id: Uuid) -> anyhow::Result<bool> {
    Ok(sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM driver_ops.cancelled_shipments WHERE shipment_id = $1)")
        .bind(shipment_id)
        .fetch_one(pool)
        .await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_order_intakes_cancellation() {
        let json = r#"{
            "id":"00000000-0000-0000-0000-000000000099",
            "source":"logisticos/order-intake",
            "event_type":"shipment.cancelled",
            "tenant_id":"00000000-0000-0000-0000-000000000001",
            "time":"2026-09-19T08:00:00Z",
            "data":{"shipment_id":"00000000-0000-0000-0000-0000000000aa","reason":"payment_expired"}
        }"#;
        let event: Event<ShipmentCancelledData> = serde_json::from_str(json).expect("parses");
        assert_eq!(event.data.shipment_id.to_string(), "00000000-0000-0000-0000-0000000000aa");
    }

    #[test]
    fn the_notice_names_the_job_and_refreshes_the_list() {
        let push = cancelled_push(Some("CM-PH1-S0000001X"));
        // dispatch_message is the app's refresh-the-task-list push.
        assert_eq!(push["type"], "dispatch_message");
        assert!(push["body"].as_str().expect("body").contains("CM-PH1-S0000001X"));
        assert!(cancelled_push(None)["body"].as_str().expect("body").contains("a job"));
        assert!(cancelled_push(Some(""))["body"].as_str().expect("body").contains("a job"));
    }
}
