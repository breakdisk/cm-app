//! Consumes DRIVER_AVAILABLE events → retries all pending dispatch_queue rows
//! for the driver's tenant.
//!
//! When auto-dispatch fails at booking time (no driver available), the shipment
//! sits with status='pending' in dispatch_queue. This consumer fires whenever a
//! driver goes online and sweeps those rows, attempting quick_dispatch for each.
//! Failures are still non-fatal — the row stays pending for manual ops intervention.
//!
//! Rows that have already been retried MAX_AUTO_RETRY_ATTEMPTS times are skipped
//! and logged as needing manual ops intervention. This prevents a permanently
//! unfulfillable shipment (e.g. missing geocoords, all drivers blocked) from
//! burning retry budget on every driver login event.
//!
//! Configurable via AUTO_RETRY_MAX_ATTEMPTS env var; default 5.

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

const DEFAULT_MAX_RETRY_ATTEMPTS: i32 = 5;

pub async fn start_driver_available_consumer(
    brokers: &str,
    group_id: &str,
    pool: PgPool,
    dispatch_service: Arc<DriverAssignmentService>,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let max_retry_attempts: i32 = std::env::var("AUTO_RETRY_MAX_ATTEMPTS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_MAX_RETRY_ATTEMPTS);

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
                            if let Err(e) = handle_driver_available(payload, &*repo, &dispatch_service, max_retry_attempts).await {
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
    max_retry_attempts: i32,
) -> anyhow::Result<()> {
    let event: Event<DriverAvailable> = serde_json::from_slice(payload)?;
    let tenant_id = event.tenant_id;
    let driver_id = event.data.driver_id;

    let pending = repo.list_pending(tenant_id).await?;
    if pending.is_empty() {
        return Ok(());
    }

    // Partition: rows within retry budget vs exhausted rows needing manual intervention.
    let (retryable, exhausted): (Vec<_>, Vec<_>) = pending
        .into_iter()
        .partition(|row| row.auto_dispatch_attempts < max_retry_attempts);

    if !exhausted.is_empty() {
        tracing::warn!(
            driver_id       = %driver_id,
            tenant_id       = %tenant_id,
            exhausted_count = exhausted.len(),
            max_attempts    = max_retry_attempts,
            exhausted_ids   = ?exhausted.iter().map(|r| r.shipment_id).collect::<Vec<_>>(),
            "Skipping exhausted shipments — manual ops intervention required"
        );
    }

    if retryable.is_empty() {
        return Ok(());
    }

    tracing::info!(
        driver_id = %driver_id,
        tenant_id = %tenant_id,
        count     = retryable.len(),
        "Driver came online — retrying pending dispatch queue"
    );

    for row in retryable {
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
                    shipment_id         = %shipment_id,
                    driver_id           = %assignment.driver_id.inner(),
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
                // record_failed_attempt is a best-effort audit trail.
                // quick_dispatch already calls reset_to_pending on failure (which
                // increments auto_dispatch_attempts), so this may result in a
                // double-increment on some error paths. That's acceptable —
                // it only accelerates reaching the exhaustion threshold.
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
