use async_trait::async_trait;
use sqlx::PgPool;
use logisticos_types::DriverId;
use crate::domain::{entities::DriverLocation, repositories::LocationRepository};

/// Writes to a TimescaleDB hypertable partitioned by `recorded_at`.
/// Reads use a LATERAL join to get the most recent row efficiently.
pub struct PgLocationRepository {
    pool: PgPool,
}

impl PgLocationRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[derive(sqlx::FromRow)]
struct LocationRow {
    driver_id:   uuid::Uuid,
    tenant_id:   uuid::Uuid,
    lat:         f64,
    lng:         f64,
    accuracy_m:  Option<f32>,
    speed_kmh:   Option<f32>,
    heading:     Option<f32>,
    battery_pct: Option<i16>,
    recorded_at: chrono::DateTime<chrono::Utc>,
    received_at: chrono::DateTime<chrono::Utc>,
}

impl From<LocationRow> for DriverLocation {
    fn from(r: LocationRow) -> Self {
        DriverLocation {
            driver_id: r.driver_id,
            tenant_id: r.tenant_id,
            lat: r.lat,
            lng: r.lng,
            accuracy_m: r.accuracy_m,
            speed_kmh: r.speed_kmh,
            heading: r.heading,
            battery_pct: r.battery_pct.map(|b| b as u8),
            recorded_at: r.recorded_at,
            received_at: r.received_at,
        }
    }
}

#[async_trait]
impl LocationRepository for PgLocationRepository {
    async fn record(&self, loc: &DriverLocation) -> anyhow::Result<()> {
        // INSERT only — TimescaleDB hypertable, never update location history.
        sqlx::query(
            r#"INSERT INTO driver_ops.driver_locations
                   (driver_id, tenant_id, lat, lng, accuracy_m, speed_kmh, heading,
                    battery_pct, recorded_at, received_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)"#
        )
        .bind(loc.driver_id)
        .bind(loc.tenant_id)
        .bind(loc.lat)
        .bind(loc.lng)
        .bind(loc.accuracy_m)
        .bind(loc.speed_kmh)
        .bind(loc.heading)
        .bind(loc.battery_pct.map(|b| b as i16))
        .bind(loc.recorded_at)
        .bind(loc.received_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn latest(&self, driver_id: &DriverId) -> anyhow::Result<Option<DriverLocation>> {
        let row = sqlx::query_as::<_, LocationRow>(
            r#"SELECT driver_id, tenant_id, lat, lng, accuracy_m, speed_kmh, heading,
                      battery_pct, recorded_at, received_at
               FROM driver_ops.driver_locations
               WHERE driver_id = $1
               ORDER BY recorded_at DESC
               LIMIT 1"#
        )
        .bind(driver_id.inner())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(DriverLocation::from))
    }

    async fn history(
        &self,
        driver_id: &DriverId,
        from: chrono::DateTime<chrono::Utc>,
        to: chrono::DateTime<chrono::Utc>,
    ) -> anyhow::Result<Vec<DriverLocation>> {
        let rows = sqlx::query_as::<_, LocationRow>(
            r#"SELECT driver_id, tenant_id, lat, lng, accuracy_m, speed_kmh, heading,
                      battery_pct, recorded_at, received_at
               FROM driver_ops.driver_locations
               WHERE driver_id = $1
                 AND recorded_at BETWEEN $2 AND $3
               ORDER BY recorded_at ASC"#
        )
        .bind(driver_id.inner())
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(DriverLocation::from).collect())
    }
}
