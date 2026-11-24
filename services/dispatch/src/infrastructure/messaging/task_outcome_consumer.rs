//! Consumes driver-ops task outcomes (PICKUP_COMPLETED, DELIVERY_COMPLETED,
//! DELIVERY_FAILED) and frees the rider when the route's work has ended.
//!
//! The topics carry driver-ops' `TaskCompleted` / `TaskFailed` payloads, not
//! libs/events' `DeliveryCompleted`, so a lenient local shape is read. The
//! route is resolved by task id, then shipment — never by the event's
//! `driver_id` (drivers.id vs user_id split-brain, ADR-0015).
//!
//! Writes only when `DISPATCH__CLOSE_ASSIGNMENTS` is on: an image that cannot
//! read `completed` would see the closed row as an active assignment.

use logisticos_events::{envelope::Event, topics};
use rdkafka::{
    config::ClientConfig,
    consumer::{CommitMode, Consumer, StreamConsumer},
    message::Message,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::watch;
use uuid::Uuid;

use crate::domain::repositories::DriverAssignmentRepository;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TaskOutcome {
    #[serde(default)]
    pub task_id: Option<Uuid>,
    #[serde(default)]
    pub shipment_id: Option<Uuid>,
}

pub async fn start_task_outcome_consumer(
    brokers: &str,
    group_id: &str,
    repo: Arc<dyn DriverAssignmentRepository>,
    enabled: bool,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", format!("{}-task-outcome", group_id))
        // A missed outcome keeps a rider busy; the sweep repairs it, but
        // reading from the start is cheap and the handler is replay-safe.
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()?;

    consumer.subscribe(&[
        topics::DELIVERY_COMPLETED,
        topics::DELIVERY_FAILED,
        topics::PICKUP_COMPLETED,
    ])?;

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow_and_update() {
                    tracing::info!("Task-outcome consumer shutting down");
                    break;
                }
            }
            result = consumer.recv() => {
                match result {
                    Ok(msg) => {
                        if let Some(payload) = msg.payload() {
                            // Retried in place: an uncommitted message is not
                            // redelivered until a restart. The handler is
                            // replay-safe.
                            let mut attempt = 0u32;
                            while let Err(e) = handle_task_outcome(payload, repo.as_ref(), enabled).await {
                                attempt += 1;
                                if attempt >= 6 {
                                    tracing::error!(offset = msg.offset(), err = %e,
                                        "task-outcome consumer: failed 6 times — the 90-second sweep will close the assignment");
                                    break;
                                }
                                tracing::warn!(attempt, err = %e, "task-outcome consumer: handler error — retrying");
                                tokio::time::sleep(std::time::Duration::from_secs(2u64.pow(attempt.min(5)))).await;
                            }
                        }
                        consumer.commit_message(&msg, CommitMode::Async).ok();
                    }
                    Err(e) => {
                        tracing::error!(err = %e, "task-outcome consumer: recv error");
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                }
            }
        }
    }

    Ok(())
}

/// Returns `true` if an assignment or route changed.
pub async fn handle_task_outcome(
    payload: &[u8],
    repo: &dyn DriverAssignmentRepository,
    enabled: bool,
) -> anyhow::Result<bool> {
    if !enabled {
        return Ok(false);
    }
    let event: Event<TaskOutcome> = serde_json::from_slice(payload)?;
    let tenant_id = event.tenant_id;
    let Some(route_id) = repo
        .find_route_for_outcome(tenant_id, event.data.task_id, event.data.shipment_id)
        .await?
    else {
        tracing::debug!(task_id = ?event.data.task_id, "task outcome matches no route — ignored");
        return Ok(false);
    };
    let changed = repo.close_if_finished(tenant_id, route_id).await?;
    if changed {
        tracing::info!(route_id = %route_id, "Work ended — assignment closed");
    }
    Ok(changed)
}
