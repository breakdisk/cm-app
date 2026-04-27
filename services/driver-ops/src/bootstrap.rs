use std::sync::Arc;
use sqlx::postgres::PgPoolOptions;
use anyhow::Context;
use tokio::sync::{broadcast, watch};
use crate::config::Config;
use crate::application::services::{DriverService, TaskService, LocationService};
use crate::infrastructure::db::{PgDriverRepository, PgTaskRepository, PgLocationRepository};
use crate::infrastructure::messaging::start_task_consumer;
use crate::infrastructure::external::FcmClient;
use crate::api::http::{router, AppState, RosterEvent};
use logisticos_auth::jwt::JwtService;
use logisticos_events::producer::KafkaProducer;

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load().context("Failed to load driver-ops config")?;

    let otlp = std::env::var("OTLP_ENDPOINT").ok();
    logisticos_tracing::init(logisticos_tracing::TracingConfig {
        service_name: "driver-ops",
        env: &cfg.app.env,
        otlp_endpoint: otlp.as_deref(),
        log_level: None,
    })?;

    tracing::info!(env = %cfg.app.env, "driver-ops service starting");

    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .after_connect(|conn, _meta| Box::pin(async move {
            sqlx::query("SET search_path TO driver_ops, public")
                .execute(&mut *conn)
                .await?;
            Ok(())
        }))
        .connect(&cfg.database.url)
        .await
        .context("Failed to connect to PostgreSQL")?;

    logisticos_common::migrations::run(&pool, "driver_ops", &sqlx::migrate!("./migrations")).await
        .context("driver-ops migration failed")?;

    let kafka = Arc::new(
        KafkaProducer::new(&cfg.kafka.brokers)
            .context("Failed to connect Kafka")?
    );

    // Shutdown watch channel — broadcast to all background consumers.
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // Build FCM client — optional; logs a warning if env vars are not set.
    let fcm_client = FcmClient::new(
        cfg.identity.internal_url.clone(),
        cfg.fcm.project_id.clone(),
        &cfg.fcm.service_account_json,
    ).map(std::sync::Arc::new);
    if fcm_client.is_none() {
        tracing::warn!(
            "FCM push disabled: set IDENTITY__INTERNAL_URL, FCM__PROJECT_ID, \
             FCM__SERVICE_ACCOUNT_JSON to enable driver push notifications"
        );
    }

    // Spawn TASK_ASSIGNED consumer — creates driver_ops.tasks rows on dispatch.
    let pool_for_tasks    = pool.clone();
    let brokers_for_tasks = cfg.kafka.brokers.clone();
    let group_for_tasks   = cfg.kafka.group_id.clone();
    let shutdown_rx_tasks = shutdown_rx.clone();
    tokio::spawn(async move {
        if let Err(e) = start_task_consumer(
            &brokers_for_tasks,
            &group_for_tasks,
            pool_for_tasks,
            fcm_client,
            shutdown_rx_tasks,
        ).await {
            tracing::error!("Task consumer crashed: {e}");
        }
    });

    let jwt_secret = std::env::var("AUTH__JWT_SECRET")
        .context("AUTH__JWT_SECRET not set")?;
    let jwt = Arc::new(JwtService::new(&jwt_secret, 3600, 86400));

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

    // Broadcast channel for WebSocket roster streaming — location + status (capacity 512)
    let (roster_tx, _) = broadcast::channel::<RosterEvent>(512);

    let state = Arc::new(AppState {
        driver_service,
        task_service,
        location_service,
        jwt: Arc::clone(&jwt),
        roster_tx,
    });

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

    let app = router(state).layer(cors);

    let addr = format!("{}:{}", cfg.app.host, cfg.app.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("Failed to bind to {addr}"))?;

    tracing::info!(addr = %addr, "driver-ops service listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("driver-ops server error")?;

    // Signal background consumers to stop.
    if shutdown_tx.send(true).is_err() {
        tracing::warn!("Task consumer already stopped before shutdown signal");
    }

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
