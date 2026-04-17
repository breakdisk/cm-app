//! Consumes SHIPMENT_CREATED events → inserts into dispatch_queue.
//! When `booked_by_customer == true`, the consumer immediately auto-dispatches
//! the shipment via `DriverAssignmentService::quick_dispatch` so the customer
//! app flow doesn't require a human dispatcher. Auto-dispatch failures (e.g.
//! no available driver) are logged but do not fail the Kafka message — the
//! shipment stays queued for manual dispatch.

use logisticos_events::{envelope::Event, payloads::ShipmentCreated, topics};
use logisticos_types::TenantId;
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    config::ClientConfig,
    message::Message,
};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::watch;
use uuid::Uuid;

use crate::application::commands::QuickDispatchCommand;
use crate::application::services::DriverAssignmentService;
use crate::infrastructure::db::dispatch_queue_repo::{DispatchQueueRow, PgDispatchQueueRepository};

pub async fn start_shipment_consumer(
    brokers: &str,
    group_id: &str,
    pool: PgPool,
    dispatch_service: Arc<DriverAssignmentService>,
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
                            if let Err(e) = handle_shipment_created(payload, &repo, &dispatch_service).await {
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
    dispatch_service: &DriverAssignmentService,
) -> anyhow::Result<()> {
    let event: Event<ShipmentCreated> = serde_json::from_slice(payload)?;
    // Use tenant_id from the Event envelope (authoritative), not merchant_id from the payload
    let tenant_id = event.tenant_id;
    let d = event.data;
    let shipment_id = d.shipment_id;
    let booked_by_customer = d.booked_by_customer;

    let row = DispatchQueueRow {
        id:                   Uuid::new_v4(),
        tenant_id,
        shipment_id,
        customer_name:        d.customer_name,
        customer_phone:       d.customer_phone,
        customer_email:       if d.customer_email.is_empty() { None } else { Some(d.customer_email) },
        tracking_number:      if d.tracking_number.is_empty() { None } else { Some(d.tracking_number) },
        dest_address_line1:   d.destination_address,
        dest_city:            d.destination_city,
        dest_province:        String::new(),    // TODO: ShipmentCreated payload doesn't carry province yet
        dest_postal_code:     String::new(),    // TODO: Same — postal_code not in ShipmentCreated payload
        dest_lat:             d.destination_lat,
        dest_lng:             d.destination_lng,
        cod_amount_cents:     d.cod_amount_cents,
        special_instructions: None,             // TODO: ShipmentCreated payload doesn't carry special_instructions yet
        service_type:         d.service_type,
        status:               "pending".to_string(),
    };

    repo.upsert(&row).await?;
    tracing::info!(shipment_id = %shipment_id, booked_by_customer, "Shipment added to dispatch queue");

    // Customer-app bookings have no human dispatcher — auto-assign the best
    // available driver immediately. If auto-dispatch fails (no driver in zone,
    // compliance block, etc.) the shipment stays in the queue for manual
    // dispatch and we just log the reason.
    if booked_by_customer {
        let cmd = QuickDispatchCommand {
            shipment_id,
            preferred_driver_id: None,
        };
        match dispatch_service
            .quick_dispatch(TenantId::from_uuid(tenant_id), cmd)
            .await
        {
            Ok(assignment) => {
                tracing::info!(
                    shipment_id = %shipment_id,
                    driver_id   = %assignment.driver_id.inner(),
                    "Customer booking auto-dispatched"
                );
            }
            Err(e) => {
                tracing::warn!(
                    shipment_id = %shipment_id,
                    err         = %e,
                    "Customer booking auto-dispatch failed — shipment remains in queue for manual dispatch"
                );
            }
        }
    }

    Ok(())
}
