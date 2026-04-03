use std::{net::SocketAddr, sync::Arc};

use rdkafka::{consumer::StreamConsumer, ClientConfig};
use sqlx::postgres::PgPoolOptions;

use crate::{
    api::http,
    application::{handlers, services::TrackingService},
    config::Config,
    infrastructure::db::PgTrackingRepository,
    AppState,
};

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load()?;
    logisticos_tracing::init(logisticos_tracing::TracingConfig {
        service_name: "delivery-experience",
        env: &cfg.app.env,
        otlp_endpoint: None,
        log_level: None,
    })?;

    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .connect(&cfg.database.url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    let tracking_repo = Arc::new(PgTrackingRepository::new(pool.clone()));
    let tracking_svc  = Arc::new(TrackingService::new(tracking_repo.clone()));

    // Kafka consumer for shipment lifecycle projections.
    let consumer: Arc<StreamConsumer> = Arc::new(
        ClientConfig::new()
            .set("bootstrap.servers", &cfg.kafka.brokers)
            .set("group.id", &cfg.kafka.group_id)
            .set("auto.offset.reset", "earliest")
            .set("enable.auto.commit", "false")
            .create()?,
    );

    let consumer_repo = tracking_repo.clone() as Arc<dyn crate::domain::repositories::TrackingRepository>;
    tokio::spawn(async move {
        handlers::run_consumer(consumer, consumer_repo).await;
    });

    let state = AppState { tracking_svc };

    use tower_http::cors::CorsLayer;
    use axum::http::{HeaderName, HeaderValue, Method};

    let cors = CorsLayer::new()
        .allow_origin([
            "http://localhost:3001".parse::<HeaderValue>().unwrap(),
            "http://localhost:3002".parse::<HeaderValue>().unwrap(),
            "http://localhost:3003".parse::<HeaderValue>().unwrap(),
            "http://localhost:8083".parse::<HeaderValue>().unwrap(),
        ])
        .allow_methods([
            Method::GET, Method::POST, Method::PUT,
            Method::PATCH, Method::DELETE, Method::OPTIONS,
        ])
        .allow_headers([
            HeaderName::from_static("content-type"),
            HeaderName::from_static("authorization"),
        ]);

    let app = http::router()
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.app.host, cfg.app.port).parse()?;
    tracing::info!(addr = %addr, "delivery-experience service listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    use tokio::signal;
    let ctrl_c = async { signal::ctrl_c().await.expect("ctrl-c handler") };
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("delivery-experience shutdown signal received");
}
