//! Consumes JOB_DROPPED events (driver-ops) → takes the shipments off the
//! driver, puts them back in the queue, and offers them to the gig pool again.
//!
//! The driver who dropped a shipment is recorded in `dispatch.shipment_drops`,
//! which auto-selection and the offer waves both skip. If the broadcast finds
//! nobody, the row stays pending and the 90-second sweep dispatches it 1:1.

use logisticos_events::{envelope::Event, payloads::JobDropped, topics};
use logisticos_types::TenantId;
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    config::ClientConfig,
    message::Message,
};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::watch;

use crate::application::services::OfferService;
use crate::infrastructure::db::dispatch_queue_repo::PgDispatchQueueRepository;

pub async fn start_job_dropped_consumer(
    brokers: &str,
    group_id: &str,
    pool: PgPool,
    offer_service: Arc<OfferService>,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", format!("{}-job-dropped", group_id))
        // A missed drop strands a shipment on a driver who has left it.
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()?;

    consumer.subscribe(&[topics::JOB_DROPPED])?;
    let repo = PgDispatchQueueRepository::new(pool);

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow_and_update() {
                    tracing::info!("Job-dropped consumer shutting down");
                    break;
                }
            }
            result = consumer.recv() => {
                match result {
                    Ok(msg) => {
                        if let Some(payload) = msg.payload() {
                            if let Err(e) = handle_job_dropped(payload, &repo, &offer_service).await {
                                // Not committed: redelivered, and replay-safe.
                                tracing::error!(err = %e, "job-dropped consumer: handler error");
                                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                                continue;
                            }
                        }
                        consumer.commit_message(&msg, CommitMode::Async).ok();
                    }
                    Err(e) => {
                        tracing::error!(err = %e, "job-dropped consumer: recv error");
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                }
            }
        }
    }

    Ok(())
}

async fn handle_job_dropped(
    payload: &[u8],
    repo: &PgDispatchQueueRepository,
    offer_service: &OfferService,
) -> anyhow::Result<()> {
    let event: Event<JobDropped> = serde_json::from_slice(payload)?;
    let d = event.data;

    let requeued = repo
        .requeue_after_drop(d.tenant_id, d.driver_id, d.route_id, &d.shipment_ids, d.dropped_at)
        .await?;

    for shipment_id in requeued {
        tracing::info!(
            shipment_id = %shipment_id, dropped_by = %d.driver_id, reason = %d.reason_code,
            "Dropped shipment back in the queue"
        );
        // Best-effort: the sweep picks up anything the broadcast cannot place.
        if let Err(e) = offer_service.broadcast(TenantId::from_uuid(d.tenant_id), shipment_id).await {
            tracing::info!(
                shipment_id = %shipment_id, err = %e,
                "Re-broadcast after drop found no one — left pending for the sweep"
            );
        }
    }
    Ok(())
}
