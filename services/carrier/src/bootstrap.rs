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
        adapters::{
            AdapterRegistry,
            dhl::DhlAdapter,
            fedex::FedexAdapter,
            ups::UpsAdapter,
            tnt::TntAdapter,
            dpd::DpdAdapter,
            aramex::AramexAdapter,
        },
        cache::CachedCarrierRepository,
        db::{PgCarrierBookingRepository, PgCarrierRepository, PgMarketplaceRepository, PgSlaRecordRepository},
        messaging::{start_allocation_booking_worker, start_delivery_consumer, CarrierPublisher},
        storage::{S3StorageAdapter, StorageAdapter},
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
    let booking_repo     = Arc::new(PgCarrierBookingRepository::new(pool.clone()));

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
    let marketplace_svc_inner = MarketplaceService::new(
        Arc::clone(&marketplace_repo) as Arc<dyn crate::domain::repositories::MarketplaceRepository>,
        Arc::clone(&kafka_producer),
    );
    // Online booking payment. Absent config leaves `payments: None`, which
    // refuses `payment_method: "online"` with a clear business-rule error and
    // leaves the `invoice` path -- the only one that existed before -- exactly
    // as it was.
    let marketplace_svc = Arc::new(if cfg.services.payments_url.is_empty() {
        tracing::info!("SERVICES__PAYMENTS_URL unset — marketplace bookings are invoice-only");
        marketplace_svc_inner
    } else {
        let payments: Arc<dyn crate::application::services::BookingPayments> =
            Arc::new(crate::infrastructure::payments_client::CarrierPaymentsClient::new(
                cfg.services.payments_url.clone(),
            ));
        marketplace_svc_inner.with_payments(
            payments,
            cfg.services.booking_currency.clone(),
            cfg.services.booking_payment_return_url.clone(),
        )
    });

    // S3/R2 object storage for carrier label files.
    let s3_bucket     = std::env::var("S3_BUCKET").unwrap_or_else(|_| "logisticos-carrier-labels".into());
    let s3_endpoint   = std::env::var("S3_ENDPOINT_URL").ok();
    let s3_region     = std::env::var("S3_REGION").or_else(|_| std::env::var("AWS_REGION")).ok();
    let s3_path_style = std::env::var("S3_FORCE_PATH_STYLE")
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false);
    let s3_creds = match (cfg.s3.access_key_id.clone(), cfg.s3.secret_access_key.clone()) {
        (Some(k), Some(s)) => Some((k, s)),
        _ => None,
    };

    let storage: Option<Arc<dyn StorageAdapter>> = match S3StorageAdapter::new(
        s3_endpoint, s3_bucket, s3_region, s3_path_style, s3_creds,
    ).await {
        Ok(adapter) => {
            tracing::info!("Carrier label storage (S3/R2) ready");
            Some(Arc::new(adapter) as Arc<dyn StorageAdapter>)
        }
        Err(e) => {
            tracing::warn!("S3/R2 storage unavailable — labels will not be stored: {e}");
            None
        }
    };

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

    // Spawn hub-carrier consumer — books a 3PL SLA allocation when hub-ops (or
    // dispatch Auto fallback) requests carrier last-mile for a deconsolidated shipment.
    {
        let brokers   = cfg.kafka.brokers.clone();
        let group_id  = cfg.kafka.group_id.clone();
        let svc       = Arc::clone(&carrier_svc);
        let rx        = shutdown_rx.clone();
        tokio::spawn(async move {
            if let Err(e) = crate::infrastructure::hub_carrier_consumer::start_hub_carrier_consumer(
                &brokers, &group_id, svc, rx,
            ).await {
                tracing::error!("Hub-carrier consumer exited with error: {e}");
            }
        });
    }

    // `payment.intent.authorized` / `payment.intent.failed` for bookings.
    // Without this an online booking opens a hold and is then shown to nobody:
    // the carrier never sees it, the response-window clock never starts, and a
    // declined payment never cancels the booking.
    {
        let brokers  = cfg.kafka.brokers.clone();
        let group_id = cfg.kafka.group_id.clone();
        let repo     = Arc::clone(&marketplace_repo)
            as Arc<dyn crate::domain::repositories::MarketplaceRepository>;
        let rx       = shutdown_rx.clone();
        tokio::spawn(async move {
            if let Err(e) =
                crate::infrastructure::booking_payment_consumer::start_booking_payment_consumer(
                    &brokers, &group_id, repo, rx,
                ).await
            {
                tracing::error!("Booking-payment consumer exited with error: {e}");
            }
        });
    }

    // The carrier-response window. `carrier_response_window_mins` has been a
    // field on every listing since the marketplace tables landed and nothing
    // has ever enforced it; with money attached, an unanswered booking now
    // means a merchant's funds ring-fenced indefinitely.
    {
        let svc    = Arc::clone(&marketplace_svc);
        let mut rx = shutdown_rx.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                tokio::select! {
                    _ = rx.changed() => {
                        if *rx.borrow_and_update() { break; }
                    }
                    _ = tick.tick() => {
                        match svc.sweep_expired_bookings().await {
                            Ok(n) if n > 0 => tracing::info!(expired = n, "marketplace booking sweep"),
                            Ok(_) => {}
                            Err(e) => tracing::error!(err = %e, "marketplace booking sweep failed"),
                        }
                    }
                }
            }
        });
    }

    // Build 3PL adapter registry — only adapters with credentials are registered.
    let mut registry = AdapterRegistry::new();

    if let Some(c) = &cfg.carriers.dhl {
        registry.register(Arc::new(DhlAdapter::new(
            c.base_url.clone(), c.account_number.clone(), c.api_key.clone(), c.api_secret.clone(),
        )));
        tracing::info!("DHL adapter registered (base: {})", c.base_url);
    }
    if let Some(c) = &cfg.carriers.fedex {
        registry.register(Arc::new(FedexAdapter::new(
            c.base_url.clone(), c.client_id.clone(), c.client_secret.clone(), c.account_number.clone(),
        )));
        tracing::info!("FedEx adapter registered (base: {})", c.base_url);
    }
    if let Some(c) = &cfg.carriers.ups {
        registry.register(Arc::new(UpsAdapter::new(
            c.base_url.clone(), c.client_id.clone(), c.client_secret.clone(), c.account_number.clone(),
        )));
        tracing::info!("UPS adapter registered (base: {})", c.base_url);
    }
    if let Some(c) = &cfg.carriers.tnt {
        registry.register(Arc::new(TntAdapter::new(
            c.base_url.clone(), c.client_id.clone(), c.client_secret.clone(), c.account_number.clone(),
        )));
        tracing::info!("TNT adapter registered (via FedEx infra, base: {})", c.base_url);
    }
    if let Some(c) = &cfg.carriers.dpd {
        registry.register(Arc::new(DpdAdapter::new(
            c.base_url.clone(), c.username.clone(), c.password.clone(), c.account.clone(),
        )));
        tracing::info!("DPD adapter registered (base: {})", c.base_url);
    }
    if let Some(c) = &cfg.carriers.aramex {
        registry.register(Arc::new(AramexAdapter::new(
            c.base_url.clone(),
            c.username.clone(),
            c.password.clone(),
            c.version.clone(),
            c.entity.clone(),
            c.account_number.clone(),
            c.account_pin.clone(),
            c.account_country_code.clone(),
        )));
        tracing::info!("Aramex adapter registered (base: {})", c.base_url);
    }

    let active_codes: Vec<_> = registry.codes().iter().map(|s| s.to_string()).collect();
    if active_codes.is_empty() {
        tracing::warn!("No 3PL carrier adapters configured — MCP carrier tools will return NotConfigured errors");
    } else {
        tracing::info!(carriers = ?active_codes, "Carrier adapter registry ready");
    }

    let adapter_registry = Arc::new(registry);

    // Spawn G4 — allocation booking worker (CARRIER_ALLOCATED → 3PL book_shipment)
    {
        let brokers          = cfg.kafka.brokers.clone();
        let group_id         = cfg.kafka.group_id.clone();
        let c_repo           = Arc::clone(&carrier_repo);
        let registry_arc     = Arc::clone(&adapter_registry);
        let b_repo           = Arc::clone(&booking_repo);
        let storage_arc      = storage.clone();
        let order_intake_url = cfg.services.order_intake_url.clone();
        let rx               = shutdown_rx.clone();
        tokio::spawn(async move {
            if let Err(e) = start_allocation_booking_worker(
                brokers, group_id, c_repo, registry_arc,
                b_repo, storage_arc, order_intake_url, rx,
            ).await {
                tracing::error!("Allocation booking worker exited with error: {e}");
            }
        });
    }

    let jwt_secret = std::env::var("AUTH__JWT_SECRET")
        .context("AUTH__JWT_SECRET env var not set")?;
    let jwt = Arc::new(JwtService::new(&jwt_secret, 3600, 86400));

    // Clone carrier_svc before moving state into the router — gRPC server
    // needs its own Arc handle and `with_state` consumes `state`.
    let carrier_svc_for_grpc = Arc::clone(&carrier_svc);

    let state = AppState {
        carrier_svc,
        marketplace_svc,
        jwt: Arc::clone(&jwt),
        adapter_registry,
        storage,
        booking_repo: Some(booking_repo),
    };

    // Compose the app:
    //   • JWT-protected routes (all /v1/* except webhooks)
    //   • MCP server routes (/mcp/*) — protected by the same JWT middleware
    //   • Unauthenticated webhook routes (/v1/webhooks/*)
    // Both share AppState; `propagate_request_id` and TraceLayer wrap the whole app.
    let app = http::router()
        .merge(mcp_router())
        .layer(axum::middleware::from_fn_with_state(jwt, logisticos_auth::middleware::require_auth))
        .merge(http::observability_router())
        .merge(http::webhook_router())
        .layer(axum::middleware::from_fn(propagate_request_id))
        .layer(tower_http::trace::TraceLayer::new_for_http())
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

// ── MCP router ───────────────────────────────────────────────────────────────

fn mcp_router() -> axum::Router<AppState> {
    use axum::{
        extract::State,
        http::{HeaderMap, StatusCode},
        response::{IntoResponse, Json},
        routing::{get, post},
        Router,
    };
    use serde_json::{json, Value};
    use std::time::Instant;
    use crate::mcp::{auth, audit, tools};

    async fn tools_list() -> impl IntoResponse {
        Json(json!({ "tools": tools::list() }))
    }

    async fn tools_call(
        State(state): State<AppState>,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> impl IntoResponse {
        let state = Arc::new(state);
        let ctx = match auth::extract_context(&headers, &state.jwt) {
            Ok(c)  => c,
            Err(e) => return (StatusCode::UNAUTHORIZED, Json(json!({ "error": e }))).into_response(),
        };

        let tool_name = match body["params"]["name"].as_str() {
            Some(n) => n.to_string(),
            None    => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "params.name is required" }))).into_response(),
        };

        let args  = body["params"]["arguments"].clone();
        let start = Instant::now();

        match tools::dispatch(&tool_name, &args, &ctx, &state).await {
            Ok(result) => {
                audit::audit_tool_call(&ctx, &tool_name, true, start);
                Json(json!({ "result": result })).into_response()
            }
            Err(e) => {
                audit::audit_tool_call(&ctx, &tool_name, false, start);
                (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": e }))).into_response()
            }
        }
    }

    Router::new()
        .route("/mcp/tools",      get(tools_list))
        .route("/mcp/tools/call", post(tools_call))
}

// ─────────────────────────────────────────────────────────────────────────────

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
