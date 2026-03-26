use std::{net::SocketAddr, sync::Arc};
use sqlx::postgres::PgPoolOptions;

use crate::{api::http, application::services::FleetService, config::Config, infrastructure::db::PgVehicleRepository, AppState};

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load()?;
    logisticos_tracing::init(&cfg.app.env, "fleet")?;

    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .connect(&cfg.database.url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    let vehicle_repo = Arc::new(PgVehicleRepository::new(pool));
    let fleet_svc    = Arc::new(FleetService::new(vehicle_repo));

    let state = AppState { fleet_svc };

    let app = http::router()
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.app.host, cfg.app.port).parse()?;
    tracing::info!(addr = %addr, "fleet service listening");

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
    tracing::info!("fleet shutdown");
}
