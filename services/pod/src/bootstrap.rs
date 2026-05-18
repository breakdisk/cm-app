use std::sync::Arc;
use sqlx::postgres::PgPoolOptions;
use anyhow::Context;
use crate::config::Config;
use crate::application::services::PodService;
use crate::infrastructure::db::{PgPodRepository, PgOtpRepository, PgPickupRepository, PgTelemetryRepository};
use crate::infrastructure::external::storage::S3StorageAdapter;
use crate::infrastructure::external::sms::{NoOpSmsAdapter, TwilioSmsAdapter};
use crate::api::http::{router, AppState};
use logisticos_auth::jwt::JwtService;
use logisticos_events::producer::KafkaProducer;

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load().context("Failed to load pod config")?;

    let otlp = std::env::var("OTLP_ENDPOINT").ok();
    logisticos_tracing::init(logisticos_tracing::TracingConfig {
        service_name: "pod",
        env: &cfg.app.env,
        otlp_endpoint: otlp.as_deref(),
        log_level: None,
    })?;

    tracing::info!(env = %cfg.app.env, "pod service starting");

    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .after_connect(|conn, _meta| Box::pin(async move {
            sqlx::query("SET search_path TO pod, public")
                .execute(&mut *conn)
                .await?;
            Ok(())
        }))
        .connect(&cfg.database.url)
        .await
        .context("Failed to connect to PostgreSQL")?;

    logisticos_common::migrations::run(&pool, "pod", &sqlx::migrate!("./migrations")).await
        .context("POD migration failed")?;

    let kafka = Arc::new(
        KafkaProducer::new(&cfg.kafka.brokers).context("Kafka connection failed")?
    );

    let jwt_secret = std::env::var("AUTH__JWT_SECRET").context("AUTH__JWT_SECRET not set")?;
    let jwt = Arc::new(JwtService::new(&jwt_secret, 3600, 86400));

    // S3/MinIO/R2 storage adapter
    //
    // S3_REGION must be set or the AWS SDK falls back to IMDS lookup which
    // hangs for ~1 s and then errors every call with "A region must be set".
    // For Cloudflare R2 use "auto"; for AWS S3 use the bucket's region (e.g.
    // "us-east-1"); for MinIO any non-empty value works.
    //
    // S3__ACCESS_KEY_ID / S3__SECRET_ACCESS_KEY must be set to the Cloudflare R2
    // access key (32 chars). If omitted, the AWS SDK auto-discovers credentials
    // from AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY — but those are typically
    // 20-char AWS keys that R2 rejects with "Credential access key has length 20,
    // should be 32".
    let s3_bucket           = std::env::var("S3_BUCKET").unwrap_or_else(|_| "logisticos-pod".to_string());
    let s3_endpoint         = std::env::var("S3_ENDPOINT_URL").ok();
    let s3_region           = std::env::var("S3_REGION").or_else(|_| std::env::var("AWS_REGION")).ok();
    let s3_force_path_style = std::env::var("S3_FORCE_PATH_STYLE")
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false); // false = virtual-hosted style (R2); true = path-style (MinIO)
    // Detect Cloudflare R2 by endpoint host. R2 credentials are *exactly* 32
    // chars; AWS S3 keys are 20 chars; MinIO accepts anything. We've burned
    // multiple deploys on the AWS SDK silently picking up a leftover 20-char
    // `AWS_ACCESS_KEY_ID` from the container env and embedding it in presigned
    // URLs that R2 then rejects with `Credential access key has length 20,
    // should be 32`. The fix is to refuse to boot in that configuration —
    // surface the misconfig at startup, not at the first driver POD upload.
    let is_r2 = s3_endpoint
        .as_deref()
        .map(|e| e.contains("r2.cloudflarestorage.com"))
        .unwrap_or(false);

    let s3_credentials = match (cfg.s3.access_key_id.clone(), cfg.s3.secret_access_key.clone()) {
        (Some(k), Some(s)) => {
            if is_r2 && k.len() != 32 {
                anyhow::bail!(
                    "S3__ACCESS_KEY_ID is {} chars but Cloudflare R2 requires exactly 32. \
                     This usually means an AWS S3 key (20 chars) is set instead of an R2 API token. \
                     Generate an R2 token at https://dash.cloudflare.com/?to=/:account/r2/api-tokens \
                     and update S3__ACCESS_KEY_ID / S3__SECRET_ACCESS_KEY.",
                    k.len()
                );
            }
            tracing::info!(
                key_len = k.len(),
                target = if is_r2 { "cloudflare-r2" } else { "s3-compatible" },
                "S3 storage: using explicit credentials from config"
            );
            Some((k, s))
        }
        _ => {
            if is_r2 {
                anyhow::bail!(
                    "S3 storage targets Cloudflare R2 ({}) but S3__ACCESS_KEY_ID / \
                     S3__SECRET_ACCESS_KEY are not set. Refusing to start: the AWS SDK \
                     credential chain would silently pick up any AWS_ACCESS_KEY_ID in the \
                     environment (typically a 20-char AWS key) and R2 would reject every \
                     presigned PUT with `Credential access key has length 20, should be 32`. \
                     Set both env vars to your R2 API token (32-char access key id).",
                    s3_endpoint.as_deref().unwrap_or("<unknown>")
                );
            }
            tracing::warn!(
                "S3 storage: explicit credentials not configured — falling back to AWS SDK \
                 credential auto-discovery. Safe for real AWS S3 / MinIO; never use this path \
                 against Cloudflare R2."
            );
            None
        }
    };
    let storage = Arc::new(
        S3StorageAdapter::new(s3_endpoint, s3_bucket, s3_region, s3_force_path_style, s3_credentials).await
            .context("Failed to init S3 storage")?
    );

    // Twilio SMS for OTP delivery.
    // All three vars must be present to enable real SMS; if any are missing the
    // service boots with a no-op adapter so photo/signature PODs still work.
    // OTP-required deliveries will log a warning instead of sending an SMS —
    // set the vars in production to enable real OTP delivery.
    let sms: Arc<dyn crate::infrastructure::external::sms::SmsAdapter> = match (
        std::env::var("TWILIO_ACCOUNT_SID").ok(),
        std::env::var("TWILIO_AUTH_TOKEN").ok(),
        std::env::var("TWILIO_FROM_NUMBER").ok(),
    ) {
        (Some(sid), Some(token), Some(from)) => {
            tracing::info!("SMS: Twilio configured (from {})", from);
            Arc::new(TwilioSmsAdapter::new(sid, token, from))
        }
        _ => {
            tracing::warn!(
                "SMS: TWILIO_ACCOUNT_SID / TWILIO_AUTH_TOKEN / TWILIO_FROM_NUMBER not all set — \
                 using NoOpSmsAdapter. OTP SMS will NOT be delivered. \
                 Set all three env vars in production."
            );
            Arc::new(NoOpSmsAdapter)
        }
    };

    // Repositories
    let pod_repo      = Arc::new(PgPodRepository::new(pool.clone()));
    let otp_repo      = Arc::new(PgOtpRepository::new(pool.clone()));
    let pickup_repo   = Arc::new(PgPickupRepository::new(pool.clone()));
    let telemetry     = Arc::new(PgTelemetryRepository::new(pool.clone()));

    let pod_service = Arc::new(PodService::new(
        Arc::clone(&pod_repo)    as _,
        Arc::clone(&otp_repo)    as _,
        Arc::clone(&pickup_repo) as _,
        Arc::clone(&telemetry)   as _,
        Arc::clone(&storage)     as _,
        Arc::clone(&sms)         as _,
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
