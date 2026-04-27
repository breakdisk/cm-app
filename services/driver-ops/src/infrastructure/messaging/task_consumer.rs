//! Consumes TASK_ASSIGNED events → creates DriverTask rows in driver_ops.tasks.

use std::sync::Arc;
use logisticos_events::{envelope::Event, payloads::TaskAssigned, topics};
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    config::ClientConfig,
    Message,
};
use sqlx::PgPool;
use tokio::sync::watch;
use crate::infrastructure::external::FcmClient;

pub async fn start_task_consumer(
    brokers: &str,
    group_id: &str,
    pool: PgPool,
    fcm: Option<Arc<FcmClient>>,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", &format!("{}-tasks", group_id))
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()?;

    consumer.subscribe(&[topics::TASK_ASSIGNED])?;

    tracing::info!("task consumer: subscribed to {}", topics::TASK_ASSIGNED);

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    tracing::info!("task consumer: shutdown signal received");
                    break;
                }
            }
            result = consumer.recv() => {
                match result {
                    Ok(msg) => {
                        if let Some(payload) = msg.payload() {
                            if let Err(e) = handle_task_assigned(payload, &pool, fcm.clone()).await {
                                tracing::warn!(err = %e, "task consumer: handler error (skipping)");
                            }
                        }
                        consumer.commit_message(&msg, CommitMode::Async).ok();
                    }
                    Err(e) => {
                        tracing::error!(err = %e, "task consumer: recv error");
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                }
            }
        }
    }
    Ok(())
}

async fn handle_task_assigned(payload: &[u8], pool: &PgPool, fcm: Option<Arc<FcmClient>>) -> anyhow::Result<()> {
    let event: Event<TaskAssigned> = serde_json::from_slice(payload)?;
    let t = event.data;

    // Validate task_type — driver_ops.tasks.task_type has a CHECK constraint and
    // task_service::complete_task only branches on Pickup/Delivery. Anything
    // outside that set is a producer bug; reject early so we don't poison the row.
    let task_type = match t.task_type.as_str() {
        "pickup" | "delivery" => t.task_type.as_str(),
        other => {
            tracing::warn!(
                task_id = %t.task_id,
                task_type = %other,
                "task consumer: unknown task_type — defaulting to 'delivery'"
            );
            "delivery"
        }
    };

    // Ensure driver row exists before inserting the task.
    // TASK_ASSIGNED may arrive before the driver has ever logged in (driver_ops.drivers is
    // populated on first app login). This guard creates a stub row so the FK doesn't fail.
    // ON CONFLICT (user_id): a driver registered via POST /v1/drivers may have a different
    // primary id — we must not overwrite it, just skip if user_id already exists.
    sqlx::query(
        r#"INSERT INTO driver_ops.drivers (id, user_id, tenant_id, first_name, last_name, phone, status)
           VALUES ($1, $1, $2, 'Driver', '', '', 'offline')
           ON CONFLICT (user_id) DO NOTHING"#,
    )
    .bind(t.driver_id)
    .bind(t.tenant_id)
    .execute(pool)
    .await?;

    // The task FK references drivers.id, not user_id. If a driver was registered via the API
    // (id = random uuid ≠ user_id), we must use the actual drivers.id here.
    let actual_driver_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM driver_ops.drivers WHERE user_id = $1",
    )
    .bind(t.driver_id)
    .fetch_one(pool)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO driver_ops.tasks (
            id,
            driver_id,
            route_id,
            shipment_id,
            task_type,
            sequence,
            status,
            address_line1,
            city,
            province,
            postal_code,
            country,
            lat,
            lng,
            customer_name,
            customer_phone,
            customer_email,
            tracking_number,
            cod_amount_cents,
            special_instructions
        ) VALUES (
            $1, $2, $3, $4,
            $5, $6, 'pending',
            $7, $8, $9, $10, 'PH',
            $11, $12,
            $13, $14, $15, $16, $17, $18
        )
        ON CONFLICT (id) DO NOTHING
        "#,
    )
    .bind(t.task_id)
    .bind(actual_driver_id)
    .bind(t.route_id)
    .bind(t.shipment_id)
    .bind(task_type)                // pickup | delivery (validated above)
    .bind(t.sequence)               // i32
    .bind(&t.address_line1)
    .bind(&t.address_city)
    .bind(&t.address_province)
    .bind(&t.address_postal_code)
    .bind(t.address_lat)            // Option<f64>
    .bind(t.address_lng)            // Option<f64>
    .bind(&t.customer_name)
    .bind(&t.customer_phone)
    .bind(if t.customer_email.is_empty() { None } else { Some(&t.customer_email) })
    .bind(if t.tracking_number.is_empty() { None } else { Some(&t.tracking_number) })
    .bind(t.cod_amount_cents)       // Option<i64>
    .bind(t.special_instructions.as_deref())
    .execute(pool)
    .await?;

    tracing::info!(
        task_id    = %t.task_id,
        driver_id  = %t.driver_id,
        shipment_id = %t.shipment_id,
        sequence   = t.sequence,
        task_type  = task_type,
        "task consumer: task created for driver"
    );

    // Fire-and-forget FCM push — does not block task creation or Kafka commit.
    // t.driver_id is the driver's user_id (identity-service UUID), which is what
    // identity.push_tokens.user_id indexes on.
    if let Some(fcm) = fcm {
        let driver_user_id = t.driver_id;
        tokio::spawn(async move {
            fcm.notify_driver(driver_user_id).await;
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn task_assigned_payload_deserializes() {
        // The JSON uses "data" key (not "payload") — matches Event<T> struct
        let json = r#"{
            "id":"00000000-0000-0000-0000-000000000099",
            "source":"dispatch",
            "event_type":"task.assigned",
            "tenant_id":"00000000-0000-0000-0000-000000000001",
            "time":"2026-01-01T00:00:00Z",
            "data":{
                "task_id":"00000000-0000-0000-0000-000000000010",
                "assignment_id":"00000000-0000-0000-0000-000000000011",
                "shipment_id":"00000000-0000-0000-0000-000000000012",
                "route_id":"00000000-0000-0000-0000-000000000013",
                "driver_id":"00000000-0000-0000-0000-000000000004",
                "tenant_id":"00000000-0000-0000-0000-000000000001",
                "sequence":1,
                "address_line1":"123 Test St",
                "address_city":"Manila",
                "address_province":"Metro Manila",
                "address_postal_code":"1000",
                "address_lat":14.5995,
                "address_lng":120.9842,
                "customer_name":"Test Customer",
                "customer_phone":"+63912345678",
                "cod_amount_cents":null,
                "special_instructions":null
            }
        }"#;

        let result: Result<
            logisticos_events::envelope::Event<logisticos_events::payloads::TaskAssigned>,
            _,
        > = serde_json::from_str(json);
        assert!(result.is_ok(), "Deserialization failed: {:?}", result.err());
        let ev = result.unwrap();
        assert_eq!(ev.data.customer_name, "Test Customer");
        assert_eq!(ev.data.sequence, 1i32);
    }
}
