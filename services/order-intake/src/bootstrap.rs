use std::{net::SocketAddr, sync::Arc};

use anyhow::Context;
use sqlx::postgres::PgPoolOptions;
use logisticos_auth::jwt::JwtService;

use crate::{
    api::http::{AppState, router},
    application::{
        queries::ShipmentQueryService,
        services::shipment_service::ShipmentService,
    },
    config::Config,
    infrastructure::{
        db::PgShipmentRepository,
        external::PassthroughNormalizer,
        messaging::{KafkaEventPublisher, status_consumer::start_status_consumer},
    },
};

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load()?;
    logisticos_tracing::init(logisticos_tracing::TracingConfig {
        service_name:  "order-intake",
        env:           &cfg.app.env,
        otlp_endpoint: None,
        log_level:     None,
    })?;

    // Database
    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .after_connect(|conn, _meta| Box::pin(async move {
            sqlx::query("SET search_path TO order_intake, public")
                .execute(&mut *conn)
                .await?;
            Ok(())
        }))
        .connect(&cfg.database.url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    // Infrastructure adapters
    let repo       = Arc::new(PgShipmentRepository { pool: pool.clone() });
    let publisher  = Arc::new(KafkaEventPublisher::new(&cfg.kafka.brokers)?);
    let normalizer = Arc::new(PassthroughNormalizer);

    // Spawn Kafka status consumer in background
    let pool_for_consumer   = pool.clone();
    let brokers_for_consumer = cfg.kafka.brokers.clone();
    let group_for_consumer   = cfg.kafka.group_id.clone();
    tokio::spawn(async move {
        if let Err(e) = start_status_consumer(
            &brokers_for_consumer,
            &group_for_consumer,
            pool_for_consumer,
        )
        .await
        {
            tracing::error!("Status consumer error: {e}");
        }
    });

    let jwt_secret = std::env::var("AUTH__JWT_SECRET").context("AUTH__JWT_SECRET not set")?;
    let jwt = Arc::new(JwtService::new(&jwt_secret, 3600, 86400));

    // Application services
    let svc   = Arc::new(ShipmentService::new(repo.clone(), publisher, normalizer));
    let query = Arc::new(ShipmentQueryService::new(repo.clone()));

    // Axum router
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

    let state = AppState { svc, query, jwt };
    let app = router(state)
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .layer(cors);

    let addr: SocketAddr = format!("{}:{}", cfg.app.host, cfg.app.port).parse()?;
    tracing::info!(addr = %addr, "order-intake service listening");

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
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("SIGTERM")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
}
