use std::sync::Arc;

use anyhow::Context;
use logisticos_auth::jwt::JwtService;
use sqlx::postgres::PgPoolOptions;

use crate::api::http::{router, AppState};
use crate::application::services::order_payments::OrderPayments;
use crate::application::services::{BasketService, CatalogService};
use crate::config::Config;
use crate::infrastructure::db::{
    PgBasketRepository, PgCatalogRepository, PgOrderRepository, PgTelemetryRepository,
    PgVendorRepository,
};
use crate::infrastructure::external::{
    BasketServiceAdapter, CatalogServiceAdapter, FieldOpsDispatch, OmniPaymentsClient,
};
use crate::application::services::{CheckoutService, RecoveryService};
use crate::domain::repositories::{
    OrderRepository, TelemetryRepository, VendorLedgerRepository, VendorRepository,
};
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

    // One client for both directions of the field-ops conversation: dispatch
    // (write) and supply (read). Same base url, same signer, one connection
    // pool — and `CourierSupply` is a separate trait so the Fleet agent's
    // capacity question can never dispatch as a side effect of being asked.
    let field_ops = Arc::new(FieldOpsDispatch::new(cfg.field_ops_url.clone(), jwt.clone()));

    // The mesh writes through BasketService like every other caller, so it
    // inherits the optimistic lock rather than opening a second write path.
    let mesh = Arc::new(omnideliv_mesh::MeshRunner::new(
        Arc::new(logisticos_agent_runtime::claude::ClaudeClient::new(
            cfg.claude_api_key.clone(),
            cfg.claude_model.clone(),
            cfg.claude_max_tokens,
        )),
        Arc::new(PgMeshSessionStore::new(pool.clone())),
        Arc::new(BasketServiceAdapter::new(baskets.clone())),
        // The runner holds the catalog and builds a tool box per run, binding
        // that run's tenant and the customer's own delivery point. It also
        // reads this directly during reconcile to verify proposed lines —
        // verification must not travel the model's tool surface, because
        // checking a model's output against facts the model supplied checks
        // nothing.
        Arc::new(CatalogServiceAdapter::new(catalog.clone()).with_supply(field_ops.clone())),
        omnideliv_mesh::MeshConfig {
            fanout_deadline: std::time::Duration::from_secs(cfg.mesh_deadline_secs),
            ..omnideliv_mesh::MeshConfig::default()
        },
    ));

    // OmniDeliv's prepaid-checkout foundation: authorize-then-capture-or-void
    // against `services/payments`' mesh-internal payment-intent routes. A
    // deployment with this misconfigured still boots and still takes COD
    // orders — only `PaymentMethod::Online` checkout fails, and it fails
    // loudly per-request (a 5xx from `payments.authorize`) rather than at
    // startup.
    let payments: Arc<dyn OrderPayments> = Arc::new(OmniPaymentsClient::new(cfg.payments_url.clone()));

    let checkout = Arc::new(CheckoutService::new(
        Arc::new(PgBasketRepository::new(pool.clone())),
        Arc::new(PgVendorRepository::new(pool.clone())),
        // The signer, not a token: field-ops validates `exp` and reads
        // `tenant_id` from the claim set, so the token has to be minted per
        // call with the caller's tenant. See field_ops_dispatch.rs.
        field_ops.clone(),
        payments.clone(),
        cfg.payment_currency.clone(),
        cfg.payment_return_url_base.clone(),
    ));
    let orders: Arc<dyn OrderRepository> = Arc::new(PgOrderRepository::new(pool.clone()));
    let telemetry: Arc<dyn TelemetryRepository> = Arc::new(PgTelemetryRepository::new(pool.clone()));

    // One vendor repository Arc held directly on AppState, for the tracking
    // read to draw stop coordinates from — separate from the `PgVendorRepository`
    // instances threaded into `CatalogService`, `BasketService` and
    // `CheckoutService` above, each of which owns its own.
    let vendors: Arc<dyn VendorRepository> = Arc::new(PgVendorRepository::new(pool.clone()));

    // One ledger repository for both readers: the milestone consumer that
    // credits it and the vendor endpoint that reads it back.
    let ledgers: Arc<dyn VendorLedgerRepository> =
        Arc::new(PgVendorLedgerRepository::new(pool.clone()));

    // One producer for the order events. A broker that is unreachable at
    // startup degrades to Noop rather than refusing to boot: a customer who
    // cannot order is worse off than one who is not messaged.
    let producer = match logisticos_events::producer::KafkaProducer::new(&cfg.kafka.brokers) {
        Ok(p) => Some(Arc::new(p)),
        Err(e) => {
            tracing::error!(err = %e, brokers = %cfg.kafka.brokers,
                "Kafka unavailable — order confirmations, delivery notices and \
                 vendor order alerts will NOT be sent");
            None
        }
    };

    let order_events: Arc<dyn crate::infrastructure::messaging::OrderEvents> = match &producer {
        Some(p) => Arc::new(crate::infrastructure::messaging::KafkaOrderEvents::new(p.clone())),
        None => Arc::new(crate::infrastructure::messaging::NoopOrderEvents),
    };

    // Shares the one producer above rather than opening a second connection to
    // the same brokers: two publishers, one client.
    let vendor_events: Arc<dyn crate::infrastructure::messaging::VendorLegEvents> = match &producer {
        Some(p) => Arc::new(crate::infrastructure::messaging::KafkaVendorLegEvents::new(p.clone())),
        None => Arc::new(crate::infrastructure::messaging::NoopVendorLegEvents),
    };

    // The guarded single-leg writer. Deliberately not the order repository:
    // that writes a whole order last-write-wins, which would let one tablet
    // silently overwrite another tablet's acceptance.
    // Cloned before `vendor_events` is moved into AppState, for the
    // payment-authorized consumer that notifies stores on the online path.
    let vendor_events_for_payments = vendor_events.clone();
    // And again for the unanswered-leg sweep, which re-alerts by republishing
    // the same event checkout published.
    let vendor_events_for_recovery = vendor_events.clone();

    // Bounds the unauthenticated scan endpoint. Per-process, so N replicas
    // allow N times the rate — see the module docs for why that trade was made
    // over adding Redis to this service.
    let scan_limiter = Arc::new(crate::api::http::scan_limit::ScanLimiter::new());

    let venues: Arc<dyn crate::domain::repositories::VenueRepository> =
        Arc::new(crate::infrastructure::db::PgVenueRepository::new(pool.clone()));

    let legs: Arc<dyn crate::domain::repositories::VendorLegRepository> =
        Arc::new(crate::infrastructure::db::PgVendorLegRepository::new(pool.clone()));
    let legs_for_recovery = legs.clone();

    // Storage is optional. An environment with no STORAGE__* vars keeps
    // serving catalogs; only the photo routes go dark, and they say so.
    let photos = if cfg.storage.endpoint.trim().is_empty() {
        tracing::info!("omnideliv: no STORAGE__ENDPOINT — product photos disabled");
        None
    } else {
        match crate::infrastructure::storage::PhotoStorage::new(&cfg.storage).await {
            Ok(s) => {
                if let Err(e) = s.ensure_bucket().await {
                    // Not fatal: the bucket may be created out of band, and a
                    // boot failure here would take the whole catalog down for
                    // a feature that is additive.
                    tracing::warn!(error = ?e, "omnideliv: could not ensure the photo bucket");
                }
                Some(Arc::new(s))
            }
            Err(e) => {
                tracing::warn!(error = ?e, "omnideliv: photo storage misconfigured — photos disabled");
                None
            }
        }
    };

    let state = Arc::new(AppState {
        catalog,
        baskets,
        mesh,
        checkout,
        orders: orders.clone(),
        telemetry: telemetry.clone(),
        ledgers: ledgers.clone(),
        order_events: order_events.clone(),
        jwt,
        photos,
        courier_telemetry: field_ops.clone(),
        vendors:           vendors.clone(),
        legs,
        vendor_events,
        venues,
        table_session_mins:  cfg.table_session_mins,
        table_session_cap:   cfg.table_session_cap,
        table_scan_base_url: cfg.table_scan_base_url.clone(),
        online_payment_enabled: cfg.online_payment_enabled,
        scan_limiter:        scan_limiter.clone(),
    });

    // Courier milestones. Spawned rather than awaited so the HTTP surface comes
    // up regardless: a broker outage must not take down checkout and browse,
    // which do not need it.
    //
    // The consumer commits only after the handler returns Ok, so a failed
    // ledger credit is redelivered rather than dropped — which is what the
    // credit-before-advance ordering in the handler relies on.
    let milestones = Arc::new(CourierMilestoneHandler::new(
        orders.clone(),
        ledgers,
        telemetry.clone(),
        order_events,
        payments.clone(),
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

    // `payment.intent.authorized` / `payment.intent.failed` — the deferred
    // half of `PaymentMethod::Online` checkout. Without this, an online order
    // opens an authorization hold and then sits forever: no courier is ever
    // offered, and a declined/expired payment never cancels the order. See
    // `infrastructure::messaging::payment_consumer`'s module doc comment.
    match logisticos_events::consumer::KafkaConsumer::new(
        &cfg.kafka.brokers,
        "omnideliv-payment-intents",
        &[
            logisticos_events::topics::PAYMENT_INTENT_AUTHORIZED,
            logisticos_events::topics::PAYMENT_INTENT_FAILED,
        ],
    ) {
        Ok(consumer) => {
            let orders_for_payments = orders.clone();
            let telemetry_for_payments = telemetry.clone();
            let dispatch_for_payments: Arc<dyn crate::application::services::CourierDispatch> =
                field_ops.clone();
            tokio::spawn(async move {
                let r = consumer
                    .run(move |topic, payload| {
                        let orders = orders_for_payments.clone();
                        let telemetry = telemetry_for_payments.clone();
                        let dispatch = dispatch_for_payments.clone();
                        let vendor_events = vendor_events_for_payments.clone();
                        async move {
                            crate::infrastructure::messaging::payment_consumer::handle(
                                &topic, payload, &orders, &telemetry, &dispatch, &vendor_events,
                            )
                            .await
                        }
                    })
                    .await;

                if let Err(e) = r {
                    tracing::error!(err = %e, "omnideliv payment-intent consumer stopped");
                }
            });
        }
        Err(e) => {
            tracing::error!(err = %e, brokers = %cfg.kafka.brokers,
                "Kafka unavailable — payment-intent events will NOT be consumed, so an online                  order will never have its courier offered and a failed payment will never cancel it");
        }
    }

    // Stuck-order recovery. A timer rather than an event handler, because a
    // stuck order is defined by an event that never arrived — nothing
    // event-driven can notice its absence.
    let recovery = Arc::new(RecoveryService::new(
        orders.clone(),
        telemetry.clone(),
        // The same field-ops client checkout uses, so a re-offer is
        // indistinguishable from a first offer at the platform tier.
        field_ops.clone(),
        payments.clone(),
        cfg.online_no_courier_timeout_mins,
    ));
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

    // Legs nobody answered. A separate sweep from the stuck-order one above
    // because it asks a different question on a different clock: that one asks
    // whether a courier took the order, this one whether the store even looked.
    let leg_recovery = Arc::new(crate::application::services::LegRecoveryService::new(
        legs_for_recovery,
        vendor_events_for_recovery,
        telemetry.clone(),
    ));
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(60));
        // Same skipped first tick as above: a sweep that fires at boot would
        // re-alert every open leg every time a pod restarts.
        tick.tick().await;
        loop {
            tick.tick().await;
            match leg_recovery.sweep().await {
                Ok(0) => {}
                Ok(n) => tracing::warn!(escalated = n, "vendor legs still unanswered"),
                Err(e) => tracing::error!(err = %e, "vendor leg sweep failed"),
            }
        }
    });

    // Evicting on a timer rather than inline: sweeping per request would make
    // one request's cost proportional to how many keys exist, which is exactly
    // what an attacker would drive up.
    let limiter_for_sweep = scan_limiter.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(300));
        tick.tick().await;
        loop {
            tick.tick().await;
            let dropped = limiter_for_sweep.evict_stale(chrono::Utc::now());
            if dropped > 0 {
                tracing::debug!(dropped, "evicted stale scan rate-limit buckets");
            }
        }
    });

    let addr = format!("0.0.0.0:{}", cfg.app.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(%addr, "omnideliv listening");
    axum::serve(listener, router(state)).await?;

    Ok(())
}
