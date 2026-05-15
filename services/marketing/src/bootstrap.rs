use std::{net::SocketAddr, sync::Arc};
use rdkafka::{producer::FutureProducer, ClientConfig};
use sqlx::postgres::PgPoolOptions;

use anyhow::Context;
use logisticos_auth::jwt::JwtService;
use crate::{
    api::http,
    application::services::{CampaignService, EventPublisher},
    config::Config,
    infrastructure::db::PgCampaignRepository,
    AppState,
};

// ---------------------------------------------------------------------------
// Kafka-backed event publisher
// ---------------------------------------------------------------------------

struct KafkaPublisher {
    producer: FutureProducer,
}

#[async_trait::async_trait]
impl EventPublisher for KafkaPublisher {
    async fn publish(&self, topic: &str, key: &str, payload: &[u8]) -> anyhow::Result<()> {
        use rdkafka::producer::FutureRecord;
        use std::time::Duration;
        self.producer
            .send(
                FutureRecord::to(topic).key(key).payload(payload),
                Duration::from_secs(5),
            )
            .await
            .map_err(|(e, _)| anyhow::anyhow!("Kafka publish error: {}", e))?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Bootstrap
// ---------------------------------------------------------------------------

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load()?;
    let otlp = std::env::var("OTLP_ENDPOINT").ok();
    logisticos_tracing::init(logisticos_tracing::TracingConfig {
        service_name: "marketing",
        env: &cfg.app.env,
        otlp_endpoint: otlp.as_deref(),
        log_level: None,
    })?;

    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .after_connect(|conn, _meta| Box::pin(async move {
            sqlx::query("SET search_path TO marketing, public")
                .execute(&mut *conn)
                .await?;
            Ok(())
        }))
        .connect(&cfg.database.url)
        .await?;

    logisticos_common::migrations::run(&pool, "marketing", &sqlx::migrate!("./migrations")).await?;

    let producer: FutureProducer = ClientConfig::new()
        .set("bootstrap.servers", &cfg.kafka.brokers)
        .set("message.timeout.ms", "5000")
        .create()?;

    let campaign_repo = Arc::new(PgCampaignRepository::new(pool));
    let publisher     = Arc::new(KafkaPublisher { producer });
    let campaign_svc  = Arc::new(CampaignService::new(campaign_repo, publisher));

    // Spawn a background consumer that receives CAMPAIGN_COMPLETED events from
    // the engagement service and flips the campaign status to Completed.
    // Uses a distinct group ID suffix to avoid stealing partition assignments
    // from the main service consumer group.
    let completion_svc      = campaign_svc.clone();
    let completion_brokers  = cfg.kafka.brokers.clone();
    let completion_group_id = format!("{}-completion", cfg.kafka.group_id);
    let (completion_shutdown_tx, completion_shutdown_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        if let Err(e) = run_completion_consumer(
            completion_brokers,
            completion_group_id,
            completion_svc,
            completion_shutdown_rx,
        ).await {
            tracing::error!(err = %e, "Campaign completion consumer crashed");
        }
    });

    // Spawn a background poller that auto-activates campaigns whose scheduled_at has elapsed.
    // Runs every 60 seconds; shares the same CampaignService instance via Arc.
    let poller_svc = campaign_svc.clone();
    let (poller_shutdown_tx, poller_shutdown_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        run_scheduled_poller(poller_svc, poller_shutdown_rx).await;
    });

    let jwt_secret = std::env::var("AUTH__JWT_SECRET")
        .context("AUTH__JWT_SECRET env var not set")?;
    let jwt = Arc::new(JwtService::new(&jwt_secret, 3600, 86400));

    let state = AppState { campaign_svc, jwt: Arc::clone(&jwt) };

    let app = http::router()
        .layer(axum::middleware::from_fn_with_state(jwt, logisticos_auth::middleware::require_auth))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.app.host, cfg.app.port).parse()?;
    tracing::info!(addr = %addr, "marketing service listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    // Signal background tasks to stop after the HTTP server drains.
    completion_shutdown_tx.send(true).ok();
    poller_shutdown_tx.send(true).ok();

    Ok(())
}

// ---------------------------------------------------------------------------
// CAMPAIGN_COMPLETED consumer — flips campaign status to Completed
// ---------------------------------------------------------------------------

async fn run_completion_consumer(
    brokers:      String,
    group_id:     String,
    campaign_svc: Arc<CampaignService>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> anyhow::Result<()> {
    use rdkafka::{consumer::{CommitMode, Consumer, StreamConsumer}, ClientConfig, Message};
    use logisticos_events::topics;

    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", &brokers)
        .set("group.id", &group_id)
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()?;

    consumer.subscribe(&[topics::CAMPAIGN_COMPLETED])?;
    tracing::info!(group_id, "marketing completion consumer subscribed to CAMPAIGN_COMPLETED");

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow_and_update() {
                    tracing::info!("Marketing completion consumer shutting down");
                    break;
                }
            }
            result = consumer.recv() => {
                match result {
                    Ok(msg) => {
                        if let Some(bytes) = msg.payload() {
                            if let Ok(json) = serde_json::from_slice::<serde_json::Value>(bytes) {
                                handle_campaign_completed(&json, &campaign_svc).await;
                            }
                        }
                        consumer.commit_message(&msg, CommitMode::Async).ok();
                    }
                    Err(e) => {
                        tracing::error!(err = %e, "Campaign completion consumer Kafka error");
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                }
            }
        }
    }
    Ok(())
}

async fn handle_campaign_completed(
    payload:      &serde_json::Value,
    campaign_svc: &CampaignService,
) {
    let Some(id_str) = payload["campaign_id"].as_str() else {
        tracing::warn!("CAMPAIGN_COMPLETED missing campaign_id — skipping");
        return;
    };
    let Ok(campaign_id) = id_str.parse::<uuid::Uuid>() else {
        tracing::warn!(campaign_id = id_str, "CAMPAIGN_COMPLETED invalid campaign_id UUID");
        return;
    };
    let total_sent      = payload["total_sent"].as_u64().unwrap_or(0);
    let total_delivered = payload["total_delivered"].as_u64().unwrap_or(0);
    let total_failed    = payload["total_failed"].as_u64().unwrap_or(0);

    match campaign_svc.complete(campaign_id, total_sent, total_delivered, total_failed).await {
        Ok(c) => tracing::info!(
            campaign_id = %campaign_id,
            total_sent,
            total_failed,
            status = ?c.status,
            "Campaign marked as completed"
        ),
        Err(e) => tracing::error!(
            campaign_id = %campaign_id,
            err = %e,
            "Failed to mark campaign as completed"
        ),
    }
}

// ---------------------------------------------------------------------------
// Scheduled campaign poller — fires every 60 s, activates due campaigns
// ---------------------------------------------------------------------------

async fn run_scheduled_poller(
    campaign_svc: Arc<CampaignService>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    use tokio::time::{interval, MissedTickBehavior};
    let mut ticker = interval(std::time::Duration::from_secs(60));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow_and_update() {
                    tracing::info!("Marketing scheduled poller shutting down");
                    break;
                }
            }
            _ = ticker.tick() => {
                match campaign_svc.activate_due_campaigns().await {
                    Ok(0)  => {}  // nothing due — quiet tick
                    Ok(n)  => tracing::info!(activated = n, "Scheduled poller activated due campaigns"),
                    Err(e) => tracing::error!(err = %e, "Scheduled poller error"),
                }
            }
        }
    }
}

async fn shutdown_signal() {
    use tokio::signal;
    let ctrl_c = async { signal::ctrl_c().await.expect("ctrl-c") };
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate()).expect("SIGTERM").recv().await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
    tracing::info!("marketing shutdown");
}
