//! Consumes SHIPMENT_CREATED events → inserts into dispatch_queue.
//!
//! When `auto_dispatch == true`, the consumer immediately auto-assigns the
//! shipment via `DriverAssignmentService::quick_dispatch`. `auto_dispatch` is
//! the agentic-first trigger — customer and merchant roles default it on at
//! intake time, admin defaults off. It is intentionally decoupled from
//! `booked_by_customer`, which now carries billing semantics only (drives
//! PaymentReceipt vs merchant invoice in the payments service).
//!
//! Auto-dispatch failures (e.g. no available driver) are logged but do not
//! fail the Kafka message — the shipment stays queued for manual dispatch.

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
use crate::infrastructure::db::dispatch_queue_repo::{DispatchQueueRepository, DispatchQueueRow, PgDispatchQueueRepository};

pub async fn start_shipment_consumer(
    brokers: &str,
    group_id: &str,
    pool: PgPool,
    dispatch_service: Arc<DriverAssignmentService>,
    ai_dispatch_enabled: bool,
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
                            if let Err(e) = handle_shipment_created(payload, &*repo, &dispatch_service, ai_dispatch_enabled).await {
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
    repo: &dyn DispatchQueueRepository,
    dispatch_service: &DriverAssignmentService,
    ai_dispatch_enabled: bool,
) -> anyhow::Result<()> {
    let event: Event<ShipmentCreated> = serde_json::from_slice(payload)?;
    // Use tenant_id from the Event envelope (authoritative), not merchant_id from the payload
    let tenant_id = event.tenant_id;
    let d = event.data;
    let shipment_id = d.shipment_id;
    let booked_by_customer = d.booked_by_customer;
    let auto_dispatch = d.auto_dispatch;

    let row = DispatchQueueRow {
        id:                   Uuid::new_v4(),
        tenant_id,
        shipment_id,
        customer_id:          d.customer_id,  // From ShipmentCreated payload
        customer_name:        d.customer_name,
        customer_phone:       d.customer_phone,
        customer_email:       if d.customer_email.is_empty() { None } else { Some(d.customer_email) },
        tracking_number:      if d.tracking_number.is_empty() { None } else { Some(d.tracking_number) },
        dest_address_line1:   d.destination_address,
        dest_city:            d.destination_city,
        dest_province:        d.destination_province,
        dest_postal_code:     d.destination_postal_code,
        dest_lat:             d.destination_lat,
        dest_lng:             d.destination_lng,
        origin_address_line1: d.origin_address,
        origin_city:          d.origin_city,
        origin_province:      d.origin_province,
        origin_postal_code:   d.origin_postal_code,
        origin_lat:           d.origin_lat,
        origin_lng:           d.origin_lng,
        cod_amount_cents:     d.cod_amount_cents,
        special_instructions: d.special_instructions.clone(),
        service_type:         d.service_type,
        merchant_name:        d.merchant_name,
        delivery_category:    d.delivery_category,
        weight_grams:         i64::from(d.weight_grams),
        status:               "pending".to_string(),
        auto_dispatch_attempts: 0,
        last_dispatch_error:    None,
        last_attempt_at:        None,
        queued_at:              None,
        dispatched_at:          None,
    };

    repo.upsert(&row).await?;
    tracing::info!(shipment_id = %shipment_id, booked_by_customer, auto_dispatch, "Shipment added to dispatch queue");

    // Agentic-first: if the order-intake handler flagged this shipment for
    // auto-dispatch, assign the best available driver immediately.
    //
    // When AI_DISPATCH_ENABLED=true the Python DispatchAgent handles assignment
    // by consuming the same Kafka event and calling back via the internal
    // /v1/internal/shipments/:id/assign endpoint. We skip the heuristic here
    // to avoid a race condition where both paths attempt assignment.
    //
    // When AI dispatch is off (default), the heuristic runs synchronously as before.
    if auto_dispatch {
        if ai_dispatch_enabled {
            tracing::info!(
                shipment_id = %shipment_id,
                "AI dispatch enabled — deferring assignment to DispatchAgent (Kafka-triggered)"
            );
        } else {
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
                        "Shipment auto-dispatched (heuristic)"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        shipment_id = %shipment_id,
                        err         = %e,
                        "Auto-dispatch failed — shipment remains in queue for manual dispatch"
                    );
                    if let Err(record_err) = repo
                        .record_failed_attempt(shipment_id, &e.to_string())
                        .await
                    {
                        tracing::error!(
                            shipment_id = %shipment_id,
                            err         = %record_err,
                            "Failed to record auto-dispatch attempt on dispatch_queue"
                        );
                    }
                }
            }
        }
    }

    Ok(())
}
