use std::sync::Arc;
use sqlx::postgres::PgPoolOptions;
use anyhow::Context;
use crate::config::Config;
use crate::application::services::{ComplianceService, ExpiryCheckerService};
use crate::infrastructure::storage::DocumentStorage;
use crate::infrastructure::db::{
    PgComplianceProfileRepository,
    PgDriverDocumentRepository,
    PgDocumentTypeRepository,
    PgAuditLogRepository,
};
use crate::infrastructure::messaging::{ComplianceProducer, start_driver_consumer};
use crate::api::http::{router, AppState};
use logisticos_auth::jwt::JwtService;
use logisticos_events::producer::KafkaProducer;

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load().context("Failed to load compliance config")?;

    let otlp_endpoint = std::env::var("OTLP_ENDPOINT").ok();
    logisticos_tracing::init(logisticos_tracing::TracingConfig {
        service_name: "compliance",
        env: &cfg.app.env,
        otlp_endpoint: otlp_endpoint.as_deref(),
        log_level: None,
    })?;

    tracing::info!(env = %cfg.app.env, "compliance service starting");

    // ── Database ─────────────────────────────────────────────────────────────
    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .after_connect(|conn, _meta| Box::pin(async move {
            sqlx::query("SET search_path TO compliance, public")
                .execute(&mut *conn)
                .await?;
            Ok(())
        }))
        .connect(&cfg.database.url)
        .await
        .context("Failed to connect to PostgreSQL")?;

    logisticos_common::migrations::run(&pool, "compliance", &sqlx::migrate!("./migrations"))
        .await
        .context("compliance migration failed")?;

    // ── Document storage ──────────────────────────────────────────────────────
    let storage = Arc::new(
        DocumentStorage::new(&cfg.storage)
            .await
            .context("Failed to init document storage")?,
    );

    // ── Kafka producer ────────────────────────────────────────────────────────
    let kafka = Arc::new(
        KafkaProducer::new(&cfg.kafka.brokers)
            .context("Failed to create Kafka producer")?,
    );

    // ── JWT service ───────────────────────────────────────────────────────────
    let jwt_secret = std::env::var("AUTH__JWT_SECRET")
        .context("AUTH__JWT_SECRET not set")?;
    let jwt = Arc::new(JwtService::new(
        &jwt_secret,
        cfg.auth.access_token_ttl,
        cfg.auth.refresh_token_ttl,
    ));

    // ── Repositories ──────────────────────────────────────────────────────────
    let profile_repo  = Arc::new(PgComplianceProfileRepository::new(pool.clone()));
    let document_repo = Arc::new(PgDriverDocumentRepository::new(pool.clone()));
    let doc_type_repo = Arc::new(PgDocumentTypeRepository::new(pool.clone()));
    let audit_repo    = Arc::new(PgAuditLogRepository::new(pool.clone()));

    // ── Compliance producer ───────────────────────────────────────────────────
    let producer = Arc::new(ComplianceProducer::new(Arc::clone(&kafka)));

    // ── Application services ──────────────────────────────────────────────────
    let compliance_service = Arc::new(ComplianceService::new(
        Arc::clone(&profile_repo)  as _,
        Arc::clone(&document_repo) as _,
        Arc::clone(&doc_type_repo) as _,
        Arc::clone(&audit_repo)    as _,
        Arc::clone(&producer),
    ));

    let expiry_checker = Arc::new(ExpiryCheckerService::new(
        Arc::clone(&compliance_service),
        Arc::clone(&document_repo) as _,
        Arc::clone(&profile_repo)  as _,
        Arc::clone(&producer),
    ));

    // ── Kafka consumer (shutdown watch channel) ───────────────────────────────
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    let compliance_for_consumer = Arc::clone(&compliance_service);
    let brokers    = cfg.kafka.brokers.clone();
    let group_id   = cfg.kafka.consumer_group.clone();

    tokio::spawn(async move {
        if let Err(e) = start_driver_consumer(&brokers, &group_id, compliance_for_consumer, shutdown_rx).await {
            tracing::error!("Kafka consumer error: {e}");
        }
    });

    // ── Daily expiry checker loop ─────────────────────────────────────────────
    let checker = Arc::clone(&expiry_checker);
    tokio::spawn(async move {
        // Run immediately on startup, then every 24 hours
        if let Err(e) = checker.run_once().await {
            tracing::error!("Expiry checker error on startup run: {e}");
        }
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(86_400));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if let Err(e) = checker.run_once().await {
                tracing::error!("Expiry checker error: {e}");
            }
        }
    });

    // ── HTTP server ───────────────────────────────────────────────────────────
    let state = Arc::new(AppState {
        compliance: compliance_service,
        jwt,
        storage,
        pool,
    });

    let app = router(state);

    let addr = format!("{}:{}", cfg.app.host, cfg.app.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("Failed to bind to {addr}"))?;

    tracing::info!(addr = %addr, "compliance service listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(shutdown_tx))
        .await
        .context("compliance server error")?;

    Ok(())
}

async fn shutdown_signal(shutdown_tx: tokio::sync::watch::Sender<bool>) {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .unwrap_or_else(|e| tracing::error!("Failed to install Ctrl+C handler: {e}"));
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => { s.recv().await; }
            Err(e) => {
                tracing::error!("Failed to install SIGTERM handler: {e}");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c    => {}
        _ = terminate => {}
    }

    // Signal the Kafka consumer to stop
    let _ = shutdown_tx.send(true);
    tracing::info!("Shutdown signal received; stopping compliance service");
}
