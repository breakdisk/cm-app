use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;
use crate::domain::repositories::{TelemetryRepository, TelemetryEntry};

pub struct PgTelemetryRepository {
    pool: PgPool,
}

impl PgTelemetryRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[derive(sqlx::FromRow)]
struct TelemetryRow {
    tenant_id:        Uuid,
    shipment_id:      Uuid,
    task_id:          Option<Uuid>,
    driver_id:        Option<Uuid>,
    event_type:       String,
    device_timestamp: chrono::DateTime<chrono::Utc>,
    server_timestamp: chrono::DateTime<chrono::Utc>,
    lat:              Option<f64>,
    lng:              Option<f64>,
    metadata:         serde_json::Value,
}

impl From<TelemetryRow> for TelemetryEntry {
    fn from(r: TelemetryRow) -> Self {
        TelemetryEntry {
            tenant_id:        r.tenant_id,
            shipment_id:      r.shipment_id,
            task_id:          r.task_id,
            driver_id:        r.driver_id,
            event_type:       r.event_type,
            device_timestamp: r.device_timestamp,
            server_timestamp: r.server_timestamp,
            lat:              r.lat,
            lng:              r.lng,
            metadata:         r.metadata,
        }
    }
}

#[async_trait]
impl TelemetryRepository for PgTelemetryRepository {
    async fn append(&self, entry: TelemetryEntry) -> anyhow::Result<()> {
        // event_time = device_timestamp (TimescaleDB partition key)
        sqlx::query(
            r#"INSERT INTO pod.shipment_telemetry_logs
                   (tenant_id, shipment_id, task_id, driver_id,
                    event_type, event_time, device_timestamp, server_timestamp,
                    lat, lng, metadata)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)"#
        )
        .bind(entry.tenant_id)
        .bind(entry.shipment_id)
        .bind(entry.task_id)
        .bind(entry.driver_id)
        .bind(&entry.event_type)
        .bind(entry.device_timestamp)   // event_time
        .bind(entry.device_timestamp)   // device_timestamp (same value — canonical)
        .bind(entry.server_timestamp)
        .bind(entry.lat)
        .bind(entry.lng)
        .bind(entry.metadata)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_for_shipment(
        &self,
        shipment_id: Uuid,
        limit:       u32,
    ) -> anyhow::Result<Vec<TelemetryEntry>> {
        let rows = sqlx::query_as::<_, TelemetryRow>(
            r#"SELECT tenant_id, shipment_id, task_id, driver_id,
                      event_type, device_timestamp, server_timestamp,
                      lat, lng, metadata
               FROM pod.shipment_telemetry_logs
               WHERE shipment_id = $1
               ORDER BY device_timestamp DESC
               LIMIT $2"#
        )
        .bind(shipment_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(TelemetryEntry::from).collect())
    }
}
