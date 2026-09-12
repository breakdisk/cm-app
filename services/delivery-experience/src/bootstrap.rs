use std::{net::SocketAddr, sync::Arc};

use anyhow::Context as _;
use rdkafka::{consumer::StreamConsumer, ClientConfig};
use sqlx::postgres::PgPoolOptions;

use crate::{
    api::http,
    application::{handlers, services::{EventPublisher, TrackingService}},
    config::Config,
    infrastructure::{db::PgTrackingRepository, external::PodClient, messaging::KafkaEventPublisher},
    AppState,
};

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load()?;
    logisticos_tracing::init(logisticos_tracing::TracingConfig {
        service_name: "delivery-experience",
        env: &cfg.app.env,
        otlp_endpoint: None,
        log_level: None,
    })?;

    // Connect without search_path first so migrations can create the schema
    let pre_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&cfg.database.url)
        .await?;
    // Ensure schema exists before setting search_path on pool connections
    sqlx::query("CREATE SCHEMA IF NOT EXISTS delivery_experience")
        .execute(&pre_pool)
        .await?;
    pre_pool.close().await;

    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .after_connect(|conn, _meta| Box::pin(async move {
            sqlx::query("SET search_path TO delivery_experience, public")
                .execute(&mut *conn)
                .await?;
            Ok(())
        }))
        .connect(&cfg.database.url)
        .await?;

    logisticos_common::migrations::run(&pool, "delivery_experience", &sqlx::migrate!("./migrations")).await?;

    let tracking_repo = Arc::new(PgTrackingRepository::new(pool.clone()));
    let publisher: Arc<dyn EventPublisher> = Arc::new(KafkaEventPublisher::new(&cfg.kafka.brokers)?);
    let tracking_svc  = Arc::new(
        TrackingService::new(tracking_repo.clone()).with_publisher(publisher),
    );

    // Kafka consumer for shipment lifecycle projections.
    let consumer: Arc<StreamConsumer> = Arc::new(
        ClientConfig::new()
            .set("bootstrap.servers", &cfg.kafka.brokers)
            .set("group.id", &cfg.kafka.group_id)
            .set("auto.offset.reset", "earliest")
            .set("enable.auto.commit", "false")
            .create()?,
    );

    let consumer_repo = tracking_repo.clone() as Arc<dyn crate::domain::repositories::TrackingRepository>;
    tokio::spawn(async move {
        handlers::run_consumer(consumer, consumer_repo).await;
    });

    let pod_client = Arc::new(
        PodClient::new(cfg.pod_internal_url.clone())
            .context("Failed to build PodClient HTTP client")?
    );
    tracing::info!(url = %cfg.pod_internal_url, "PodClient: targeting pod service for evidence enrichment");

    let state = AppState { tracking_svc, pod_client };

    use tower_http::cors::CorsLayer;
    use axum::http::{HeaderName, HeaderValue, Method};

    let default_origins = [
        "http://localhost:3000",
        "http://localhost:3001",
        "http://localhost:3002",
        "http://localhost:3003",
        "http://localhost:8083",
    ];
    let allowed_origins: Vec<HeaderValue> = cfg.app.cors_origins
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.split(',').map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>())
        .unwrap_or_else(|| default_origins.to_vec())
        .into_iter()
        .filter_map(|o| o.parse::<HeaderValue>().ok())
        .collect();

    let cors = CorsLayer::new()
        .allow_origin(allowed_origins)
        .allow_methods([
            Method::GET, Method::POST, Method::PUT,
            Method::PATCH, Method::DELETE, Method::OPTIONS,
        ])
        .allow_headers([
            HeaderName::from_static("content-type"),
            HeaderName::from_static("authorization"),
            HeaderName::from_static("x-logisticos-client"),
        ]);

    // `get_by_shipment_id` and `list_shipments` extract `AuthClaims`, which
    // reads `Claims` from request extensions — and only `require_auth` puts
    // them there. Nothing mounted it here, so both returned
    // 500 AUTH_NOT_CONFIGURED on every call.
    //
    // Layered over the authenticated half only: the public half is how a
    // customer with a tracking number and no account reaches their delivery,
    // and requiring a token there would break the tracking page outright.
    let jwt_secret = std::env::var("AUTH__JWT_SECRET").unwrap_or_default();
    let jwt: logisticos_auth::middleware::AuthState =
        Arc::new(logisticos_auth::jwt::JwtService::new(&jwt_secret, 3600, 86_400));

    // Outside the auth layer for the same reason the public tracking routes are:
    // a probe cannot present a JWT. This service had no /health either, so the
    // container has been reporting unhealthy on a 404.
    let observability = axum::Router::new()
        .route("/health",  axum::routing::get(|| async {
            axum::Json(serde_json::json!({"status": "ok", "service": "delivery-experience"}))
        }))
        .route("/ready",   axum::routing::get(|| async {
            axum::Json(serde_json::json!({"status": "ready"}))
        }))
        .route("/metrics", axum::routing::get(|| async { "" }));

    let app = http::public_router()
        .merge(http::authenticated_router().layer(
            axum::middleware::from_fn_with_state(
                jwt,
                logisticos_auth::middleware::require_auth,
            ),
        ))
        .merge(observability)
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.app.host, cfg.app.port).parse()?;
    tracing::info!(addr = %addr, "delivery-experience service listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    use tokio::signal;
    let ctrl_c = async { signal::ctrl_c().await.expect("ctrl-c handler") };
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("delivery-experience shutdown signal received");
}
