use std::{net::SocketAddr, sync::Arc};
use rdkafka::{consumer::StreamConsumer, ClientConfig};
use sqlx::postgres::PgPoolOptions;

use anyhow::Context;
use logisticos_auth::jwt::JwtService;
use crate::{
    api::http,
    application::{handlers, queries::QueryService},
    config::Config,
    infrastructure::db::AnalyticsDb,
    AppState,
};

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load()?;
    let otlp = std::env::var("OTLP_ENDPOINT").ok();
    logisticos_tracing::init(logisticos_tracing::TracingConfig {
        service_name: "analytics",
        env: &cfg.app.env,
        otlp_endpoint: otlp.as_deref(),
        log_level: None,
    })?;

    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .after_connect(|conn, _meta| Box::pin(async move {
            sqlx::query("SET search_path TO analytics, public")
                .execute(&mut *conn)
                .await?;
            Ok(())
        }))
        .connect(&cfg.database.url)
        .await?;

    logisticos_common::migrations::run(&pool, "analytics", &sqlx::migrate!("./migrations")).await?;

    let db         = Arc::new(AnalyticsDb::new(pool));
    let query_svc  = Arc::new(QueryService::new(db.clone()));

    let consumer: Arc<StreamConsumer> = Arc::new(
        ClientConfig::new()
            .set("bootstrap.servers", &cfg.kafka.brokers)
            .set("group.id", &cfg.kafka.group_id)
            .set("auto.offset.reset", "earliest")
            .set("enable.auto.commit", "false")
            .create()?,
    );

    let consumer_db = db.clone();
    tokio::spawn(async move {
        handlers::run_consumer(consumer, consumer_db).await;
    });

    let jwt_secret = std::env::var("AUTH__JWT_SECRET")
        .context("AUTH__JWT_SECRET env var not set")?;
    let jwt = Arc::new(JwtService::new(&jwt_secret, 3600, 86400));

    let state = AppState { query_svc, jwt: Arc::clone(&jwt) };

    use tower_http::cors::{AllowOrigin, CorsLayer};
    use axum::http::{HeaderName, HeaderValue, Method};

    const DEFAULT_ORIGINS: &[&str] = &[
        "http://localhost:3001",
        "http://localhost:3002",
        "http://localhost:3003",
        "http://localhost:8083",
    ];
    let allowed_origins: Vec<HeaderValue> = cfg.app.cors_origins
        .as_deref()
        .map(|s| s.split(',').map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>())
        .unwrap_or_else(|| DEFAULT_ORIGINS.to_vec())
        .into_iter()
        .filter_map(|o| o.parse::<HeaderValue>().ok())
        .collect();
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(allowed_origins))
        .allow_methods([
            Method::GET, Method::POST, Method::PUT,
            Method::PATCH, Method::DELETE, Method::OPTIONS,
        ])
        .allow_headers([
            HeaderName::from_static("content-type"),
            HeaderName::from_static("authorization"),
            HeaderName::from_static("x-logisticos-client"),
        ]);

    let app = http::router()
        .layer(axum::middleware::from_fn_with_state(jwt, logisticos_auth::middleware::require_auth))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.app.host, cfg.app.port).parse()?;
    tracing::info!(addr = %addr, "analytics service listening");

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
    tracing::info!("analytics shutdown");
}
