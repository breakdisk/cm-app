use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use logisticos_types::TenantId;

use crate::domain::{
    entities::{Vehicle, VehicleId, VehicleStatus, VehicleType},
    repositories::VehicleRepository,
};

#[derive(sqlx::FromRow)]
struct VehicleRow {
    id:                   Uuid,
    tenant_id:            Uuid,
    plate_number:         String,
    vehicle_type:         String,
    make:                 String,
    model:                String,
    year:                 i16,
    color:                String,
    status:               String,
    assigned_driver_id:   Option<Uuid>,
    odometer_km:          i32,
    maintenance_history:  serde_json::Value,
    next_maintenance_due: Option<chrono::NaiveDate>,
    created_at:           chrono::DateTime<chrono::Utc>,
    updated_at:           chrono::DateTime<chrono::Utc>,
}

impl TryFrom<VehicleRow> for Vehicle {
    type Error = anyhow::Error;

    fn try_from(r: VehicleRow) -> Result<Self, Self::Error> {
        let vehicle_type: VehicleType = serde_json::from_value(serde_json::Value::String(r.vehicle_type))?;
        let status: VehicleStatus     = serde_json::from_value(serde_json::Value::String(r.status))?;
        let maintenance_history       = serde_json::from_value(r.maintenance_history)?;

        Ok(Vehicle {
            id:                   VehicleId::from_uuid(r.id),
            tenant_id:            TenantId::from_uuid(r.tenant_id),
            plate_number:         r.plate_number,
            vehicle_type,
            make:                 r.make,
            model:                r.model,
            year:                 r.year as u16,
            color:                r.color,
            status,
            assigned_driver_id:   r.assigned_driver_id,
            odometer_km:          r.odometer_km,
            maintenance_history,
            next_maintenance_due: r.next_maintenance_due,
            created_at:           r.created_at,
            updated_at:           r.updated_at,
        })
    }
}

pub struct PgVehicleRepository {
    pool: PgPool,
}

impl PgVehicleRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl VehicleRepository for PgVehicleRepository {
    async fn find_by_id(&self, id: &VehicleId) -> anyhow::Result<Option<Vehicle>> {
        let row = sqlx::query_as::<_, VehicleRow>(
            r#"
            SELECT id, tenant_id, plate_number, vehicle_type, make, model, year, color,
                   status, assigned_driver_id, odometer_km, maintenance_history,
                   next_maintenance_due, created_at, updated_at
            FROM fleet.vehicles WHERE id = $1
            "#
        )
        .bind(id.inner())
        .fetch_optional(&self.pool)
        .await?;
        row.map(Vehicle::try_from).transpose()
    }

    async fn find_by_driver(&self, tenant_id: &TenantId, driver_id: Uuid) -> anyhow::Result<Option<Vehicle>> {
        let row = sqlx::query_as::<_, VehicleRow>(
            r#"
            SELECT id, tenant_id, plate_number, vehicle_type, make, model, year, color,
                   status, assigned_driver_id, odometer_km, maintenance_history,
                   next_maintenance_due, created_at, updated_at
            FROM fleet.vehicles
            WHERE tenant_id = $1 AND assigned_driver_id = $2
            "#
        )
        .bind(tenant_id.inner())
        .bind(driver_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(Vehicle::try_from).transpose()
    }

    async fn list(&self, tenant_id: &TenantId, limit: i64, offset: i64) -> anyhow::Result<Vec<Vehicle>> {
        let rows = sqlx::query_as::<_, VehicleRow>(
            r#"
            SELECT id, tenant_id, plate_number, vehicle_type, make, model, year, color,
                   status, assigned_driver_id, odometer_km, maintenance_history,
                   next_maintenance_due, created_at, updated_at
            FROM fleet.vehicles
            WHERE tenant_id = $1 AND status != 'decommissioned'
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#
        )
        .bind(tenant_id.inner())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(Vehicle::try_from).collect()
    }

    async fn list_maintenance_due(&self, tenant_id: &TenantId, within_days: i64) -> anyhow::Result<Vec<Vehicle>> {
        let rows = sqlx::query_as::<_, VehicleRow>(
            r#"
            SELECT id, tenant_id, plate_number, vehicle_type, make, model, year, color,
                   status, assigned_driver_id, odometer_km, maintenance_history,
                   next_maintenance_due, created_at, updated_at
            FROM fleet.vehicles
            WHERE tenant_id = $1
              AND next_maintenance_due IS NOT NULL
              AND next_maintenance_due <= CURRENT_DATE + ($2 || ' days')::INTERVAL
            ORDER BY next_maintenance_due ASC
            "#
        )
        .bind(tenant_id.inner())
        .bind(within_days.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(Vehicle::try_from).collect()
    }

    async fn save(&self, v: &Vehicle) -> anyhow::Result<()> {
        let vehicle_type = serde_json::to_value(&v.vehicle_type)?
            .as_str().unwrap_or("motorcycle").to_owned();
        let status = serde_json::to_value(&v.status)?
            .as_str().unwrap_or("active").to_owned();
        let maintenance_json = serde_json::to_value(&v.maintenance_history)?;

        sqlx::query(
            r#"
            INSERT INTO fleet.vehicles (
                id, tenant_id, plate_number, vehicle_type, make, model, year, color,
                status, assigned_driver_id, odometer_km, maintenance_history,
                next_maintenance_due, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
            ON CONFLICT (id) DO UPDATE SET
                plate_number         = EXCLUDED.plate_number,
                vehicle_type         = EXCLUDED.vehicle_type,
                make                 = EXCLUDED.make,
                model                = EXCLUDED.model,
                year                 = EXCLUDED.year,
                color                = EXCLUDED.color,
                status               = EXCLUDED.status,
                assigned_driver_id   = EXCLUDED.assigned_driver_id,
                odometer_km          = EXCLUDED.odometer_km,
                maintenance_history  = EXCLUDED.maintenance_history,
                next_maintenance_due = EXCLUDED.next_maintenance_due,
                updated_at           = EXCLUDED.updated_at
            "#
        )
        .bind(v.id.inner())
        .bind(v.tenant_id.inner())
        .bind(&v.plate_number)
        .bind(vehicle_type)
        .bind(&v.make)
        .bind(&v.model)
        .bind(v.year as i16)
        .bind(&v.color)
        .bind(status)
        .bind(v.assigned_driver_id)
        .bind(v.odometer_km)
        .bind(maintenance_json)
        .bind(v.next_maintenance_due)
        .bind(v.created_at)
        .bind(v.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn count(&self, tenant_id: &TenantId) -> anyhow::Result<i64> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM fleet.vehicles WHERE tenant_id = $1 AND status != 'decommissioned'"
        )
        .bind(tenant_id.inner())
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }
}
