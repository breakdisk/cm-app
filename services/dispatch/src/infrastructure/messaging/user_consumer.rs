//! Consumes USER_CREATED events → inserts drivers into driver_profiles.

use logisticos_events::{envelope::Event, payloads::UserCreated, topics};
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    config::ClientConfig,
    message::Message,
};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::watch;

use crate::infrastructure::db::driver_profiles_repo::{DriverProfileRow, DriverProfilesRepository, PgDriverProfilesRepository};

pub async fn start_user_consumer(
    brokers: &str,
    group_id: &str,
    pool: PgPool,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", &format!("{}-users", group_id))
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()?;

    consumer.subscribe(&[topics::USER_CREATED])?;
    let repo = Arc::new(PgDriverProfilesRepository::new(pool));

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow_and_update() {
                    tracing::info!("User consumer shutting down");
                    break;
                }
            }
            result = consumer.recv() => {
                match result {
                    Ok(msg) => {
                        if let Some(payload) = msg.payload() {
                            if let Err(e) = handle_user_created(payload, &*repo).await {
                                tracing::warn!(err = %e, "user consumer: handler error (skipping)");
                            }
                        }
                        consumer.commit_message(&msg, CommitMode::Async).ok();
                    }
                    Err(e) => {
                        tracing::error!(err = %e, "user consumer: recv error");
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                }
            }
        }
    }

    Ok(())
}

async fn handle_user_created(
    payload: &[u8],
    repo: &dyn DriverProfilesRepository,
) -> anyhow::Result<()> {
    let event: Event<UserCreated> = serde_json::from_slice(payload)?;
    let d = event.data;

    // Only cache driver-role users
    if !d.roles.iter().any(|r| r == "driver") {
        return Ok(());
    }

    let row = DriverProfileRow {
        id:         d.user_id,
        tenant_id:  d.tenant_id,
        email:      d.email,
        first_name: String::new(),  // UserCreated doesn't carry name fields
        last_name:  String::new(),
    };
    repo.upsert(&row).await?;
    tracing::info!(user_id = %d.user_id, "Driver profile cached in dispatch");
    Ok(())
}
