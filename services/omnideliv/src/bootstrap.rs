use std::sync::Arc;

use anyhow::Context;
use logisticos_auth::jwt::JwtService;
use sqlx::postgres::PgPoolOptions;

use crate::api::http::{router, AppState};
use crate::application::services::{BasketService, CatalogService};
use crate::config::Config;
use crate::infrastructure::db::{PgBasketRepository, PgCatalogRepository, PgVendorRepository};

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
    let baskets = Arc::new(BasketService::new(Arc::new(PgBasketRepository::new(pool.clone()))));

    let state = Arc::new(AppState { catalog, baskets, jwt });

    let addr = format!("0.0.0.0:{}", cfg.app.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(%addr, "omnideliv listening");
    axum::serve(listener, router(state)).await?;

    Ok(())
}
