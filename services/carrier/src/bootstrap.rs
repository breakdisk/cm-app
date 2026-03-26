use std::{net::SocketAddr, sync::Arc};
use sqlx::postgres::PgPoolOptions;
use crate::{api::http, application::services::CarrierService, config::Config, infrastructure::db::PgCarrierRepository, AppState};

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load()?;
    logisticos_tracing::init(&cfg.app.env, "carrier")?;

    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .connect(&cfg.database.url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    let carrier_repo = Arc::new(PgCarrierRepository::new(pool));
    let carrier_svc  = Arc::new(CarrierService::new(carrier_repo));

    let state = AppState { carrier_svc };

    let app = http::router()
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.app.host, cfg.app.port).parse()?;
    tracing::info!(addr = %addr, "carrier service listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).with_graceful_shutdown(shutdown_signal()).await?;
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
}
