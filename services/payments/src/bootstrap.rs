use std::sync::Arc;
use sqlx::postgres::PgPoolOptions;
use anyhow::Context;
use crate::config::Config;
use crate::application::services::{InvoiceService, CodService, WalletService};
use crate::infrastructure::db::{PgInvoiceRepository, PgCodRepository, PgWalletRepository};
use crate::api::http::{router, AppState};
use logisticos_auth::jwt::JwtService;
use logisticos_events::producer::KafkaProducer;

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load().context("Failed to load payments config")?;

    let otlp = std::env::var("OTLP_ENDPOINT").ok();
    logisticos_tracing::init(logisticos_tracing::TracingConfig {
        service_name: "payments",
        env: &cfg.app.env,
        otlp_endpoint: otlp.as_deref(),
        log_level: None,
    })?;

    tracing::info!(env = %cfg.app.env, "payments service starting");

    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(&cfg.database.url)
        .await
        .context("Failed to connect to PostgreSQL")?;

    sqlx::migrate!("./migrations").run(&pool).await
        .context("Payments migration failed")?;

    let kafka = Arc::new(
        KafkaProducer::new(&cfg.kafka.brokers).context("Kafka connection failed")?
    );

    let jwt_secret = std::env::var("AUTH__JWT_SECRET").context("AUTH__JWT_SECRET not set")?;
    let jwt = Arc::new(JwtService::new(&jwt_secret, 3600, 86400));

    let invoice_repo = Arc::new(PgInvoiceRepository::new(pool.clone()));
    let cod_repo     = Arc::new(PgCodRepository::new(pool.clone()));
    let wallet_repo  = Arc::new(PgWalletRepository::new(pool.clone()));

    let invoice_service = Arc::new(InvoiceService::new(
        Arc::clone(&invoice_repo) as _, Arc::clone(&kafka),
    ));
    let cod_service = Arc::new(CodService::new(
        Arc::clone(&cod_repo) as _, Arc::clone(&wallet_repo) as _, Arc::clone(&kafka),
    ));
    let wallet_service = Arc::new(WalletService::new(
        Arc::clone(&wallet_repo) as _,
    ));

    let state = Arc::new(AppState {
        invoice_service, cod_service, wallet_service, jwt: Arc::clone(&jwt),
    });
    let app = router(state);

    let addr = format!("{}:{}", cfg.app.host, cfg.app.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("Failed to bind to {addr}"))?;

    tracing::info!(addr = %addr, "payments service listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("Payments server error")?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async { tokio::signal::ctrl_c().await.expect("ctrl_c") };
    #[cfg(unix)]
    let term = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("sigterm").recv().await;
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
}
