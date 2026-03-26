use std::{net::SocketAddr, sync::Arc};

use sqlx::postgres::PgPoolOptions;

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
        messaging::KafkaEventPublisher,
    },
};

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load()?;
    logisticos_tracing::init(&cfg.app.env, "order-intake")?;

    // Database
    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .connect(&cfg.database.url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    // Infrastructure adapters
    let repo       = Arc::new(PgShipmentRepository { pool: pool.clone() });
    let publisher  = Arc::new(KafkaEventPublisher::new(&cfg.kafka.brokers)?);
    let normalizer = Arc::new(PassthroughNormalizer);

    // Application services
    let svc   = Arc::new(ShipmentService::new(repo.clone(), publisher, normalizer));
    let query = Arc::new(ShipmentQueryService::new(repo.clone()));

    // Axum router
    let state = AppState { svc, query };
    let app   = router(state)
        .layer(tower_http::trace::TraceLayer::new_for_http());

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
