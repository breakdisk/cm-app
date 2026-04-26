use std::{net::SocketAddr, sync::Arc};
use anyhow::Context;
use sqlx::postgres::PgPoolOptions;
use logisticos_auth::jwt::JwtService;
use crate::{api::http, application::services::CarrierService, config::Config, infrastructure::db::PgCarrierRepository, AppState};

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load()?;
    let otlp = std::env::var("OTLP_ENDPOINT").ok();
    logisticos_tracing::init(logisticos_tracing::TracingConfig {
        service_name: "carrier",
        env: &cfg.app.env,
        otlp_endpoint: otlp.as_deref(),
        log_level: None,
    })?;

    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .after_connect(|conn, _meta| Box::pin(async move {
            sqlx::query("SET search_path TO carrier, public")
                .execute(&mut *conn)
                .await?;
            Ok(())
        }))
        .connect(&cfg.database.url)
        .await?;

    logisticos_common::migrations::run(&pool, "carrier", &sqlx::migrate!("./migrations")).await?;

    let carrier_repo = Arc::new(PgCarrierRepository::new(pool));
    let carrier_svc  = Arc::new(CarrierService::new(carrier_repo));

    let jwt_secret = std::env::var("AUTH__JWT_SECRET")
        .context("AUTH__JWT_SECRET env var not set")?;
    let jwt = Arc::new(JwtService::new(&jwt_secret, 3600, 86400));

    let state = AppState { carrier_svc, jwt: Arc::clone(&jwt) };

    // Mount require_auth ahead of the carrier routes so AuthClaims extracts
    // properly. Without this layer every handler 500s with
    // "Auth middleware not mounted" — see libs/auth/src/middleware.rs.
    let app = http::router()
        .layer(axum::middleware::from_fn_with_state(jwt, logisticos_auth::middleware::require_auth))
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
