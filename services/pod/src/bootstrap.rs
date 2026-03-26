use std::sync::Arc;
use sqlx::postgres::PgPoolOptions;
use anyhow::Context;
use crate::config::Config;
use crate::application::services::PodService;
use crate::infrastructure::db::{PgPodRepository, PgOtpRepository};
use crate::infrastructure::external::storage::S3StorageAdapter;
use crate::infrastructure::external::sms::TwilioSmsAdapter;
use crate::api::http::{router, AppState};
use logisticos_auth::jwt::JwtService;
use logisticos_events::producer::KafkaProducer;

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load().context("Failed to load pod config")?;

    logisticos_tracing::init(logisticos_tracing::TracingConfig {
        service_name: "pod".to_string(),
        env: cfg.app.env.clone(),
        otlp_endpoint: std::env::var("OTLP_ENDPOINT").ok(),
    })?;

    tracing::info!(env = %cfg.app.env, "pod service starting");

    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(&cfg.database.url)
        .await
        .context("Failed to connect to PostgreSQL")?;

    sqlx::migrate!("./migrations").run(&pool).await
        .context("POD migration failed")?;

    let kafka = Arc::new(
        KafkaProducer::new(&cfg.kafka.brokers).context("Kafka connection failed")?
    );

    let jwt_secret = std::env::var("AUTH__JWT_SECRET").context("AUTH__JWT_SECRET not set")?;
    let jwt = Arc::new(JwtService::new(jwt_secret, 3600, 86400));

    // S3/MinIO storage adapter
    let s3_bucket  = std::env::var("S3_BUCKET").unwrap_or_else(|_| "logisticos-pod".to_string());
    let s3_endpoint = std::env::var("S3_ENDPOINT_URL").ok(); // None = use AWS default
    let storage = Arc::new(
        S3StorageAdapter::new(s3_endpoint, s3_bucket).await
            .context("Failed to init S3 storage")?
    );

    // Twilio SMS for OTP delivery
    let twilio_sid   = std::env::var("TWILIO_ACCOUNT_SID").context("TWILIO_ACCOUNT_SID not set")?;
    let twilio_token = std::env::var("TWILIO_AUTH_TOKEN").context("TWILIO_AUTH_TOKEN not set")?;
    let twilio_from  = std::env::var("TWILIO_FROM_NUMBER").context("TWILIO_FROM_NUMBER not set")?;
    let sms = Arc::new(TwilioSmsAdapter::new(twilio_sid, twilio_token, twilio_from));

    // Repositories
    let pod_repo = Arc::new(PgPodRepository::new(pool.clone()));
    let otp_repo = Arc::new(PgOtpRepository::new(pool.clone()));

    let pod_service = Arc::new(PodService::new(
        Arc::clone(&pod_repo) as _,
        Arc::clone(&otp_repo) as _,
        Arc::clone(&storage) as _,
        Arc::clone(&sms) as _,
        Arc::clone(&kafka),
    ));

    let state = Arc::new(AppState { pod_service, jwt: Arc::clone(&jwt) });
    let app = router(state);

    let addr = format!("{}:{}", cfg.app.host, cfg.app.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("Failed to bind to {addr}"))?;

    tracing::info!(addr = %addr, "pod service listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("POD server error")?;

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
