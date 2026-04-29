//! Consumes DRIVER_AVAILABLE events → retries all pending dispatch_queue rows
//! for the driver's tenant.
//!
//! When auto-dispatch fails at booking time (no driver available), the shipment
//! sits with status='pending' in dispatch_queue. This consumer fires whenever a
//! driver goes online and sweeps those rows, attempting quick_dispatch for each.
//! Failures are still non-fatal — the row stays pending for manual ops intervention.

use logisticos_events::{envelope::Event, payloads::DriverAvailable, topics};
use logisticos_types::TenantId;
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    config::ClientConfig,
    message::Message,
};
use std::sync::Arc;
use tokio::sync::watch;

use crate::application::commands::QuickDispatchCommand;
use crate::application::services::DriverAssignmentService;
use crate::infrastructure::db::dispatch_queue_repo::{DispatchQueueRepository, PgDispatchQueueRepository};
use sqlx::PgPool;

pub async fn start_driver_available_consumer(
    brokers: &str,
    group_id: &str,
    pool: PgPool,
    dispatch_service: Arc<DriverAssignmentService>,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", &format!("{}-driver-available", group_id))
        .set("auto.offset.reset", "latest") // only care about real-time availability, not replays
        .set("enable.auto.commit", "false")
        .create()?;

    consumer.subscribe(&[topics::DRIVER_AVAILABLE])?;
    let repo = Arc::new(PgDispatchQueueRepository::new(pool));

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow_and_update() {
                    tracing::info!("Driver-available consumer shutting down");
                    break;
                }
            }
            result = consumer.recv() => {
                match result {
                    Ok(msg) => {
                        if let Some(payload) = msg.payload() {
                            if let Err(e) = handle_driver_available(payload, &*repo, &dispatch_service).await {
                                tracing::warn!(err = %e, "driver-available consumer: handler error (skipping)");
                            }
                        }
                        consumer.commit_message(&msg, CommitMode::Async).ok();
                    }
                    Err(e) => {
                        tracing::error!(err = %e, "driver-available consumer: recv error");
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                }
            }
        }
    }

    Ok(())
}

async fn handle_driver_available(
    payload: &[u8],
    repo: &dyn DispatchQueueRepository,
    dispatch_service: &DriverAssignmentService,
) -> anyhow::Result<()> {
    let event: Event<DriverAvailable> = serde_json::from_slice(payload)?;
    let tenant_id = event.tenant_id;
    let driver_id = event.data.driver_id;

    let pending = repo.list_pending(tenant_id).await?;
    if pending.is_empty() {
        return Ok(());
    }

    tracing::info!(
        driver_id = %driver_id,
        tenant_id = %tenant_id,
        count     = pending.len(),
        "Driver came online — retrying pending dispatch queue"
    );

    for row in pending {
        let shipment_id = row.shipment_id;
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
                    triggered_by_driver = %driver_id,
                    "Pending shipment auto-dispatched on driver-available"
                );
            }
            Err(e) => {
                tracing::warn!(
                    shipment_id = %shipment_id,
                    err         = %e,
                    "Retry dispatch failed — shipment remains in queue"
                );
                if let Err(record_err) = repo
                    .record_failed_attempt(shipment_id, &e.to_string())
                    .await
                {
                    tracing::error!(
                        shipment_id = %shipment_id,
                        err         = %record_err,
                        "Failed to record retry attempt on dispatch_queue"
                    );
                }
            }
        }
    }

    Ok(())
}
