use std::sync::Arc;
use anyhow::Context;
use sqlx::postgres::PgPoolOptions;
use tokio::sync::watch;

use logisticos_auth::jwt::JwtService;

use crate::{
    api::http,
    application::services::WebhookService,
    config::Config,
    infrastructure::{
        db::{PgDeliveryRepository, PgWebhookRepository},
        messaging::dispatcher,
    },
    AppState,
};

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load().context("Failed to load webhooks config")?;

    let otlp = std::env::var("OTLP_ENDPOINT").ok();
    logisticos_tracing::init(logisticos_tracing::TracingConfig {
        service_name: "webhooks",
        env: &cfg.app.env,
        otlp_endpoint: otlp.as_deref(),
        log_level: None,
    })?;

    tracing::info!(env = %cfg.app.env, "webhooks service starting");

    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .after_connect(|conn, _meta| Box::pin(async move {
            // Default search_path. Tenant scoping is RLS-driven; the dispatcher
            // operates without app.tenant_id and relies on the trait method
            // arguments to constrain queries.
            sqlx::query("SET search_path TO webhooks, public")
                .execute(&mut *conn)
                .await?;
            Ok(())
        }))
        .connect(&cfg.database.url)
        .await
        .context("Failed to connect to PostgreSQL")?;

    sqlx::query("CREATE SCHEMA IF NOT EXISTS webhooks").execute(&pool).await
        .context("Failed to create webhooks schema")?;

    logisticos_common::migrations::run(&pool, "webhooks", &sqlx::migrate!("./migrations"))
        .await
        .context("Webhooks database migration failed")?;

    let webhook_repo  = Arc::new(PgWebhookRepository::new(pool.clone()));
    let delivery_repo = Arc::new(PgDeliveryRepository::new(pool.clone()));
    let webhook_svc   = Arc::new(WebhookService::new(webhook_repo.clone()));

    let jwt_secret = std::env::var("AUTH__JWT_SECRET")
        .context("AUTH__JWT_SECRET env var not set")?;
    let jwt = Arc::new(JwtService::new(&jwt_secret, 3600, 86400));

    let state = AppState {
        webhook_svc,
        jwt: Arc::clone(&jwt),
    };

    // Spawn the Kafka dispatcher in the background. Errors logged + retried
    // internally; we don't crash the HTTP server on transient consumer issues.
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let brokers      = cfg.kafka.brokers.clone();
    let group_id     = cfg.kafka.group_id.clone();
    let webhook_repo_for_dispatch  = Arc::clone(&webhook_repo) as Arc<dyn crate::domain::repositories::WebhookRepository>;
    let delivery_repo_for_dispatch = Arc::clone(&delivery_repo) as Arc<dyn crate::domain::repositories::DeliveryRepository>;
    tokio::spawn(async move {
        if let Err(e) = dispatcher::start(
            &brokers,
            &group_id,
            webhook_repo_for_dispatch,
            delivery_repo_for_dispatch,
            shutdown_rx,
        ).await {
            tracing::error!("webhook dispatcher crashed: {e}");
        }
    });

    // Protected webhook routes get the auth layer; health/ready don't.
    // Both halves share the same AppState via with_state at the top.
    let protected = http::router()
        .layer(axum::middleware::from_fn_with_state(jwt, logisticos_auth::middleware::require_auth));
    let app = axum::Router::new()
        .route("/health", axum::routing::get(http::health::health))
        .route("/ready",  axum::routing::get(http::health::ready))
        .merge(protected)
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);

    let addr = format!("{}:{}", cfg.app.host, cfg.app.port);
    let listener = tokio::net::TcpListener::bind(&addr).await
        .with_context(|| format!("Failed to bind to {addr}"))?;
    tracing::info!(addr = %addr, "webhooks service listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("Webhooks server error")?;

    if shutdown_tx.send(true).is_err() {
        tracing::warn!("Webhook dispatcher already stopped before shutdown signal");
    }
    Ok(())
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
