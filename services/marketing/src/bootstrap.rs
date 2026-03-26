use std::{net::SocketAddr, sync::Arc};
use rdkafka::{producer::FutureProducer, ClientConfig};
use sqlx::postgres::PgPoolOptions;

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
    logisticos_tracing::init(&cfg.app.env, "marketing")?;

    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .connect(&cfg.database.url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    let producer: FutureProducer = ClientConfig::new()
        .set("bootstrap.servers", &cfg.kafka.brokers)
        .set("message.timeout.ms", "5000")
        .create()?;

    let campaign_repo = Arc::new(PgCampaignRepository::new(pool));
    let publisher     = Arc::new(KafkaPublisher { producer });
    let campaign_svc  = Arc::new(CampaignService::new(campaign_repo, publisher));

    let state = AppState { campaign_svc };

    let app = http::router()
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.app.host, cfg.app.port).parse()?;
    tracing::info!(addr = %addr, "marketing service listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
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
