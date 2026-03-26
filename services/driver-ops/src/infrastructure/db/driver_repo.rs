use async_trait::async_trait;
use sqlx::PgPool;
use logisticos_types::{Coordinates, DriverId, TenantId};
use uuid::Uuid;
use crate::domain::{
    entities::{Driver, DriverStatus},
    repositories::DriverRepository,
};

pub struct PgDriverRepository {
    pool: PgPool,
}

impl PgDriverRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[derive(sqlx::FromRow)]
struct DriverRow {
    id:              Uuid,
    tenant_id:       Uuid,
    user_id:         Uuid,
    first_name:      String,
    last_name:       String,
    phone:           String,
    status:          String,
    lat:             Option<f64>,
    lng:             Option<f64>,
    last_location_at: Option<chrono::DateTime<chrono::Utc>>,
    vehicle_id:      Option<Uuid>,
    active_route_id: Option<Uuid>,
    is_active:       bool,
    created_at:      chrono::DateTime<chrono::Utc>,
    updated_at:      chrono::DateTime<chrono::Utc>,
}

fn parse_status(s: &str) -> DriverStatus {
    match s {
        "available"  => DriverStatus::Available,
        "en_route"   => DriverStatus::EnRoute,
        "delivering" => DriverStatus::Delivering,
        "returning"  => DriverStatus::Returning,
        "on_break"   => DriverStatus::OnBreak,
        _            => DriverStatus::Offline,
    }
}

fn status_str(s: DriverStatus) -> &'static str {
    match s {
        DriverStatus::Offline    => "offline",
        DriverStatus::Available  => "available",
        DriverStatus::EnRoute    => "en_route",
        DriverStatus::Delivering => "delivering",
        DriverStatus::Returning  => "returning",
        DriverStatus::OnBreak    => "on_break",
    }
}

impl From<DriverRow> for Driver {
    fn from(r: DriverRow) -> Self {
        Driver {
            id: DriverId::from_uuid(r.id),
            tenant_id: TenantId::from_uuid(r.tenant_id),
            user_id: r.user_id,
            first_name: r.first_name,
            last_name: r.last_name,
            phone: r.phone,
            status: parse_status(&r.status),
            current_location: match (r.lat, r.lng) {
                (Some(lat), Some(lng)) => Some(Coordinates { lat, lng }),
                _ => None,
            },
            last_location_at: r.last_location_at,
            vehicle_id: r.vehicle_id,
            active_route_id: r.active_route_id,
            is_active: r.is_active,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[async_trait]
impl DriverRepository for PgDriverRepository {
    async fn find_by_id(&self, id: &DriverId) -> anyhow::Result<Option<Driver>> {
        let row = sqlx::query_as!(
            DriverRow,
            r#"SELECT id, tenant_id, user_id, first_name, last_name, phone, status,
                      lat, lng, last_location_at, vehicle_id, active_route_id,
                      is_active, created_at, updated_at
               FROM driver_ops.drivers WHERE id = $1"#,
            id.inner()
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Driver::from))
    }

    async fn find_by_user_id(&self, user_id: Uuid) -> anyhow::Result<Option<Driver>> {
        let row = sqlx::query_as!(
            DriverRow,
            r#"SELECT id, tenant_id, user_id, first_name, last_name, phone, status,
                      lat, lng, last_location_at, vehicle_id, active_route_id,
                      is_active, created_at, updated_at
               FROM driver_ops.drivers WHERE user_id = $1"#,
            user_id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Driver::from))
    }

    async fn list_by_tenant(&self, tenant_id: &TenantId) -> anyhow::Result<Vec<Driver>> {
        let rows = sqlx::query_as!(
            DriverRow,
            r#"SELECT id, tenant_id, user_id, first_name, last_name, phone, status,
                      lat, lng, last_location_at, vehicle_id, active_route_id,
                      is_active, created_at, updated_at
               FROM driver_ops.drivers
               WHERE tenant_id = $1
               ORDER BY first_name, last_name"#,
            tenant_id.inner()
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Driver::from).collect())
    }

    async fn save(&self, d: &Driver) -> anyhow::Result<()> {
        let status = status_str(d.status);
        sqlx::query!(
            r#"INSERT INTO driver_ops.drivers
                   (id, tenant_id, user_id, first_name, last_name, phone, status,
                    lat, lng, last_location_at, vehicle_id, active_route_id,
                    is_active, created_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
               ON CONFLICT (id) DO UPDATE SET
                   status           = EXCLUDED.status,
                   lat              = EXCLUDED.lat,
                   lng              = EXCLUDED.lng,
                   last_location_at = EXCLUDED.last_location_at,
                   vehicle_id       = EXCLUDED.vehicle_id,
                   active_route_id  = EXCLUDED.active_route_id,
                   is_active        = EXCLUDED.is_active,
                   updated_at       = EXCLUDED.updated_at"#,
            d.id.inner(),
            d.tenant_id.inner(),
            d.user_id,
            d.first_name,
            d.last_name,
            d.phone,
            status,
            d.current_location.map(|c| c.lat),
            d.current_location.map(|c| c.lng),
            d.last_location_at,
            d.vehicle_id,
            d.active_route_id,
            d.is_active,
            d.created_at,
            d.updated_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
