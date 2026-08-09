use std::{net::SocketAddr, sync::Arc};
use sqlx::postgres::PgPoolOptions;

use crate::{api::http, application::services::FleetService, config::Config, infrastructure::db::PgVehicleRepository, AppState};

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load()?;
    let otlp = std::env::var("OTLP_ENDPOINT").ok();
    logisticos_tracing::init(logisticos_tracing::TracingConfig {
        service_name: "fleet",
        env: &cfg.app.env,
        otlp_endpoint: otlp.as_deref(),
        log_level: None,
    })?;

    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .after_connect(|conn, _meta| Box::pin(async move {
            sqlx::query("SET search_path TO fleet, public")
                .execute(&mut *conn)
                .await?;
            Ok(())
        }))
        .connect(&cfg.database.url)
        .await?;

    logisticos_common::migrations::run(&pool, "fleet", &sqlx::migrate!("./migrations")).await?;

    let vehicle_repo = Arc::new(PgVehicleRepository::new(pool));
    let fleet_svc    = Arc::new(FleetService::new(vehicle_repo));

    let state = AppState { fleet_svc };

    // Every handler in this service extracts `AuthClaims`, which reads `Claims`
    // out of the request extensions — and only `require_auth` puts them there.
    // Without this layer the extractor's rejection fires on *every* request:
    // 500 AUTH_NOT_CONFIGURED, "Auth middleware not mounted". The error message
    // was describing the deployment accurately.
    //
    // Nineteen of the twenty-one services mount this. fleet and
    // delivery-experience did not, and nothing caught it because fleet's test
    // suite has never compiled — see the [[test]] stanzas in Cargo.toml.
    let jwt_secret = std::env::var("AUTH__JWT_SECRET").unwrap_or_default();
    let jwt: logisticos_auth::middleware::AuthState =
        Arc::new(logisticos_auth::jwt::JwtService::new(&jwt_secret, 3600, 86_400));

    let app = http::router()
        .layer(axum::middleware::from_fn_with_state(
            jwt,
            logisticos_auth::middleware::require_auth,
        ))
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
