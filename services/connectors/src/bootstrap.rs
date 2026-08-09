use std::sync::Arc;

use anyhow::Context;
use sqlx::postgres::PgPoolOptions;

use logisticos_auth::jwt::JwtService;

use crate::{
    api::http::{AppState, router},
    application::ConnectorService,
    config::Config,
    domain::repositories::CredentialsRepository,
    infrastructure::{
        db::PgCredentialsRepository,
        omnideliv_client::OmniDelivClient,
        order_intake_client::OrderIntakeClient,
    },
};

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load().context("Failed to load connectors config")?;

    logisticos_tracing::init(logisticos_tracing::TracingConfig {
        service_name: "connectors",
        env:          &cfg.app.env,
        otlp_endpoint: None,
        log_level:     None,
    })?;

    tracing::info!(env = %cfg.app.env, "connectors service starting");

    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .after_connect(|conn, _meta| Box::pin(async move {
            sqlx::query("SET search_path TO connectors, public")
                .execute(&mut *conn)
                .await?;
            Ok(())
        }))
        .connect(&cfg.database.url)
        .await
        .context("Failed to connect to PostgreSQL")?;

    // Ensure schema exists, then run migrations.
    sqlx::query("CREATE SCHEMA IF NOT EXISTS connectors")
        .execute(&pool)
        .await
        .context("Failed to create connectors schema")?;

    logisticos_common::migrations::run(
        &pool,
        "connectors",
        &sqlx::migrate!("./migrations"),
    )
    .await
    .context("Connectors database migration failed")?;

    // Wire dependencies.
    let creds_repo:   Arc<dyn CredentialsRepository> =
        Arc::new(PgCredentialsRepository::new(pool.clone()));
    let order_intake  = Arc::new(OrderIntakeClient::new(cfg.order_intake.internal_url.clone()));
    let connector_svc = Arc::new(ConnectorService::new(creds_repo, order_intake));

    let jwt = Arc::new(JwtService::new(
        &cfg.auth.jwt_secret,
        3600,
        86400,
    ));

    // Public URL used to construct webhook URLs shown to merchants.
    // Defaults to http://localhost:{port} in dev; override via PUBLIC_URL env var.
    let public_url = std::env::var("PUBLIC_URL")
        .unwrap_or_else(|_| format!("http://{}:{}", cfg.app.host, cfg.app.port));

    // Absent when this deployment runs no OmniDeliv tier. Logged either way:
    // "catalog sync is off" needs to be visible at boot, not discovered by a
    // merchant pressing a button and getting a 501.
    let omnideliv = cfg.omnideliv.as_ref().map(|c| {
        tracing::info!(url = %c.internal_url, "catalog sync enabled");
        Arc::new(OmniDelivClient::new(c.internal_url.clone(), Arc::clone(&jwt)))
    });
    if omnideliv.is_none() {
        tracing::info!("catalog sync disabled — OMNIDELIV__INTERNAL_URL is not set");
    }

    let state = AppState {
        svc:        connector_svc,
        jwt:        Arc::clone(&jwt),
        public_url,
        omnideliv,
        http:       reqwest::Client::new(),
    };

    let app = router(state)
        .route("/health", axum::routing::get(health_handler))
        .layer(tower_http::trace::TraceLayer::new_for_http());

    let addr = format!("{}:{}", cfg.app.host, cfg.app.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("Failed to bind to {addr}"))?;

    tracing::info!(addr = %addr, "connectors service listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("Connectors server error")?;

    Ok(())
}

async fn health_handler() -> impl axum::response::IntoResponse {
    axum::Json(serde_json::json!({"status": "ok", "service": "connectors"}))
}

async fn shutdown_signal() {
    let ctrl_c = async { tokio::signal::ctrl_c().await.expect("ctrl_c handler") };
    #[cfg(unix)]
    let term = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("sigterm handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = term   => {},
    }
}
