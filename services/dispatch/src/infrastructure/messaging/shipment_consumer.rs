//! Consumes SHIPMENT_CREATED events → inserts into dispatch_queue.

use logisticos_events::{envelope::Event, payloads::ShipmentCreated, topics};
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    config::ClientConfig,
    message::Message,
};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::watch;
use uuid::Uuid;

use crate::infrastructure::db::dispatch_queue_repo::{DispatchQueueRow, PgDispatchQueueRepository};

pub async fn start_shipment_consumer(
    brokers: &str,
    group_id: &str,
    pool: PgPool,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", &format!("{}-shipment", group_id))
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()?;

    consumer.subscribe(&[topics::SHIPMENT_CREATED])?;
    let repo = Arc::new(PgDispatchQueueRepository::new(pool));

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow_and_update() {
                    tracing::info!("Shipment consumer shutting down");
                    break;
                }
            }
            result = consumer.recv() => {
                match result {
                    Ok(msg) => {
                        if let Some(payload) = msg.payload() {
                            if let Err(e) = handle_shipment_created(payload, &repo).await {
                                tracing::warn!(err = %e, "shipment consumer: handler error (skipping)");
                            }
                        }
                        consumer.commit_message(&msg, CommitMode::Async).ok();
                    }
                    Err(e) => {
                        tracing::error!(err = %e, "shipment consumer: recv error");
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                }
            }
        }
    }

    Ok(())
}

async fn handle_shipment_created(
    payload: &[u8],
    repo: &PgDispatchQueueRepository,
) -> anyhow::Result<()> {
    let event: Event<ShipmentCreated> = serde_json::from_slice(payload)?;
    // Use tenant_id from the Event envelope (authoritative), not merchant_id from the payload
    let tenant_id = event.tenant_id;
    let d = event.data;

    let row = DispatchQueueRow {
        id:                   Uuid::new_v4(),
        tenant_id,
        shipment_id:          d.shipment_id,
        customer_name:        d.customer_name,
        customer_phone:       d.customer_phone,
        dest_address_line1:   d.destination_address,
        dest_city:            d.destination_city,
        dest_province:        String::new(),    // TODO: ShipmentCreated payload doesn't carry province yet — add to payload in Task 2 follow-up
        dest_postal_code:     String::new(),    // TODO: Same — postal_code not in ShipmentCreated payload
        dest_lat:             d.destination_lat,
        dest_lng:             d.destination_lng,
        cod_amount_cents:     d.cod_amount_cents,
        special_instructions: None,             // TODO: ShipmentCreated payload doesn't carry special_instructions yet — add to payload in Task 2 follow-up
        service_type:         d.service_type,
        status:               "pending".to_string(),
    };

    repo.upsert(&row).await?;
    tracing::info!(shipment_id = %d.shipment_id, "Shipment added to dispatch queue");
    Ok(())
}
