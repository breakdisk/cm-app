use std::{net::SocketAddr, sync::Arc};

use rdkafka::{consumer::StreamConsumer, ClientConfig};
use sqlx::postgres::PgPoolOptions;

use anyhow::Context;
use logisticos_auth::jwt::JwtService;
use crate::{
    api::http,
    application::{agent::AgentRunner, triggers::run_trigger_consumer},
    config::Config,
    infrastructure::{
        claude::ClaudeClient,
        db::{PgSessionRepository, SessionRepository},
        tools::{ServiceUrls, ToolRegistry},
    },
    AppState,
};

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load()?;
    logisticos_tracing::init(logisticos_tracing::TracingConfig {
        service_name: "ai-layer",
        env: &cfg.app.env,
        otlp_endpoint: None,
        log_level: None,
    })?;

    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .after_connect(|conn, _meta| Box::pin(async move {
            sqlx::query("SET search_path TO ai_layer, public")
                .execute(&mut *conn)
                .await?;
            Ok(())
        }))
        .connect(&cfg.database.url)
        .await?;

    logisticos_common::migrations::run(&pool, "ai_layer", &sqlx::migrate!("./migrations")).await?;

    // Claude API client
    let claude = Arc::new(ClaudeClient::new(cfg.anthropic.api_key.clone()));

    // Tool registry — all available MCP-style tools agents can call
    let tools = Arc::new(ToolRegistry::new(ServiceUrls {
        dispatch:     cfg.services.dispatch_url.clone(),
        order_intake: cfg.services.order_intake_url.clone(),
        driver_ops:   cfg.services.driver_ops_url.clone(),
        payments:     cfg.services.payments_url.clone(),
        engagement:   cfg.services.engagement_url.clone(),
        analytics:    cfg.services.analytics_url.clone(),
        cdp:          cfg.services.cdp_url.clone(),
        hub_ops:      cfg.services.hub_ops_url.clone(),
        fleet:        cfg.services.fleet_url.clone(),
    }));

    // Session repository
    let session_repo: Arc<dyn SessionRepository> = Arc::new(PgSessionRepository::new(pool));

    // Agent runner
    let runner = Arc::new(AgentRunner::new(
        claude,
        tools.clone(),
        session_repo.clone(),
    ));

    // Kafka trigger consumer — autonomously activates agents on domain events
    let consumer: Arc<StreamConsumer> = Arc::new(
        ClientConfig::new()
            .set("bootstrap.servers", &cfg.kafka.brokers)
            .set("group.id", &cfg.kafka.group_id)
            .set("auto.offset.reset", "earliest")
            .set("enable.auto.commit", "false")
            .create()?,
    );

    let trigger_runner = runner.clone();
    tokio::spawn(async move {
        run_trigger_consumer(consumer, trigger_runner).await;
    });

    let jwt_secret = std::env::var("AUTH__JWT_SECRET")
        .context("AUTH__JWT_SECRET env var not set")?;
    let jwt = Arc::new(JwtService::new(&jwt_secret, 3600, 86400));

    let state = AppState { runner, session_repo, tools, jwt: Arc::clone(&jwt) };

    let app = http::router()
        .layer(axum::middleware::from_fn_with_state(jwt, logisticos_auth::middleware::require_auth))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.app.host, cfg.app.port).parse()?;
    tracing::info!(addr = %addr, "ai-layer (agentic runtime) listening");

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
    tracing::info!("ai-layer shutdown");
}
