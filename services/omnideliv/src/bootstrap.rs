use std::sync::Arc;

use anyhow::Context;
use logisticos_auth::jwt::JwtService;
use sqlx::postgres::PgPoolOptions;

use crate::api::http::{router, AppState};
use crate::application::services::{BasketService, CatalogService};
use crate::config::Config;
use crate::infrastructure::db::{
    PgBasketRepository, PgCatalogRepository, PgOrderRepository, PgTelemetryRepository,
    PgVendorRepository,
};
use crate::infrastructure::external::{BasketServiceAdapter, CatalogServiceAdapter, FieldOpsDispatch};
use crate::application::services::{CheckoutService, RecoveryService};
use crate::domain::repositories::{OrderRepository, TelemetryRepository, VendorLedgerRepository};
use crate::infrastructure::db::PgVendorLedgerRepository;
use crate::infrastructure::messaging::{CourierMilestoneHandler, TOPIC_COURIER};
use crate::infrastructure::db::PgMeshSessionStore;

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load().context("Failed to load omnideliv config")?;

    let otlp = std::env::var("OTLP_ENDPOINT").ok();
    logisticos_tracing::init(logisticos_tracing::TracingConfig {
        service_name: "omnideliv",
        env: &cfg.app.env,
        otlp_endpoint: otlp.as_deref(),
        log_level: None,
    })?;

    tracing::info!(env = %cfg.app.env, "omnideliv service starting");

    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                sqlx::query("SET search_path TO omnideliv, public")
                    .execute(&mut *conn)
                    .await?;
                Ok(())
            })
        })
        .connect(&cfg.database.url)
        .await
        .context("Failed to connect to PostgreSQL")?;

    logisticos_common::migrations::run(&pool, "omnideliv", &sqlx::migrate!("./migrations"))
        .await
        .context("omnideliv migration failed")?;

    let jwt_secret = std::env::var("AUTH__JWT_SECRET").context("AUTH__JWT_SECRET not set")?;
    let jwt = Arc::new(JwtService::new(&jwt_secret, 3600, 86400));

    let catalog = Arc::new(CatalogService::new(
        Arc::new(PgVendorRepository::new(pool.clone())),
        Arc::new(PgCatalogRepository::new(pool.clone())),
        cfg.stock_freshness_mins,
    ));
    let baskets = Arc::new(BasketService::new(
        Arc::new(PgBasketRepository::new(pool.clone())),
        Arc::new(PgVendorRepository::new(pool.clone())),
        Arc::new(PgCatalogRepository::new(pool.clone())),
    ));

    // The mesh writes through BasketService like every other caller, so it
    // inherits the optimistic lock rather than opening a second write path.
    let mesh = Arc::new(omnideliv_mesh::MeshRunner::new(
        Arc::new(logisticos_agent_runtime::claude::ClaudeClient::new(
            cfg.claude_api_key.clone(),
            cfg.claude_model.clone(),
            cfg.claude_max_tokens,
        )),
        Arc::new(omnideliv_mesh::tools::MeshToolBox::new(
            Arc::new(CatalogServiceAdapter::new(catalog.clone())),
            // KNOWN LIMITATION: the tool box binds tenant and delivery address
            // at construction, so today every run searches from the configured
            // default rather than the customer's address. Correct for a
            // single-tenant slice-one deployment and wrong the moment there are
            // two; the fix is to build the tool box per run inside
            // `MeshRunner::run`, which needs the request context threaded in.
            cfg.default_tenant_id,
            cfg.default_lat,
            cfg.default_lng,
        )),
        Arc::new(PgMeshSessionStore::new(pool.clone())),
        Arc::new(BasketServiceAdapter::new(baskets.clone())),
        omnideliv_mesh::MeshConfig::default(),
    ));

    let checkout = Arc::new(CheckoutService::new(
        Arc::new(PgBasketRepository::new(pool.clone())),
        Arc::new(PgVendorRepository::new(pool.clone())),
        Arc::new(FieldOpsDispatch::new(
            cfg.field_ops_url.clone(),
            cfg.service_token.clone(),
        )),
    ));
    let orders: Arc<dyn OrderRepository> = Arc::new(PgOrderRepository::new(pool.clone()));
    let telemetry: Arc<dyn TelemetryRepository> = Arc::new(PgTelemetryRepository::new(pool.clone()));

    let state = Arc::new(AppState {
        catalog,
        baskets,
        mesh,
        checkout,
        orders: orders.clone(),
        telemetry: telemetry.clone(),
        jwt,
    });

    // Courier milestones. Spawned rather than awaited so the HTTP surface comes
    // up regardless: a broker outage must not take down checkout and browse,
    // which do not need it.
    //
    // The consumer commits only after the handler returns Ok, so a failed
    // ledger credit is redelivered rather than dropped — which is what the
    // credit-before-advance ordering in the handler relies on.
    let ledgers: Arc<dyn VendorLedgerRepository> =
        Arc::new(PgVendorLedgerRepository::new(pool.clone()));
    let milestones = Arc::new(CourierMilestoneHandler::new(
        orders.clone(),
        ledgers,
        telemetry.clone(),
    ));

    match logisticos_events::consumer::KafkaConsumer::new(
        &cfg.kafka.brokers,
        "omnideliv-courier-milestones",
        &[TOPIC_COURIER],
    ) {
        Ok(consumer) => {
            tokio::spawn(async move {
                let r = consumer
                    .run(|_topic, payload| {
                        let h = milestones.clone();
                        async move {
                            // A payload this service cannot parse is another
                            // product's shape or a version skew. Log and commit
                            // rather than looping on it forever — the
                            // alternative is one poison message blocking every
                            // later milestone on the partition.
                            match serde_json::from_value(payload.clone()) {
                                Ok(event) => h.handle(event).await,
                                Err(e) => {
                                    tracing::error!(err = %e, %payload,
                                        "unparseable courier milestone, skipping");
                                    Ok(())
                                }
                            }
                        }
                    })
                    .await;

                if let Err(e) = r {
                    tracing::error!(err = %e, "courier milestone consumer stopped");
                }
            });
        }
        Err(e) => {
            tracing::error!(err = %e, brokers = %cfg.kafka.brokers,
                "Kafka unavailable — courier milestones will NOT be consumed, so orders will                  stay in their placed state and vendor ledgers will not be credited");
        }
    }

    // Stuck-order recovery. A timer rather than an event handler, because a
    // stuck order is defined by an event that never arrived — nothing
    // event-driven can notice its absence.
    let recovery = Arc::new(RecoveryService::new(orders.clone(), telemetry.clone()));
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(60));
        // The first tick fires immediately, which would sweep before the
        // service has finished coming up. Skip it.
        tick.tick().await;
        loop {
            tick.tick().await;
            match recovery.sweep().await {
                Ok(0) => {}
                Ok(n) => tracing::warn!(escalated = n, "recovery sweep escalated stuck orders"),
                Err(e) => tracing::error!(err = %e, "recovery sweep failed"),
            }
        }
    });

    let addr = format!("0.0.0.0:{}", cfg.app.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(%addr, "omnideliv listening");
    axum::serve(listener, router(state)).await?;

    Ok(())
}
