use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::entities::CourierLocation;

#[async_trait]
pub trait LocationRepository: Send + Sync {
    async fn record(&self, l: &CourierLocation) -> anyhow::Result<()>;
    async fn latest(&self, tenant_id: Uuid, courier_id: Uuid) -> anyhow::Result<Option<CourierLocation>>;
}

pub struct PgLocationRepository {
    pool: PgPool,
}

impl PgLocationRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

fn map_row(r: &sqlx::postgres::PgRow) -> CourierLocation {
    CourierLocation {
        id:               r.get("id"),
        tenant_id:        r.get("tenant_id"),
        courier_id:       r.get("courier_id"),
        lat:              r.get("lat"),
        lng:              r.get("lng"),
        accuracy_m:       r.get("accuracy_m"),
        speed_kph:        r.get("speed_kph"),
        heading_deg:      r.get("heading_deg"),
        device_timestamp: r.get("device_timestamp"),
        recorded_at:      r.get("recorded_at"),
    }
}

#[async_trait]
impl LocationRepository for PgLocationRepository {
    async fn record(&self, l: &CourierLocation) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO field_ops.courier_locations (
                id, tenant_id, courier_id, lat, lng, accuracy_m, speed_kph,
                heading_deg, device_timestamp, recorded_at
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
            "#,
        )
        .bind(l.id).bind(l.tenant_id).bind(l.courier_id)
        .bind(l.lat).bind(l.lng)
        .bind(l.accuracy_m).bind(l.speed_kph).bind(l.heading_deg)
        .bind(l.device_timestamp).bind(l.recorded_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn latest(&self, tenant_id: Uuid, courier_id: Uuid) -> anyhow::Result<Option<CourierLocation>> {
        // Reads the raw table rather than courier_latest_locations: the view
        // has no tenant predicate on its DISTINCT ON, so filtering it by tenant
        // would still scan every courier's history. The (courier_id,
        // recorded_at DESC) index makes this exact.
        let row = sqlx::query(
            r#"
            SELECT * FROM field_ops.courier_locations
             WHERE tenant_id = $1 AND courier_id = $2
             ORDER BY recorded_at DESC
             LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(courier_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.as_ref().map(map_row))
    }
}
