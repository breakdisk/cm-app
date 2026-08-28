use std::{net::SocketAddr, sync::Arc};

use anyhow::Context;
use logisticos_auth::jwt::JwtService;
use sqlx::postgres::PgPoolOptions;

use crate::{
    api::http::{router, AppState},
    application::{
        queries::ShipmentQueryService,
        services::shipment_service::{AddressNormalizer, PaymentCapability, ShipmentService},
    },
    config::Config,
    infrastructure::{
        awb::{FallbackAwbGenerator, PostgresAwbGenerator, RedisAwbGenerator},
        db::PgShipmentRepository,
        external::{MapboxGeocoder, PassthroughNormalizer},
        http::PaymentsClient,
        messaging::{
            payment_consumer::PaymentConsumer, status_consumer::start_status_consumer,
            KafkaEventPublisher,
        },
    },
};

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load()?;
    logisticos_tracing::init(logisticos_tracing::TracingConfig {
        service_name: "order-intake",
        env: &cfg.app.env,
        otlp_endpoint: None,
        log_level: None,
    })?;

    // Database
    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                sqlx::query("SET search_path TO order_intake, public")
                    .execute(&mut *conn)
                    .await?;
                Ok(())
            })
        })
        .connect(&cfg.database.url)
        .await?;

    logisticos_common::migrations::run(&pool, "order_intake", &sqlx::migrate!("./migrations")).await?;

    // Infrastructure adapters
    let repo = Arc::new(PgShipmentRepository { pool: pool.clone() });
    let publisher = Arc::new(KafkaEventPublisher::new(&cfg.kafka.brokers)?);
    let normalizer: Arc<dyn AddressNormalizer> = match cfg.geocoder.mapbox_access_token.as_deref() {
        Some(token) if !token.is_empty() => {
            tracing::info!("address normalizer: Mapbox geocoder");
            Arc::new(MapboxGeocoder::new(token.to_string()))
        }
        _ => {
            tracing::warn!(
                "GEOCODER__MAPBOX_ACCESS_TOKEN not set — shipments will be created with \
                 coordinates: None and dispatch will reject them until geocoded out-of-band"
            );
            Arc::new(PassthroughNormalizer)
        }
    };

    // AWB generator: Redis primary with PostgreSQL fallback
    let redis_conn = redis::Client::open(cfg.redis.url.as_str())
        .context("Invalid Redis URL")?
        .get_connection_manager()
        .await
        .context("Failed to connect to Redis")?;
    let awb_generator: Arc<dyn crate::domain::value_objects::AwbGenerator> =
        Arc::new(FallbackAwbGenerator::new(
            Arc::new(RedisAwbGenerator::new(redis_conn)),
            Arc::new(PostgresAwbGenerator::new(pool.clone())),
        ));

    // Spawn Kafka status consumer in background
    let pool_for_consumer = pool.clone();
    let brokers_for_consumer = cfg.kafka.brokers.clone();
    let group_for_consumer = cfg.kafka.group_id.clone();
    tokio::spawn(async move {
        if let Err(e) = start_status_consumer(
            &brokers_for_consumer,
            &group_for_consumer,
            pool_for_consumer,
        )
        .await
        {
            tracing::error!("Status consumer error: {e}");
        }
    });

    let jwt_secret = std::env::var("AUTH__JWT_SECRET").context("AUTH__JWT_SECRET not set")?;
    let jwt = Arc::new(JwtService::new(&jwt_secret, 3600, 86400));

    // Application services
    //
    // Online payment at booking ("pay for a quote_token shipment") is an
    // optional capability — see `Config::payment_config()`. All three of
    // `payments.url`, `quote_token_secret`, and `app.public_base_url` must be
    // set or the capability is disabled entirely; shipment creation,
    // tracking, cancellation, and bulk create are all unaffected either way.
    let payment = match cfg.payment_config() {
        Ok(pc) => {
            tracing::info!("payment config present — online payment at booking ENABLED");
            Some(PaymentCapability {
                client: Arc::new(PaymentsClient::new(&pc.payments_url)),
                quote_token_secret: pc.quote_token_secret,
                shipment_return_url_base: pc.public_base_url,
            })
        }
        Err(missing) => {
            tracing::warn!(
                missing = ?missing,
                "online payment at booking is DISABLED — POST /v1/shipments/quote will \
                 return 503, and POST /v1/shipments will reject any request carrying a \
                 quote_token; normal cash bookings, tracking, and cancellation are unaffected"
            );
            None
        }
    };
    let payment_enabled = payment.is_some();
    let svc = Arc::new(ShipmentService::new(
        repo.clone(),
        publisher,
        normalizer,
        awb_generator,
        payment,
    ));
    let query = Arc::new(ShipmentQueryService::new(repo.clone()));
    let pool_for_dims = pool.clone();

    // Spawn Kafka payment consumer — closes the loop `create()` opens when a
    // quote token is presented: on `payment.intent.captured` it republishes
    // the shipment's held dispatch events and marks it Paid; on
    // `payment.intent.failed` it cancels the shipment.
    // Constructed inside the spawn, like the status consumer above: a Kafka
    // client that cannot be created at boot must disable this consumer, not
    // stop the HTTP surface from binding. Shipment creation, tracking, and
    // cancellation all serve fine without Kafka.
    //
    // Only spawned when payment is enabled — with no payment config, no
    // shipment can ever reach `awaiting_payment`, so there is nothing for
    // this consumer to act on.
    if payment_enabled {
        let brokers_for_payment = cfg.kafka.brokers.clone();
        let group_for_payment = cfg.kafka.group_id.clone();
        let svc_for_payment = Arc::clone(&svc);
        tokio::spawn(async move {
            match PaymentConsumer::new(&brokers_for_payment, &group_for_payment, svc_for_payment) {
                Ok(consumer) => consumer.run().await,
                Err(e) => tracing::error!("Payment consumer could not start: {e}"),
            }
        });
    }

    // Payment-expiry sweep — backstop for shipments left `awaiting_payment`
    // past their TTL. Under normal operation the payments service's own
    // intent sweep (`INTENT_TTL` = 30 min, `services/payments/src/application
    // /services/payment_intent_service.rs`) publishes `payment.intent.failed`
    // first, which the consumer above already cancels via. This sweep exists
    // for anything that slips past that path. TTL here matches `INTENT_TTL`
    // so the two don't silently drift apart. Same "only when enabled" gate
    // as the consumer above, for the same reason.
    if payment_enabled {
        let svc_for_sweep = Arc::clone(&svc);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(5 * 60));
            loop {
                tick.tick().await;
                match svc_for_sweep.sweep_expired_payments(30).await {
                    Ok(count) if count > 0 => tracing::info!(count, "Shipment payment sweep: cancelled stale bookings"),
                    Ok(_) => {}
                    Err(e) => tracing::error!(err = ?e, "Shipment payment sweep failed"),
                }
            }
        });
    }

    // Axum router
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

    let state = AppState {
        svc,
        query,
        jwt,
        pool: pool_for_dims,
    };
    let app = router(state)
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .layer(cors);

    let addr: SocketAddr = format!("{}:{}", cfg.app.host, cfg.app.port).parse()?;
    tracing::info!(addr = %addr, "order-intake service listening");

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
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("SIGTERM")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
}
