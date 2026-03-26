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

fn row_to_induction(
    id: Uuid, hub_id: Uuid, tenant_id: Uuid, shipment_id: Uuid,
    tracking_number: String, status: String,
    zone: Option<String>, bay: Option<String>, inducted_by: Option<Uuid>,
    inducted_at: chrono::DateTime<chrono::Utc>,
    sorted_at: Option<chrono::DateTime<chrono::Utc>>,
    dispatched_at: Option<chrono::DateTime<chrono::Utc>>,
) -> crate::domain::entities::ParcelInduction {
    use crate::domain::entities::{HubId, InductionId, InductionStatus, ParcelInduction};
    use logisticos_types::TenantId;
    let status = match status.as_str() {
        "sorted"     => InductionStatus::Sorted,
        "dispatched" => InductionStatus::Dispatched,
        "returned"   => InductionStatus::Returned,
        _            => InductionStatus::Inducted,
    };
    ParcelInduction {
        id: InductionId::from_uuid(id),
        hub_id: HubId::from_uuid(hub_id),
        tenant_id: TenantId::from_uuid(tenant_id),
        shipment_id,
        tracking_number,
        status,
        zone,
        bay,
        inducted_by,
        inducted_at,
        sorted_at,
        dispatched_at,
    }
}

#[async_trait::async_trait]
impl HubRepository for PgHubRepository {
    async fn find_by_id(&self, id: &crate::domain::entities::HubId) -> anyhow::Result<Option<crate::domain::entities::Hub>> {
        use crate::domain::entities::{Hub, HubId};
        use logisticos_types::TenantId;
        struct HubRow {
            id: Uuid, tenant_id: Uuid, name: String, address: String,
            lat: f64, lng: f64, capacity: i32, current_load: i32,
            serving_zones: Vec<String>, is_active: bool,
            created_at: chrono::DateTime<chrono::Utc>,
            updated_at: chrono::DateTime<chrono::Utc>,
        }
        let row = sqlx::query_as!(HubRow,
            r#"SELECT id, tenant_id, name, address, lat, lng, capacity, current_load,
                      serving_zones, is_active, created_at, updated_at
               FROM hub_ops.hubs WHERE id = $1"#,
            id.inner()
        ).fetch_optional(&self.pool).await?;
        Ok(row.map(|r| Hub {
            id: HubId::from_uuid(r.id), tenant_id: TenantId::from_uuid(r.tenant_id),
            name: r.name, address: r.address, lat: r.lat, lng: r.lng,
            capacity: r.capacity as u32, current_load: r.current_load as u32,
            serving_zones: r.serving_zones, is_active: r.is_active,
            created_at: r.created_at, updated_at: r.updated_at,
        }))
    }
    async fn list(&self, tenant_id: &logisticos_types::TenantId) -> anyhow::Result<Vec<crate::domain::entities::Hub>> {
        use crate::domain::entities::{Hub, HubId};
        use logisticos_types::TenantId;
        struct HubRow {
            id: Uuid, tenant_id: Uuid, name: String, address: String,
            lat: f64, lng: f64, capacity: i32, current_load: i32,
            serving_zones: Vec<String>, is_active: bool,
            created_at: chrono::DateTime<chrono::Utc>,
            updated_at: chrono::DateTime<chrono::Utc>,
        }
        let rows = sqlx::query_as!(HubRow,
            r#"SELECT id, tenant_id, name, address, lat, lng, capacity, current_load,
                      serving_zones, is_active, created_at, updated_at
               FROM hub_ops.hubs WHERE tenant_id = $1 AND is_active = true
               ORDER BY name"#,
            tenant_id.inner()
        ).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(|r| Hub {
            id: HubId::from_uuid(r.id), tenant_id: TenantId::from_uuid(r.tenant_id),
            name: r.name, address: r.address, lat: r.lat, lng: r.lng,
            capacity: r.capacity as u32, current_load: r.current_load as u32,
            serving_zones: r.serving_zones, is_active: r.is_active,
            created_at: r.created_at, updated_at: r.updated_at,
        }).collect())
    }
    async fn save(&self, hub: &crate::domain::entities::Hub) -> anyhow::Result<()> {
        sqlx::query!(
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
            "#,
            hub.id.inner(), hub.tenant_id.inner(), hub.name, hub.address,
            hub.lat, hub.lng, hub.capacity as i32, hub.current_load as i32,
            &hub.serving_zones, hub.is_active, hub.created_at, hub.updated_at,
        ).execute(&self.pool).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl InductionRepository for PgInductionRepository {
    async fn find_by_id(&self, id: &crate::domain::entities::InductionId) -> anyhow::Result<Option<crate::domain::entities::ParcelInduction>> {
        let row = sqlx::query!(
            r#"SELECT id, hub_id, tenant_id, shipment_id, tracking_number, status,
                      zone, bay, inducted_by, inducted_at, sorted_at, dispatched_at
               FROM hub_ops.parcel_inductions WHERE id = $1"#,
            id.inner()
        ).fetch_optional(&self.pool).await?;
        Ok(row.map(|r| row_to_induction(r.id, r.hub_id, r.tenant_id, r.shipment_id,
            r.tracking_number, r.status, r.zone, r.bay, r.inducted_by, r.inducted_at,
            r.sorted_at, r.dispatched_at)))
    }
    async fn find_by_shipment(&self, shipment_id: Uuid) -> anyhow::Result<Option<crate::domain::entities::ParcelInduction>> {
        let row = sqlx::query!(
            r#"SELECT id, hub_id, tenant_id, shipment_id, tracking_number, status,
                      zone, bay, inducted_by, inducted_at, sorted_at, dispatched_at
               FROM hub_ops.parcel_inductions WHERE shipment_id = $1
               LIMIT 1"#,
            shipment_id
        ).fetch_optional(&self.pool).await?;
        Ok(row.map(|r| row_to_induction(r.id, r.hub_id, r.tenant_id, r.shipment_id,
            r.tracking_number, r.status, r.zone, r.bay, r.inducted_by, r.inducted_at,
            r.sorted_at, r.dispatched_at)))
    }
    async fn list_active(&self, hub_id: &crate::domain::entities::HubId) -> anyhow::Result<Vec<crate::domain::entities::ParcelInduction>> {
        let rows = sqlx::query!(
            r#"SELECT id, hub_id, tenant_id, shipment_id, tracking_number, status,
                      zone, bay, inducted_by, inducted_at, sorted_at, dispatched_at
               FROM hub_ops.parcel_inductions
               WHERE hub_id = $1 AND status IN ('inducted', 'sorted')
               ORDER BY inducted_at DESC"#,
            hub_id.inner()
        ).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(|r| row_to_induction(r.id, r.hub_id, r.tenant_id,
            r.shipment_id, r.tracking_number, r.status, r.zone, r.bay, r.inducted_by,
            r.inducted_at, r.sorted_at, r.dispatched_at)).collect())
    }
    async fn save(&self, i: &crate::domain::entities::ParcelInduction) -> anyhow::Result<()> {
        let status = serde_json::to_value(&i.status)?.as_str().unwrap_or("inducted").to_owned();
        sqlx::query!(
            r#"
            INSERT INTO hub_ops.parcel_inductions (
                id, hub_id, tenant_id, shipment_id, tracking_number, status,
                zone, bay, inducted_by, inducted_at, sorted_at, dispatched_at
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
            ON CONFLICT (id) DO UPDATE SET
                status = EXCLUDED.status, zone = EXCLUDED.zone, bay = EXCLUDED.bay,
                sorted_at = EXCLUDED.sorted_at, dispatched_at = EXCLUDED.dispatched_at
            "#,
            i.id.inner(), i.hub_id.inner(), i.tenant_id.inner(), i.shipment_id,
            i.tracking_number, status, i.zone, i.bay, i.inducted_by,
            i.inducted_at, i.sorted_at, i.dispatched_at,
        ).execute(&self.pool).await?;
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
    claims.require_permission(permissions::FLEET_MANAGE)?;
    let hub = s.svc.create_hub(&claims.tenant_id, cmd).await?;
    Ok::<_, AppError>((StatusCode::CREATED, Json(hub)))
}

async fn list_hubs(State(s): State<AppState>, claims: AuthClaims) -> impl IntoResponse {
    claims.require_permission(permissions::FLEET_READ)?;
    let hubs = s.svc.list_hubs(&claims.tenant_id).await?;
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
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"parcels": parcels, "count": parcels.len()}))))
}

// ---------------------------------------------------------------------------
// Bootstrap
// ---------------------------------------------------------------------------

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load()?;
    logisticos_tracing::init(&cfg.app.env, "hub-ops")?;

    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
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
