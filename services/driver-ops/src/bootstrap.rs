use std::sync::Arc;
use sqlx::postgres::PgPoolOptions;
use anyhow::Context;
use tokio::sync::broadcast;
use crate::config::Config;
use crate::application::services::{DriverService, TaskService, LocationService};
use crate::infrastructure::db::{PgDriverRepository, PgTaskRepository, PgLocationRepository};
use crate::api::http::{router, AppState, LocationBroadcast};
use logisticos_auth::jwt::JwtService;
use logisticos_events::producer::KafkaProducer;

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load().context("Failed to load driver-ops config")?;

    logisticos_tracing::init(logisticos_tracing::TracingConfig {
        service_name: "driver-ops".to_string(),
        env: cfg.app.env.clone(),
        otlp_endpoint: std::env::var("OTLP_ENDPOINT").ok(),
    })?;

    tracing::info!(env = %cfg.app.env, "driver-ops service starting");

    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(&cfg.database.url)
        .await
        .context("Failed to connect to PostgreSQL")?;

    sqlx::migrate!("./migrations").run(&pool).await
        .context("driver-ops migration failed")?;

    let kafka = Arc::new(
        KafkaProducer::new(&cfg.kafka.brokers)
            .context("Failed to connect Kafka")?
    );

    let jwt_secret = std::env::var("AUTH__JWT_SECRET")
        .context("AUTH__JWT_SECRET not set")?;
    let jwt = Arc::new(JwtService::new(jwt_secret, 3600, 86400));

    // Repositories
    let driver_repo   = Arc::new(PgDriverRepository::new(pool.clone()));
    let task_repo     = Arc::new(PgTaskRepository::new(pool.clone()));
    let location_repo = Arc::new(PgLocationRepository::new(pool.clone()));

    // Application services
    let driver_service = Arc::new(DriverService::new(
        Arc::clone(&driver_repo) as _,
    ));
    let task_service = Arc::new(TaskService::new(
        Arc::clone(&task_repo) as _,
        Arc::clone(&driver_repo) as _,
        Arc::clone(&kafka),
    ));
    let location_service = Arc::new(LocationService::new(
        Arc::clone(&driver_repo) as _,
        Arc::clone(&location_repo) as _,
        Arc::clone(&kafka),
    ));

    // Broadcast channel for WebSocket location streaming (capacity 512)
    let (location_tx, _) = broadcast::channel::<LocationBroadcast>(512);

    let state = Arc::new(AppState {
        driver_service,
        task_service,
        location_service,
        jwt: Arc::clone(&jwt),
        location_tx,
    });

    let app = router(state);

    let addr = format!("{}:{}", cfg.app.host, cfg.app.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("Failed to bind to {addr}"))?;

    tracing::info!(addr = %addr, "driver-ops service listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("driver-ops server error")?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async { tokio::signal::ctrl_c().await.expect("ctrl_c") };
    #[cfg(unix)]
    let term = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("sigterm").recv().await;
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
}
