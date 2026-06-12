//! Consumes ASSIGNMENT_REJECTED events → maintains drivers.decline_count and
//! applies the automatic ban at the decline threshold.
//!
//! Decline accountability rule: a driver who declines `DECLINE_BAN_THRESHOLD`
//! assignments is deactivated (`is_active = false`, `status = 'offline'`).
//! Deactivated drivers drop out of dispatch's `find_available_near` pool
//! automatically (it filters on `is_active`). Reinstatement is a manual ops
//! action: PATCH /v1/drivers/:id with is_active=true — ops should reset
//! decline_count alongside, otherwise the next decline re-bans immediately.

use std::sync::Arc;
use logisticos_events::{envelope::Event, payloads::AssignmentRejected, topics};
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    config::ClientConfig,
    Message,
};
use sqlx::PgPool;
use tokio::sync::watch;
use crate::infrastructure::external::FcmClient;

/// Number of declines after which a driver is automatically deactivated.
pub const DECLINE_BAN_THRESHOLD: i32 = 20;

pub async fn start_assignment_rejected_consumer(
    brokers: &str,
    group_id: &str,
    pool: PgPool,
    fcm: Option<Arc<FcmClient>>,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", &format!("{}-assignment-rejected", group_id))
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()?;

    consumer.subscribe(&[topics::ASSIGNMENT_REJECTED])?;

    tracing::info!("assignment-rejected consumer: subscribed to {}", topics::ASSIGNMENT_REJECTED);

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    tracing::info!("assignment-rejected consumer: shutdown signal received");
                    break;
                }
            }
            result = consumer.recv() => {
                match result {
                    Ok(msg) => {
                        if let Some(payload) = msg.payload() {
                            if let Err(e) = handle_assignment_rejected(payload, &pool, fcm.clone()).await {
                                tracing::warn!(err = %e, "assignment-rejected consumer: handler error (skipping)");
                            }
                        }
                        consumer.commit_message(&msg, CommitMode::Async).ok();
                    }
                    Err(e) => {
                        tracing::error!(err = %e, "assignment-rejected consumer: recv error");
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                }
            }
        }
    }
    Ok(())
}

async fn handle_assignment_rejected(
    payload: &[u8],
    pool: &PgPool,
    fcm: Option<Arc<FcmClient>>,
) -> anyhow::Result<()> {
    let event: Event<AssignmentRejected> = serde_json::from_slice(payload)?;
    let r = event.data;

    // Increment the decline counter. driver_id in the event is the identity
    // user UUID (claims.user_id), which is what drivers.user_id indexes on.
    let new_count: Option<i32> = sqlx::query_scalar(
        r#"UPDATE driver_ops.drivers
           SET decline_count = decline_count + 1,
               updated_at    = NOW()
           WHERE user_id = $1
           RETURNING decline_count"#,
    )
    .bind(r.driver_id)
    .fetch_optional(pool)
    .await?;

    let Some(new_count) = new_count else {
        tracing::warn!(
            driver_id = %r.driver_id,
            "assignment-rejected consumer: no driver row for user_id — decline not counted"
        );
        return Ok(());
    };

    tracing::info!(
        driver_id     = %r.driver_id,
        assignment_id = %r.assignment_id,
        decline_count = new_count,
        reason        = %r.reason,
        "assignment-rejected consumer: decline recorded"
    );

    if new_count >= DECLINE_BAN_THRESHOLD {
        // Automatic ban: deactivate + force offline. is_active=false removes
        // the driver from dispatch's availability pool; status='offline'
        // keeps the app UI consistent on next sync.
        sqlx::query(
            r#"UPDATE driver_ops.drivers
               SET is_active  = false,
                   status     = 'offline',
                   updated_at = NOW()
               WHERE user_id = $1"#,
        )
        .bind(r.driver_id)
        .execute(pool)
        .await?;

        tracing::warn!(
            driver_id     = %r.driver_id,
            decline_count = new_count,
            threshold     = DECLINE_BAN_THRESHOLD,
            "assignment-rejected consumer: driver BANNED — decline threshold reached"
        );

        // Best-effort push so the driver knows why work stopped arriving.
        if let Some(fcm) = fcm {
            let driver_user_id = r.driver_id;
            tokio::spawn(async move {
                fcm.notify_driver_instruction(
                    driver_user_id,
                    "account_suspended",
                    "Your account has been deactivated after reaching the decline limit. \
                     Contact your fleet manager to be reinstated.",
                ).await;
            });
        }
    }

    Ok(())
}
