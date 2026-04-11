use std::{net::SocketAddr, sync::Arc};
use sqlx::postgres::PgPoolOptions;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use uuid::Uuid;

use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::rbac::permissions;
use logisticos_errors::AppError;

use crate::{
    application::services::{
        CreateHubCommand, HubRepository, HubService, InductParcelCommand,
        InductionRepository, SortParcelCommand,
    },
    config::Config,
};

// ---------------------------------------------------------------------------
// PostgreSQL repositories (inline for hub-ops — small surface area)
// ---------------------------------------------------------------------------

struct PgHubRepository { pool: sqlx::PgPool }
struct PgInductionRepository { pool: sqlx::PgPool }

#[derive(sqlx::FromRow)]
struct HubRow {
    id: Uuid, tenant_id: Uuid, name: String, address: String,
    lat: f64, lng: f64, capacity: i32, current_load: i32,
    serving_zones: Vec<String>, is_active: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow)]
struct InductionRow {
    id: Uuid, hub_id: Uuid, tenant_id: Uuid, shipment_id: Uuid,
    tracking_number: String, status: String,
    zone: Option<String>, bay: Option<String>, inducted_by: Option<Uuid>,
    inducted_at: chrono::DateTime<chrono::Utc>,
    sorted_at: Option<chrono::DateTime<chrono::Utc>>,
    dispatched_at: Option<chrono::DateTime<chrono::Utc>>,
}

fn row_to_hub(r: HubRow) -> crate::domain::entities::Hub {
    use crate::domain::entities::{Hub, HubId};
    use logisticos_types::TenantId;
    Hub {
        id: HubId::from_uuid(r.id), tenant_id: TenantId::from_uuid(r.tenant_id),
        name: r.name, address: r.address, lat: r.lat, lng: r.lng,
        capacity: r.capacity as u32, current_load: r.current_load as u32,
        serving_zones: r.serving_zones, is_active: r.is_active,
        created_at: r.created_at, updated_at: r.updated_at,
    }
}

fn row_to_induction(r: InductionRow) -> crate::domain::entities::ParcelInduction {
    use crate::domain::entities::{HubId, InductionId, InductionStatus, ParcelInduction};
    use logisticos_types::TenantId;
    let status = match r.status.as_str() {
        "sorted"     => InductionStatus::Sorted,
        "dispatched" => InductionStatus::Dispatched,
        "returned"   => InductionStatus::Returned,
        _            => InductionStatus::Inducted,
    };
    ParcelInduction {
        id: InductionId::from_uuid(r.id),
        hub_id: HubId::from_uuid(r.hub_id),
        tenant_id: TenantId::from_uuid(r.tenant_id),
        shipment_id: r.shipment_id,
        tracking_number: r.tracking_number,
        status,
        zone: r.zone,
        bay: r.bay,
        inducted_by: r.inducted_by,
        inducted_at: r.inducted_at,
        sorted_at: r.sorted_at,
        dispatched_at: r.dispatched_at,
    }
}

#[async_trait::async_trait]
impl HubRepository for PgHubRepository {
    async fn find_by_id(&self, id: &crate::domain::entities::HubId) -> anyhow::Result<Option<crate::domain::entities::Hub>> {
        let row = sqlx::query_as::<_, HubRow>(
            r#"SELECT id, tenant_id, name, address, lat, lng, capacity, current_load,
                      serving_zones, is_active, created_at, updated_at
               FROM hub_ops.hubs WHERE id = $1"#
        ).bind(id.inner()).fetch_optional(&self.pool).await?;
        Ok(row.map(row_to_hub))
    }
    async fn list(&self, tenant_id: &logisticos_types::TenantId) -> anyhow::Result<Vec<crate::domain::entities::Hub>> {
        let rows = sqlx::query_as::<_, HubRow>(
            r#"SELECT id, tenant_id, name, address, lat, lng, capacity, current_load,
                      serving_zones, is_active, created_at, updated_at
               FROM hub_ops.hubs WHERE tenant_id = $1 AND is_active = true
               ORDER BY name"#
        ).bind(tenant_id.inner()).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(row_to_hub).collect())
    }
    async fn save(&self, hub: &crate::domain::entities::Hub) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO hub_ops.hubs (
                id, tenant_id, name, address, lat, lng, capacity, current_load,
                serving_zones, is_active, created_at, updated_at
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
            ON CONFLICT (id) DO UPDATE SET
                current_load = EXCLUDED.current_load,
                serving_zones = EXCLUDED.serving_zones,
                is_active = EXCLUDED.is_active,
                updated_at = EXCLUDED.updated_at
            "#
        )
        .bind(hub.id.inner()).bind(hub.tenant_id.inner()).bind(&hub.name).bind(&hub.address)
        .bind(hub.lat).bind(hub.lng).bind(hub.capacity as i32).bind(hub.current_load as i32)
        .bind(&hub.serving_zones).bind(hub.is_active).bind(hub.created_at).bind(hub.updated_at)
        .execute(&self.pool).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl InductionRepository for PgInductionRepository {
    async fn find_by_id(&self, id: &crate::domain::entities::InductionId) -> anyhow::Result<Option<crate::domain::entities::ParcelInduction>> {
        let row = sqlx::query_as::<_, InductionRow>(
            r#"SELECT id, hub_id, tenant_id, shipment_id, tracking_number, status,
                      zone, bay, inducted_by, inducted_at, sorted_at, dispatched_at
               FROM hub_ops.parcel_inductions WHERE id = $1"#
        ).bind(id.inner()).fetch_optional(&self.pool).await?;
        Ok(row.map(row_to_induction))
    }
    async fn find_by_shipment(&self, shipment_id: Uuid) -> anyhow::Result<Option<crate::domain::entities::ParcelInduction>> {
        let row = sqlx::query_as::<_, InductionRow>(
            r#"SELECT id, hub_id, tenant_id, shipment_id, tracking_number, status,
                      zone, bay, inducted_by, inducted_at, sorted_at, dispatched_at
               FROM hub_ops.parcel_inductions WHERE shipment_id = $1
               LIMIT 1"#
        ).bind(shipment_id).fetch_optional(&self.pool).await?;
        Ok(row.map(row_to_induction))
    }
    async fn list_active(&self, hub_id: &crate::domain::entities::HubId) -> anyhow::Result<Vec<crate::domain::entities::ParcelInduction>> {
        let rows = sqlx::query_as::<_, InductionRow>(
            r#"SELECT id, hub_id, tenant_id, shipment_id, tracking_number, status,
                      zone, bay, inducted_by, inducted_at, sorted_at, dispatched_at
               FROM hub_ops.parcel_inductions
               WHERE hub_id = $1 AND status IN ('inducted', 'sorted')
               ORDER BY inducted_at DESC"#
        ).bind(hub_id.inner()).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(row_to_induction).collect())
    }
    async fn save(&self, i: &crate::domain::entities::ParcelInduction) -> anyhow::Result<()> {
        let status = serde_json::to_value(&i.status)?.as_str().unwrap_or("inducted").to_owned();
        sqlx::query(
            r#"
            INSERT INTO hub_ops.parcel_inductions (
                id, hub_id, tenant_id, shipment_id, tracking_number, status,
                zone, bay, inducted_by, inducted_at, sorted_at, dispatched_at
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
            ON CONFLICT (id) DO UPDATE SET
                status = EXCLUDED.status, zone = EXCLUDED.zone, bay = EXCLUDED.bay,
                sorted_at = EXCLUDED.sorted_at, dispatched_at = EXCLUDED.dispatched_at
            "#
        )
        .bind(i.id.inner()).bind(i.hub_id.inner()).bind(i.tenant_id.inner()).bind(i.shipment_id)
        .bind(&i.tracking_number).bind(status).bind(&i.zone).bind(&i.bay).bind(i.inducted_by)
        .bind(i.inducted_at).bind(i.sorted_at).bind(i.dispatched_at)
        .execute(&self.pool).await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct AppState { svc: Arc<HubService> }

// ---------------------------------------------------------------------------
// HTTP handlers
// ---------------------------------------------------------------------------

async fn create_hub(State(s): State<AppState>, claims: AuthClaims, Json(cmd): Json<CreateHubCommand>) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::FLEET_MANAGE)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let hub = s.svc.create_hub(&tenant_id, cmd).await?;
    Ok::<_, AppError>((StatusCode::CREATED, Json(hub)))
}

async fn list_hubs(State(s): State<AppState>, claims: AuthClaims) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::FLEET_READ)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let hubs = s.svc.list_hubs(&tenant_id).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"hubs": hubs}))))
}

async fn induct(State(s): State<AppState>, claims: AuthClaims, Json(cmd): Json<InductParcelCommand>) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_UPDATE)?;
    let induction = s.svc.induct_parcel(cmd).await?;
    Ok::<_, AppError>((StatusCode::CREATED, Json(induction)))
}

async fn sort(State(s): State<AppState>, claims: AuthClaims, Json(cmd): Json<SortParcelCommand>) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_UPDATE)?;
    let induction = s.svc.sort_parcel(cmd).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(induction)))
}

async fn dispatch(State(s): State<AppState>, claims: AuthClaims, Path(id): Path<Uuid>) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_UPDATE)?;
    let induction = s.svc.dispatch_parcel(id).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(induction)))
}

async fn manifest(State(s): State<AppState>, claims: AuthClaims, Path(hub_id): Path<Uuid>) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_READ)?;
    let parcels = s.svc.hub_manifest(hub_id).await?;
    let count = parcels.len();
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"parcels": parcels, "count": count}))))
}

// ---------------------------------------------------------------------------
// Bootstrap
// ---------------------------------------------------------------------------

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load()?;
    let otlp = std::env::var("OTLP_ENDPOINT").ok();
    logisticos_tracing::init(logisticos_tracing::TracingConfig {
        service_name: "hub-ops",
        env: &cfg.app.env,
        otlp_endpoint: otlp.as_deref(),
        log_level: None,
    })?;

    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .after_connect(|conn, _meta| Box::pin(async move {
            sqlx::query("SET search_path TO hub_ops, public")
                .execute(&mut *conn)
                .await?;
            Ok(())
        }))
        .connect(&cfg.database.url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    let hub_repo       = Arc::new(PgHubRepository { pool: pool.clone() });
    let induction_repo = Arc::new(PgInductionRepository { pool });
    let svc = Arc::new(HubService::new(hub_repo, induction_repo));
    let state = AppState { svc };

    let app = Router::new()
        .route("/v1/hubs",                          get(list_hubs).post(create_hub))
        .route("/v1/hubs/:id/manifest",             get(manifest))
        .route("/v1/hubs/induct",                   post(induct))
        .route("/v1/hubs/sort",                     post(sort))
        .route("/v1/hubs/dispatch/:id",             post(dispatch))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.app.host, cfg.app.port).parse()?;
    tracing::info!(addr = %addr, "hub-ops service listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).with_graceful_shutdown(shutdown_signal()).await?;
    Ok(())
}

async fn shutdown_signal() {
    use tokio::signal;
    let ctrl_c = async { signal::ctrl_c().await.expect("ctrl-c") };
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate()).expect("SIGTERM").recv().await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
}
