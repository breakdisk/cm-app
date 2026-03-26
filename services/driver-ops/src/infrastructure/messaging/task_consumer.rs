//! Consumes TASK_ASSIGNED events → creates DriverTask rows in driver_ops.tasks.

use logisticos_events::{envelope::Event, payloads::TaskAssigned, topics};
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    config::ClientConfig,
    Message,
};
use sqlx::PgPool;
use tokio::sync::watch;

pub async fn start_task_consumer(
    brokers: &str,
    group_id: &str,
    pool: PgPool,
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
                            if let Err(e) = handle_task_assigned(payload, &pool).await {
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

async fn handle_task_assigned(payload: &[u8], pool: &PgPool) -> anyhow::Result<()> {
    let event: Event<TaskAssigned> = serde_json::from_slice(payload)?;
    let t = event.data;

    // Insert the task row.
    // The driver row must already exist in driver_ops.drivers (created when the driver
    // user was registered). The FK will fail if the driver is unknown, which is logged
    // as a warning and skipped — Kafka offset is still committed so we don't loop forever.
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
            cod_amount_cents,
            special_instructions
        ) VALUES (
            $1, $2, $3, $4,
            'delivery', $5, 'pending',
            $6, $7, $8, $9, 'PH',
            $10, $11,
            $12, $13, $14, $15
        )
        ON CONFLICT (id) DO NOTHING
        "#,
    )
    .bind(t.task_id)
    .bind(t.driver_id)
    .bind(t.route_id)
    .bind(t.shipment_id)
    .bind(t.sequence)               // i32
    .bind(&t.address_line1)
    .bind(&t.address_city)
    .bind(&t.address_province)
    .bind(&t.address_postal_code)
    .bind(t.address_lat)            // Option<f64>
    .bind(t.address_lng)            // Option<f64>
    .bind(&t.customer_name)
    .bind(&t.customer_phone)
    .bind(t.cod_amount_cents)       // Option<i64>
    .bind(t.special_instructions.as_deref())
    .execute(pool)
    .await?;

    tracing::info!(
        task_id    = %t.task_id,
        driver_id  = %t.driver_id,
        shipment_id = %t.shipment_id,
        sequence   = t.sequence,
        "task consumer: task created for driver"
    );
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
