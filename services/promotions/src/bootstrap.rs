use std::sync::Arc;

use anyhow::Context;
use logisticos_auth::jwt::JwtService;
use sqlx::postgres::PgPoolOptions;

use crate::api::http::{router, AppState};
use crate::application::{Promotions, Rules};
use crate::config::Config;
use crate::domain::window::WindowRule;
use crate::infrastructure::{db::PgPromotionsStore, events_consumer};

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load().context("Failed to load promotions config")?;

    let otlp = std::env::var("OTLP_ENDPOINT").ok();
    logisticos_tracing::init(logisticos_tracing::TracingConfig {
        service_name: "promotions",
        env: &cfg.app.env,
        otlp_endpoint: otlp.as_deref(),
        log_level: None,
    })?;

    let p = &cfg.promotions;
    tracing::info!(
        env = %cfg.app.env,
        window_from_day = p.window_from_day,
        window_to_day = p.window_to_day,
        ceiling_pct = p.ceiling_pct,
        ceiling_flat_units = p.ceiling_flat_units,
        utc_offset_minutes = p.utc_offset_minutes,
        referral_reward_cents = p.referral_reward_cents,
        credit_currency = %p.credit_currency,
        "promotions service starting",
    );

    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                sqlx::query("SET search_path TO promotions, public").execute(&mut *conn).await?;
                Ok(())
            })
        })
        .connect(&cfg.database.url)
        .await
        .context("Failed to connect to PostgreSQL")?;

    logisticos_common::migrations::run(&pool, "promotions", &sqlx::migrate!("./migrations"))
        .await
        .context("promotions migration failed")?;

    let jwt_secret = std::env::var("AUTH__JWT_SECRET").context("AUTH__JWT_SECRET not set")?;
    let jwt = Arc::new(JwtService::new(&jwt_secret, 3600, 86400));

    let store = Arc::new(PgPromotionsStore::new(pool.clone()));
    let promotions = Arc::new(Promotions::new(
        store.clone(),
        store,
        Rules {
            window: WindowRule::new(p.window_from_day, p.window_to_day),
            ceiling_pct: p.ceiling_pct.clamp(0, 100),
            ceiling_flat_units: p.ceiling_flat_units.max(0),
            utc_offset_minutes: p.utc_offset_minutes.clamp(-12 * 60, 14 * 60),
            referral_reward_cents: p.referral_reward_cents.max(0),
            credit_currency: p.credit_currency.trim().to_ascii_uppercase(),
        },
    ));

    // Bookings, completions and cancellations: the loyalty count, referral
    // rewards, and codes and credit given back. Spawned, never awaited into
    // startup: a broker that will not connect must not stop quotes being priced.
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    {
        let brokers = cfg.kafka.brokers.clone();
        let promotions = Arc::clone(&promotions);
        tokio::spawn(async move {
            if let Err(e) = events_consumer::start(&brokers, promotions, shutdown_rx).await {
                tracing::error!(err = %e, "promotions events consumer stopped — loyalty and referrals stall, cancellations keep their codes");
            }
        });
    }

    let state = Arc::new(AppState { promotions, jwt });

    let addr = format!("0.0.0.0:{}", cfg.app.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(%addr, "promotions listening");
    axum::serve(listener, router(state)).await?;

    let _ = shutdown_tx.send(true);
    Ok(())
}
