//! Kafka consumer that updates canonical shipment status when downstream
//! services report progress (driver assigned, delivered, failed).
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
struct DeliveryCompletedEvt {
    shipment_id: Uuid,
}

#[derive(Serialize, Deserialize)]
struct DeliveryFailedEvt {
    shipment_id: Uuid,
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
        topics::DELIVERY_COMPLETED,
        topics::DELIVERY_FAILED,
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
        _ => {}
    }
    Ok(())
}
