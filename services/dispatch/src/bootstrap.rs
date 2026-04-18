use std::sync::Arc;
use sqlx::postgres::PgPoolOptions;
use anyhow::Context;
use tokio::sync::{watch, Mutex};
use crate::config::Config;
use crate::application::services::DriverAssignmentService;
use crate::infrastructure::db::{
    PgRouteRepository, PgDriverAssignmentRepository, PgDriverAvailabilityRepository,
    ComplianceCache, DispatchQueueRepository, DriverProfilesRepository,
    PgDispatchQueueRepository, PgDriverProfilesRepository,
};
use crate::infrastructure::messaging::compliance_consumer::start_compliance_consumer;
use crate::infrastructure::messaging::{start_shipment_consumer, start_user_consumer};
use crate::api::http::{router, AppState};
use logisticos_auth::jwt::JwtService;
use logisticos_events::producer::KafkaProducer;

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load().context("Failed to load dispatch config")?;

    let otlp = std::env::var("OTLP_ENDPOINT").ok();
    logisticos_tracing::init(logisticos_tracing::TracingConfig {
        service_name: "dispatch",
        env: &cfg.app.env,
        otlp_endpoint: otlp.as_deref(),
        log_level: None,
    })?;

    tracing::info!(env = %cfg.app.env, "dispatch service starting");

    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .after_connect(|conn, _meta| Box::pin(async move {
            // driver_ops included so PostGIS types (geography, geometry) resolve.
            // PostGIS was created while connected with search_path = driver_ops,public
            // so its types live in driver_ops and can't be relocated (ALTER EXTENSION
            // postgis SET SCHEMA is unsupported). dispatch still owns its own tables —
            // dispatch comes first, so unqualified writes land in dispatch.
            sqlx::query("SET search_path TO dispatch, driver_ops, public")
                .execute(&mut *conn)
                .await?;
            Ok(())
        }))
        .connect(&cfg.database.url)
        .await
        .context("Failed to connect to PostgreSQL")?;

    // dispatch now shares the svc_driver_ops database so driver_avail_repo can read
    // driver_ops.drivers and driver_ops.driver_latest_locations in the same query.
    // The dispatch schema won't exist yet on that DB — create it before migrations run.
    sqlx::query("CREATE SCHEMA IF NOT EXISTS dispatch").execute(&pool).await
        .context("Failed to create dispatch schema")?;
    sqlx::query(r#"CREATE EXTENSION IF NOT EXISTS "uuid-ossp""#).execute(&pool).await
        .context("Failed to enable uuid-ossp extension")?;
    sqlx::query("CREATE EXTENSION IF NOT EXISTS postgis").execute(&pool).await
        .context("Failed to enable postgis extension")?;

    logisticos_common::migrations::run(&pool, "dispatch", &sqlx::migrate!("./migrations")).await
        .context("Dispatch database migration failed")?;

    let kafka = Arc::new(
        KafkaProducer::new(&cfg.kafka.brokers)
            .context("Failed to connect Kafka producer")?
    );

    let jwt_secret = std::env::var("AUTH__JWT_SECRET")
        .context("AUTH__JWT_SECRET env var not set")?;
    let jwt = Arc::new(JwtService::new(&jwt_secret, 3600, 86400));

    // Redis — compliance status cache
    let redis_manager = redis::aio::ConnectionManager::new(
        redis::Client::open(cfg.redis.url.as_str())
            .context("Failed to create Redis client")?,
    )
    .await
    .context("Failed to connect to Redis")?;

    let compliance_cache = Arc::new(Mutex::new(ComplianceCache::new(redis_manager)));

    // Spawn compliance Kafka consumer — updates cache on compliance.status_changed events
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let cache_for_consumer = Arc::clone(&compliance_cache);
    let brokers_clone = cfg.kafka.brokers.clone();
    let consumer_group = format!("{}-compliance", cfg.kafka.group_id);
    tokio::spawn(async move {
        if let Err(e) = start_compliance_consumer(
            &brokers_clone,
            &consumer_group,
            cache_for_consumer,
            shutdown_rx,
        ).await {
            tracing::error!("Compliance consumer error: {e}");
        }
    });

    // Repositories
    let route_repo       = Arc::new(PgRouteRepository::new(pool.clone()));
    let assignment_repo  = Arc::new(PgDriverAssignmentRepository::new(pool.clone()));
    let driver_avail     = Arc::new(PgDriverAvailabilityRepository::new(pool.clone()));
    let queue_repo       = Arc::new(PgDispatchQueueRepository::new(pool.clone()));
    let drivers_repo     = Arc::new(PgDriverProfilesRepository::new(pool.clone()));

    // Application service
    let dispatch_service = Arc::new(DriverAssignmentService::new(
        Arc::clone(&route_repo) as _,
        Arc::clone(&assignment_repo) as _,
        Arc::clone(&driver_avail) as _,
        Arc::clone(&kafka),
        Some(Arc::clone(&compliance_cache)),
        Arc::clone(&queue_repo) as Arc<dyn DispatchQueueRepository>,
    ));

    // Spawn shipment consumer — populates dispatch_queue from SHIPMENT_CREATED events
    // and auto-dispatches customer-app bookings (booked_by_customer == true).
    let pool_for_shipment     = pool.clone();
    let brokers_shipment      = cfg.kafka.brokers.clone();
    let group_shipment        = cfg.kafka.group_id.clone();
    let dispatch_svc_shipment = Arc::clone(&dispatch_service);
    let shutdown_rx_shipment  = shutdown_tx.subscribe();
    tokio::spawn(async move {
        if let Err(e) = start_shipment_consumer(
            &brokers_shipment,
            &group_shipment,
            pool_for_shipment,
            dispatch_svc_shipment,
            shutdown_rx_shipment,
        ).await {
            tracing::error!("Shipment consumer crashed: {e}");
        }
    });

    // Spawn user consumer — caches driver profiles from USER_CREATED events
    let pool_for_users    = pool.clone();
    let brokers_users     = cfg.kafka.brokers.clone();
    let group_users       = cfg.kafka.group_id.clone();
    let shutdown_rx_users = shutdown_tx.subscribe();
    tokio::spawn(async move {
        if let Err(e) = start_user_consumer(&brokers_users, &group_users, pool_for_users, shutdown_rx_users).await {
            tracing::error!("User consumer crashed: {e}");
        }
    });

    let state = Arc::new(AppState {
        dispatch_service,
        jwt:          Arc::clone(&jwt),
        queue_repo:   queue_repo as Arc<dyn DispatchQueueRepository>,
        drivers_repo: drivers_repo as Arc<dyn DriverProfilesRepository>,
    });
    use tower_http::cors::CorsLayer;
    use axum::http::{HeaderName, HeaderValue, Method};

    let cors = CorsLayer::new()
        .allow_origin([
            "http://localhost:3001".parse::<HeaderValue>().unwrap(),
            "http://localhost:3002".parse::<HeaderValue>().unwrap(),
            "http://localhost:3003".parse::<HeaderValue>().unwrap(),
            "http://localhost:8083".parse::<HeaderValue>().unwrap(),
        ])
        .allow_methods([
            Method::GET, Method::POST, Method::PUT,
            Method::PATCH, Method::DELETE, Method::OPTIONS,
        ])
        .allow_headers([
            HeaderName::from_static("content-type"),
            HeaderName::from_static("authorization"),
        ]);

    let app = router(state).layer(cors);

    let addr = format!("{}:{}", cfg.app.host, cfg.app.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("Failed to bind to {addr}"))?;

    tracing::info!(addr = %addr, "dispatch service listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("Dispatch server error")?;

    // Signal Kafka consumer to stop
    if shutdown_tx.send(true).is_err() {
        tracing::warn!("Compliance consumer already stopped before shutdown signal");
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
