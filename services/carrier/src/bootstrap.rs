use std::{net::SocketAddr, sync::Arc};
use anyhow::Context;
use sqlx::postgres::PgPoolOptions;
use tokio::sync::watch;
use tonic::transport::Server as GrpcServer;
use logisticos_auth::jwt::JwtService;
use logisticos_events::producer::KafkaProducer;

use crate::{
    api::{http, middleware::propagate_request_id},
    application::services::{CarrierService, MarketplaceService},
    config::Config,
    infrastructure::{
        cache::CachedCarrierRepository,
        db::{PgCarrierRepository, PgMarketplaceRepository, PgSlaRecordRepository},
        messaging::{start_delivery_consumer, CarrierPublisher},
    },
    AppState,
};

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load()?;
    let otlp = std::env::var("OTLP_ENDPOINT").ok();
    logisticos_tracing::init(logisticos_tracing::TracingConfig {
        service_name: "carrier",
        env: &cfg.app.env,
        otlp_endpoint: otlp.as_deref(),
        log_level: None,
    })?;

    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .after_connect(|conn, _meta| Box::pin(async move {
            sqlx::query("SET search_path TO carrier, public")
                .execute(&mut *conn)
                .await?;
            Ok(())
        }))
        .connect(&cfg.database.url)
        .await?;

    logisticos_common::migrations::run(&pool, "carrier", &sqlx::migrate!("./migrations")).await?;

    // Repositories
    let pg_carrier_repo  = Arc::new(PgCarrierRepository::new(pool.clone()));
    let sla_repo         = Arc::new(PgSlaRecordRepository::new(pool.clone()));
    let marketplace_repo = Arc::new(PgMarketplaceRepository::new(pool.clone()));

    // Wrap the carrier repo in a Redis write-through cache to cut latency on
    // the hot /me and find_by_id paths (partner portal polls /me on every page).
    let carrier_repo: Arc<dyn crate::domain::repositories::CarrierRepository> =
        match CachedCarrierRepository::new(pg_carrier_repo.clone(), &cfg.redis.url).await {
            Ok(cached) => Arc::new(cached),
            Err(e) => {
                tracing::warn!("Redis unavailable — falling back to uncached carrier repo: {e}");
                pg_carrier_repo as Arc<dyn crate::domain::repositories::CarrierRepository>
            }
        };

    // Kafka producer + publisher
    let kafka_producer = Arc::new(KafkaProducer::new(&cfg.kafka.brokers)?);
    let publisher      = Arc::new(CarrierPublisher::new(Arc::clone(&kafka_producer)));

    // Outbound webhook client (notifies 3PL carriers of allocations)
    let outbound_secret = std::env::var("CARRIER_WEBHOOK_SECRET").ok();
    let external_client = Arc::new(
        crate::infrastructure::external::ExternalCarrierClient::new(outbound_secret),
    );

    // Application services
    let carrier_svc = Arc::new(CarrierService::new(
        Arc::clone(&carrier_repo),
        Arc::clone(&sla_repo)     as Arc<dyn crate::domain::repositories::SlaRecordRepository>,
        Arc::clone(&publisher),
        external_client,
    ));
    let marketplace_svc = Arc::new(MarketplaceService::new(
        Arc::clone(&marketplace_repo) as Arc<dyn crate::domain::repositories::MarketplaceRepository>,
        Arc::clone(&kafka_producer),
    ));

    // Graceful shutdown channel
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // Spawn delivery outcome consumer
    {
        let brokers    = cfg.kafka.brokers.clone();
        let group_id   = cfg.kafka.group_id.clone();
        let c_repo     = Arc::clone(&carrier_repo);
        let s_repo     = Arc::clone(&sla_repo) as Arc<dyn crate::domain::repositories::SlaRecordRepository>;
        let rx         = shutdown_rx.clone();
        tokio::spawn(async move {
            if let Err(e) = start_delivery_consumer(&brokers, &group_id, c_repo, s_repo, rx).await {
                tracing::error!("Delivery consumer exited with error: {e}");
            }
        });
    }

    let jwt_secret = std::env::var("AUTH__JWT_SECRET")
        .context("AUTH__JWT_SECRET env var not set")?;
    let jwt = Arc::new(JwtService::new(&jwt_secret, 3600, 86400));

    // Clone carrier_svc before moving state into the router — gRPC server
    // needs its own Arc handle and `with_state` consumes `state`.
    let carrier_svc_for_grpc = Arc::clone(&carrier_svc);

    let state = AppState { carrier_svc, marketplace_svc, jwt: Arc::clone(&jwt) };

    // CORS — allow the partner, admin, and merchant portals to call the
    // carrier service directly. Production origins are set via APP__CORS_ORIGINS
    // (comma-separated); the defaults cover common local dev ports.
    use axum::http::{HeaderName, HeaderValue, Method};
    use tower_http::cors::CorsLayer;

    let default_origins = [
        "http://localhost:3001",
        "http://localhost:3002",
        "http://localhost:3003",
        "http://localhost:8083",
    ];
    let allowed_origins: Vec<HeaderValue> = cfg.app.cors_origins
        .as_deref()
        .map(|s| s.split(',').map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>())
        .unwrap_or_else(|| default_origins.to_vec())
        .into_iter()
        .filter_map(|o| o.parse::<HeaderValue>().ok())
        .collect();

    let cors = CorsLayer::new()
        .allow_origin(allowed_origins)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            HeaderName::from_static("content-type"),
            HeaderName::from_static("authorization"),
            HeaderName::from_static("x-logisticos-client"),
        ]);

    // Compose the app:
    //   • JWT-protected routes (all /v1/* except webhooks)
    //   • Unauthenticated webhook routes (/v1/webhooks/*)
    // Both share AppState; `propagate_request_id` and TraceLayer wrap the whole app.
    let app = http::router()
        .layer(axum::middleware::from_fn_with_state(jwt, logisticos_auth::middleware::require_auth))
        .merge(http::webhook_router())
        .layer(axum::middleware::from_fn(propagate_request_id))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.app.host, cfg.app.port).parse()?;
    tracing::info!(addr = %addr, "carrier HTTP service listening");

    // Optionally start the gRPC server on a secondary port.
    if let Some(grpc_port) = cfg.app.grpc_port {
        let grpc_addr: SocketAddr = format!("{}:{}", cfg.app.host, grpc_port).parse()?;
        tracing::info!(addr = %grpc_addr, "carrier gRPC service listening");
        let grpc_svc = crate::api::grpc::CarrierGrpc::new(carrier_svc_for_grpc)
            .into_service();
        let mut grpc_rx = shutdown_rx.clone();
        tokio::spawn(async move {
            if let Err(e) = GrpcServer::builder()
                .add_service(grpc_svc)
                .serve_with_shutdown(grpc_addr, async move {
                    grpc_rx.changed().await.ok();
                })
                .await
            {
                tracing::error!("gRPC server exited with error: {e}");
            }
        });
    }

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(shutdown_tx))
        .await?;
    Ok(())
}

async fn shutdown_signal(shutdown_tx: watch::Sender<bool>) {
    use tokio::signal;
    let ctrl_c = async { signal::ctrl_c().await.expect("ctrl-c") };
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate()).expect("SIGTERM").recv().await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
    // Signal background consumers to stop
    let _ = shutdown_tx.send(true);
}
