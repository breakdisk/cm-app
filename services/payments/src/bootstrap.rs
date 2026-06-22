use std::sync::Arc;
use sqlx::postgres::PgPoolOptions;
use anyhow::Context;
use chrono::Datelike as _;
use crate::config::Config;
use crate::application::services::{
    BillingAggregationService, CarrierSettlementService, CodRemittanceService, CodService,
    InvoiceService, WalletService, WithdrawalService,
};
use crate::infrastructure::cache::RedisSequenceSource;
use crate::infrastructure::db::{
    PgBillingRunRepository, PgCodRemittanceBatchRepository, PgCodRepository,
    PgInvoiceRepository, PgWalletRepository, PgMerchantBillingAccountRepository,
    PgWithdrawalRequestRepository, PgDriverLedgerRepository,
};
use crate::infrastructure::http::OrderIntakeClient;
use crate::api::http::{router, AppState};
use crate::infrastructure::messaging::{
    PodConsumer, WeightDiscrepancyConsumer, PickupCapturedConsumer, CustomsDutyConsumer,
    DeliveryCompletedConsumer,
};
use logisticos_auth::jwt::JwtService;
use logisticos_events::producer::KafkaProducer;

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load().context("Failed to load payments config")?;

    let otlp = std::env::var("OTLP_ENDPOINT").ok();
    logisticos_tracing::init(logisticos_tracing::TracingConfig {
        service_name: "payments",
        env: &cfg.app.env,
        otlp_endpoint: otlp.as_deref(),
        log_level: None,
    })?;

    tracing::info!(env = %cfg.app.env, "payments service starting");

    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .after_connect(|conn, _meta| Box::pin(async move {
            sqlx::query("SET search_path TO payments, public")
                .execute(&mut *conn)
                .await?;
            Ok(())
        }))
        .connect(&cfg.database.url)
        .await
        .context("Failed to connect to PostgreSQL")?;

    logisticos_common::migrations::run(&pool, "payments", &sqlx::migrate!("./migrations")).await
        .context("Payments migration failed")?;

    let kafka = Arc::new(
        KafkaProducer::new(&cfg.kafka.brokers).context("Kafka connection failed")?
    );

    let jwt_secret = std::env::var("AUTH__JWT_SECRET").context("AUTH__JWT_SECRET not set")?;
    let jwt = Arc::new(JwtService::new(&jwt_secret, 3600, 86400));

    let invoice_repo     = Arc::new(PgInvoiceRepository::new(pool.clone()));
    let cod_repo         = Arc::new(PgCodRepository::new(pool.clone()));
    let cod_batch_repo   = Arc::new(PgCodRemittanceBatchRepository::new(pool.clone()));
    let wallet_repo      = Arc::new(PgWalletRepository::new(pool.clone()));
    let withdrawal_repo  = Arc::new(PgWithdrawalRequestRepository::new(pool.clone()));
    let billing_run_repo = Arc::new(PgBillingRunRepository::new(pool.clone()));
    let merchant_billing_account_repo = Arc::new(
        PgMerchantBillingAccountRepository::new(pool.clone())
    );
    let sequence_source  = Arc::new(
        RedisSequenceSource::new(&cfg.redis.url).context("Failed to connect to Redis for sequences")?
    );
    let order_intake_client = Arc::new(OrderIntakeClient::new(&cfg.order_intake.url));

    let driver_ledger_repo = Arc::new(PgDriverLedgerRepository::new(pool.clone()));

    let partner_bonus_repo = Arc::new(
        crate::infrastructure::db::partner_bonus_repo::PgPartnerBonusRepo::new(pool.clone())
    );
    let commission_query = Arc::new(
        crate::application::queries::CommissionBreakdownQuery::new(pool.clone())
    );

    let invoice_service = Arc::new(InvoiceService::new(
        Arc::clone(&invoice_repo) as _,
        Arc::clone(&kafka),
        sequence_source as _,
        Arc::clone(&order_intake_client) as _,
    ));
    let cod_service = Arc::new(CodService::new(
        Arc::clone(&cod_repo) as _,
        Arc::clone(&order_intake_client) as _,
        Arc::clone(&kafka),
    ));
    let cod_remittance_service = Arc::new(CodRemittanceService::new(
        Arc::clone(&cod_repo) as _,
        Arc::clone(&cod_batch_repo) as _,
        Arc::clone(&wallet_repo) as _,
        Arc::clone(&kafka),
        Arc::clone(&merchant_billing_account_repo) as _,
    ));
    let wallet_service = Arc::new(WalletService::new(
        Arc::clone(&wallet_repo) as _,
    ));
    let withdrawal_service = Arc::new(WithdrawalService::new(
        Arc::clone(&wallet_repo) as _,
        Arc::clone(&withdrawal_repo),
        Arc::clone(&kafka),
    ));
    let billing_service = Arc::new(BillingAggregationService::new(
        Arc::clone(&billing_run_repo) as _,
        Arc::clone(&order_intake_client) as _,
        Arc::clone(&invoice_service),
    ));
    let carrier_settlement_service = Arc::new(CarrierSettlementService::new(
        pool.clone(),
        Arc::clone(&wallet_repo) as _,
        Arc::clone(&withdrawal_repo),
    ));

    let templates_dir = std::env::var("PAYMENTS_TEMPLATES_DIR")
        .unwrap_or_else(|_| "./templates".into());
    let pdf_renderer = match crate::application::services::PdfRenderer::new(&templates_dir).await {
        Ok(r) => {
            tracing::info!("PDF renderer initialised");
            Some(Arc::new(r))
        }
        Err(e) => {
            tracing::warn!(err = %e, "PDF renderer failed to initialise — /invoices/:id/pdf will return 503");
            None
        }
    };

    let state = Arc::new(AppState {
        invoice_service:                   Arc::clone(&invoice_service),
        cod_service:                       Arc::clone(&cod_service),
        cod_remittance_service:            Arc::clone(&cod_remittance_service),
        wallet_service,
        billing_service:                   Arc::clone(&billing_service),
        carrier_settlement_service:        Arc::clone(&carrier_settlement_service),
        jwt:                               Arc::clone(&jwt),
        merchant_billing_account_repo:     Arc::clone(&merchant_billing_account_repo) as _,
        commission_query:                  Arc::clone(&commission_query),
        partner_bonus_repo:                Arc::clone(&partner_bonus_repo),
        withdrawal_service,
        pdf_renderer,
        driver_ledger_repo:                Arc::clone(&driver_ledger_repo) as _,
    });
    let app = router(state);

    // Monthly merchant billing cron — runs on the 1st of each month and issues
    // shipment-charges invoices for every (tenant, merchant) pair configured in
    // BILLING_MERCHANT_CONFIG (JSON array of billing merchant descriptors).
    // If the env var is absent or empty, the cron skips with a warning.
    //
    // Each entry must have: tenant_id, merchant_id, tenant_code, merchant_email (optional).
    // Example: '[{"tenant_id":"...","merchant_id":"...","tenant_code":"PH1","merchant_email":"ops@example.com"}]'
    //
    // Billing is idempotent: re-running for the same (tenant, merchant, period) is a no-op.
    let billing_svc_for_cron = Arc::clone(&billing_service);
    tokio::spawn(async move {
        // Check every 6 hours so we don't miss the 1st of the month by more than a few hours.
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(6 * 3_600));
        let mut last_run_month: Option<(i32, u32)> = None;
        loop {
            tick.tick().await;
            let now = chrono::Utc::now();
            let (year, month, day) = (now.year(), now.month(), now.day());
            // Run on the 1st of each month, once per calendar month.
            if day != 1 { continue; }
            if last_run_month == Some((year, month)) { continue; }

            let merchants_json = std::env::var("BILLING_MERCHANT_CONFIG").unwrap_or_default();
            if merchants_json.trim().is_empty() {
                tracing::warn!("Monthly billing cron: BILLING_MERCHANT_CONFIG not set — skipping");
                last_run_month = Some((year, month));
                continue;
            }

            let merchants: Vec<serde_json::Value> = match serde_json::from_str(&merchants_json) {
                Ok(v) => v,
                Err(e) => {
                    tracing::error!(err = %e, "Monthly billing cron: failed to parse BILLING_MERCHANT_CONFIG");
                    last_run_month = Some((year, month));
                    continue;
                }
            };

            // Bill for the previous calendar month (the one that just ended).
            let (bill_year, bill_month) = if month == 1 { (year - 1, 12u32) } else { (year, month - 1) };
            tracing::info!(
                merchants  = merchants.len(),
                bill_year  = bill_year,
                bill_month = bill_month,
                "Monthly billing cron: starting run",
            );

            for m in &merchants {
                let Some(tenant_id) = m["tenant_id"].as_str().and_then(|s| s.parse::<uuid::Uuid>().ok()) else { continue; };
                let Some(merchant_id) = m["merchant_id"].as_str().and_then(|s| s.parse::<uuid::Uuid>().ok()) else { continue; };
                let tenant_code = m["tenant_code"].as_str().unwrap_or("PH1").to_owned();
                let merchant_email = m["merchant_email"].as_str().filter(|s| !s.is_empty()).map(str::to_owned);

                use crate::application::commands::RunBillingCommand;
                let cmd = RunBillingCommand {
                    tenant_id,
                    tenant_code,
                    merchant_id,
                    merchant_email,
                    year: bill_year,
                    month: bill_month,
                };
                if let Err(e) = billing_svc_for_cron.run_monthly(cmd).await {
                    tracing::error!(
                        err         = %e,
                        tenant_id   = %tenant_id,
                        merchant_id = %merchant_id,
                        "Monthly billing cron: failed for merchant",
                    );
                } else {
                    tracing::info!(
                        tenant_id   = %tenant_id,
                        merchant_id = %merchant_id,
                        "Monthly billing cron: invoice issued",
                    );
                }
            }
            last_run_month = Some((year, month));
        }
    });

    // Nightly COD auto-batching — groups all collected-but-unbatched COD by
    // (tenant, merchant) up to previous UTC midnight and creates remittance batches.
    // Finance can then confirm each batch via HTTP to trigger wallet credit.
    let remittance_svc_for_cron = Arc::clone(&cod_remittance_service);
    tokio::spawn(async move {
        // Run immediately on startup to catch any missed batch, then every 24 hours.
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(24 * 3_600));
        loop {
            tick.tick().await;
            // Cutoff = start of today UTC (i.e. everything collected before today)
            let cutoff = {
                use chrono::{TimeZone, Utc, NaiveTime};
                Utc.from_utc_datetime(
                    &chrono::Utc::now().date_naive().and_time(NaiveTime::MIN)
                )
            };
            if let Err(e) = remittance_svc_for_cron.run_daily_batching(cutoff).await {
                tracing::error!(err = %e, "COD daily batching cron failed");
            }
        }
    });

    // Spawn Kafka consumer for pod.captured — runs for the lifetime of the process.
    let pod_consumer = PodConsumer::new(
        &cfg.kafka.brokers,
        &cfg.kafka.group_id,
        Arc::clone(&cod_service),
        Arc::clone(&invoice_service),
    )
    .context("Failed to create PodConsumer")?;
    tokio::spawn(async move { pod_consumer.run().await });

    // Spawn weight-discrepancy consumer — appends surcharge adjustments to issued
    // invoices when hub-ops finds actual weight > declared weight.
    let (weight_shutdown_tx, weight_shutdown_rx) = tokio::sync::watch::channel(false);
    let weight_consumer = WeightDiscrepancyConsumer::new(
        &cfg.kafka.brokers,
        &cfg.kafka.group_id,
        Arc::clone(&invoice_service),
        Arc::clone(&invoice_repo) as Arc<dyn crate::domain::repositories::InvoiceRepository>,
    )
    .context("Failed to create WeightDiscrepancyConsumer")?;
    tokio::spawn(async move { weight_consumer.run(weight_shutdown_rx).await });

    // Spawn pickup.captured consumer — debits the driver's cash-flow ledger for
    // Track A (Balikbayan) pickups so finance can see liability before remittance.
    let pickup_consumer = PickupCapturedConsumer::new(
        &cfg.kafka.brokers,
        &cfg.kafka.group_id,
        Arc::clone(&driver_ledger_repo) as Arc<dyn crate::domain::repositories::DriverLedgerRepository>,
    )
    .context("Failed to create PickupCapturedConsumer")?;
    tokio::spawn(async move { pickup_consumer.run().await });

    // Spawn customs-duty consumer — generates a customs-duty invoice per payer
    // when a container clears customs with duties owed.
    let (_customs_duty_shutdown_tx, customs_duty_shutdown_rx) = tokio::sync::watch::channel(false);
    let customs_duty_consumer = CustomsDutyConsumer::new(
        &cfg.kafka.brokers,
        &cfg.kafka.group_id,
        Arc::clone(&invoice_service),
    )
    .context("Failed to create CustomsDutyConsumer")?;
    tokio::spawn(async move { customs_duty_consumer.run(customs_duty_shutdown_rx).await });

    // Spawn delivery-completed consumer — credits carrier wallets with delivery
    // margin and COD commission for every gig-driver delivery that has a carrier_id.
    let delivery_consumer = DeliveryCompletedConsumer::new(
        &cfg.kafka.brokers,
        &cfg.kafka.group_id,
        Arc::clone(&wallet_repo) as _,
    )
    .context("Failed to create DeliveryCompletedConsumer")?;
    tokio::spawn(async move { delivery_consumer.run().await });

    // Weekly carrier settlement cron — every Saturday at midnight UTC.
    // Sweeps all unsettled carrier wallet earnings (margin + COD commission)
    // and creates settlement run records + withdrawal requests for disbursement.
    let settlement_svc_for_cron = Arc::clone(&carrier_settlement_service);
    tokio::spawn(async move {
        // Poll every 30 minutes so we catch the Saturday window within half an hour.
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(30 * 60));
        let mut last_run_week: Option<(i32, u32)> = None;
        loop {
            tick.tick().await;
            let now = chrono::Utc::now();
            // ISO weekday: Saturday = 6
            if now.weekday().number_from_monday() != 6 { continue; }
            // Only run once per calendar week (identified by ISO year + week number).
            let iso_week = now.iso_week();
            let week_key = (iso_week.year(), iso_week.week());
            if last_run_week == Some(week_key) { continue; }

            let tenant_ids_json = std::env::var("SETTLEMENT_TENANT_IDS").unwrap_or_default();
            if tenant_ids_json.trim().is_empty() {
                tracing::warn!("Weekly carrier settlement cron: SETTLEMENT_TENANT_IDS not set — skipping");
                last_run_week = Some(week_key);
                continue;
            }

            let tenant_ids: Vec<uuid::Uuid> = match serde_json::from_str(&tenant_ids_json) {
                Ok(v) => v,
                Err(e) => {
                    tracing::error!(err = %e, "Weekly carrier settlement: failed to parse SETTLEMENT_TENANT_IDS");
                    last_run_week = Some(week_key);
                    continue;
                }
            };

            let period_end = (now - chrono::Duration::days(1)).date_naive();
            tracing::info!(
                tenants      = tenant_ids.len(),
                period_end   = %period_end,
                "Weekly carrier settlement cron: starting run",
            );

            for tenant_id in &tenant_ids {
                let t = logisticos_types::TenantId::from_uuid(*tenant_id);
                match settlement_svc_for_cron.run(&t, Some(period_end), uuid::Uuid::nil()).await {
                    Ok(outcomes) => tracing::info!(
                        tenant_id  = %tenant_id,
                        carriers   = outcomes.len(),
                        "Weekly carrier settlement: run complete",
                    ),
                    Err(e) => tracing::error!(
                        err       = %e,
                        tenant_id = %tenant_id,
                        "Weekly carrier settlement: run failed",
                    ),
                }
            }
            last_run_week = Some(week_key);
        }
    });

    let addr = format!("{}:{}", cfg.app.host, cfg.app.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("Failed to bind to {addr}"))?;

    tracing::info!(addr = %addr, "payments service listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("Payments server error")?;

    weight_shutdown_tx.send(true).ok();

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
