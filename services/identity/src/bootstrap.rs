use std::sync::Arc;
use sqlx::postgres::PgPoolOptions;
use anyhow::Context;
use crate::config::Config;
use crate::application::services::{AuthService, TenantService, ApiKeyService};
use crate::infrastructure::db::{PgTenantRepository, PgUserRepository, PgApiKeyRepository};
use crate::api::http::{router, AppState};
use logisticos_auth::jwt::JwtService;
use logisticos_events::producer::KafkaProducer;

pub async fn run() -> anyhow::Result<()> {
    // 1. Load config from environment
    let cfg = Config::load().context("Failed to load config")?;

    // 2. Init structured tracing (JSON in prod, pretty in dev)
    logisticos_tracing::init(logisticos_tracing::TracingConfig {
        service_name: "identity".to_string(),
        env: cfg.app.env.clone(),
        otlp_endpoint: std::env::var("OTLP_ENDPOINT").ok(),
    })?;

    tracing::info!(env = %cfg.app.env, "identity service starting");

    // 3. Connect PostgreSQL with connection pool
    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(&cfg.database.url)
        .await
        .context("Failed to connect to PostgreSQL")?;

    tracing::info!("PostgreSQL pool established");

    // 4. Run pending migrations (sqlx compile-time checked)
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("Database migration failed")?;

    tracing::info!("Database migrations applied");

    // 5. Connect Kafka producer
    let kafka = Arc::new(
        KafkaProducer::new(&cfg.kafka.brokers)
            .context("Failed to connect Kafka producer")?
    );

    tracing::info!(brokers = %cfg.kafka.brokers, "Kafka producer ready");

    // 6. JWT service — shared across auth service and middleware
    let jwt = Arc::new(JwtService::new(
        cfg.auth.jwt_secret.clone(),
        cfg.auth.jwt_expiry_seconds,
        cfg.auth.refresh_token_expiry_seconds,
    ));

    // 7. Repositories — injected as trait objects (hexagonal architecture)
    let tenant_repo = Arc::new(PgTenantRepository::new(pool.clone()));
    let user_repo   = Arc::new(PgUserRepository::new(pool.clone()));
    let api_key_repo = Arc::new(PgApiKeyRepository::new(pool.clone()));

    // 8. Application services — depend only on repository traits, not DB types
    let auth_service = Arc::new(AuthService::new(
        Arc::clone(&tenant_repo) as _,
        Arc::clone(&user_repo) as _,
        Arc::clone(&jwt),
    ));

    let tenant_service = Arc::new(TenantService::new(
        Arc::clone(&tenant_repo) as _,
        Arc::clone(&user_repo) as _,
        Arc::clone(&kafka),
    ));

    let api_key_service = Arc::new(ApiKeyService::new(
        Arc::clone(&api_key_repo) as _,
    ));

    // 9. Build Axum router with shared application state
    let state = Arc::new(AppState {
        auth_service,
        tenant_service,
        api_key_service,
        jwt: Arc::clone(&jwt),
    });

    let app = router(state);

    // 10. Bind and serve
    let addr = format!("{}:{}", cfg.app.host, cfg.app.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("Failed to bind to {addr}"))?;

    tracing::info!(addr = %addr, "identity service listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("Server error")?;

    tracing::info!("identity service shutdown complete");
    Ok(())
}

/// Listen for SIGTERM/SIGINT for graceful pod termination in Kubernetes.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => { tracing::info!("Received SIGINT, shutting down"); },
        _ = terminate => { tracing::info!("Received SIGTERM, shutting down"); },
    }
}
