use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;
use crate::domain::{
    entities::{ProofOfPickup, PopStatus},
    repositories::PickupRepository,
};

pub struct PgPickupRepository {
    pool: PgPool,
}

impl PgPickupRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[derive(sqlx::FromRow)]
struct PopRow {
    id:                     Uuid,
    tenant_id:              Uuid,
    shipment_id:            Uuid,
    task_id:                Uuid,
    driver_id:              Uuid,
    status:                 String,
    capture_lat:            f64,
    capture_lng:            f64,
    geofence_verified:      bool,
    out_of_bounds_handover: bool,
    photo_s3_key:           Option<String>,
    photo_size_bytes:       Option<i64>,
    actual_weight_g:        Option<i64>,
    declared_weight_g:      Option<i64>,
    barcode_scanned:        bool,
    scanned_barcode:        Option<String>,
    service_code:           String,
    declared_value_cents:   Option<i64>,
    device_timestamp:       Option<chrono::DateTime<chrono::Utc>>,
    captured_at:            chrono::DateTime<chrono::Utc>,
    created_at:             chrono::DateTime<chrono::Utc>,
}

impl From<PopRow> for ProofOfPickup {
    fn from(r: PopRow) -> Self {
        ProofOfPickup {
            id:                     r.id,
            tenant_id:              r.tenant_id,
            shipment_id:            r.shipment_id,
            task_id:                r.task_id,
            driver_id:              r.driver_id,
            status:                 if r.status == "submitted" { PopStatus::Submitted } else { PopStatus::Draft },
            capture_lat:            r.capture_lat,
            capture_lng:            r.capture_lng,
            geofence_verified:      r.geofence_verified,
            out_of_bounds_handover: r.out_of_bounds_handover,
            photo_s3_key:           r.photo_s3_key,
            photo_size_bytes:       r.photo_size_bytes.map(|v| v as u64),
            actual_weight_g:        r.actual_weight_g,
            declared_weight_g:      r.declared_weight_g,
            barcode_scanned:        r.barcode_scanned,
            scanned_barcode:        r.scanned_barcode,
            service_code:           r.service_code,
            declared_value_cents:   r.declared_value_cents,
            device_timestamp:       r.device_timestamp,
            captured_at:            r.captured_at,
            created_at:             r.created_at,
        }
    }
}

const SELECT_COLS: &str = r#"
    id, tenant_id, shipment_id, task_id, driver_id, status,
    capture_lat, capture_lng, geofence_verified, out_of_bounds_handover,
    photo_s3_key, photo_size_bytes,
    actual_weight_g, declared_weight_g,
    barcode_scanned, scanned_barcode,
    COALESCE(service_code, 'standard') AS service_code,
    declared_value_cents,
    device_timestamp, captured_at, created_at
"#;

#[async_trait]
impl PickupRepository for PgPickupRepository {
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<ProofOfPickup>> {
        let sql = format!("SELECT {SELECT_COLS} FROM pod.proofs_of_pickup WHERE id = $1");
        let row = sqlx::query_as::<_, PopRow>(&sql)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(ProofOfPickup::from))
    }

    async fn find_by_shipment(&self, shipment_id: Uuid) -> anyhow::Result<Option<ProofOfPickup>> {
        let sql = format!(
            "SELECT {SELECT_COLS} FROM pod.proofs_of_pickup \
             WHERE shipment_id = $1 ORDER BY created_at DESC LIMIT 1"
        );
        let row = sqlx::query_as::<_, PopRow>(&sql)
            .bind(shipment_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(ProofOfPickup::from))
    }

    async fn find_completed_by_shipment(&self, shipment_id: Uuid) -> anyhow::Result<Option<ProofOfPickup>> {
        // The unique partial index `uq_pop_shipment_submitted` on (shipment_id)
        // WHERE status = 'submitted' guarantees at most one row matches.
        let sql = format!(
            "SELECT {SELECT_COLS} FROM pod.proofs_of_pickup \
             WHERE shipment_id = $1 AND status = 'submitted' LIMIT 1"
        );
        let row = sqlx::query_as::<_, PopRow>(&sql)
            .bind(shipment_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(ProofOfPickup::from))
    }

    async fn save(&self, pop: &ProofOfPickup) -> anyhow::Result<()> {
        let status = if pop.status == PopStatus::Submitted { "submitted" } else { "draft" };
        sqlx::query(
            r#"INSERT INTO pod.proofs_of_pickup
                   (id, tenant_id, shipment_id, task_id, driver_id, status,
                    capture_lat, capture_lng, geofence_verified, out_of_bounds_handover,
                    photo_s3_key, photo_size_bytes,
                    actual_weight_g, declared_weight_g,
                    barcode_scanned, scanned_barcode,
                    service_code, declared_value_cents,
                    device_timestamp, captured_at, created_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21)
               ON CONFLICT (id) DO UPDATE SET
                   status                 = EXCLUDED.status,
                   photo_s3_key           = EXCLUDED.photo_s3_key,
                   photo_size_bytes       = EXCLUDED.photo_size_bytes,
                   actual_weight_g        = EXCLUDED.actual_weight_g,
                   declared_weight_g      = EXCLUDED.declared_weight_g,
                   barcode_scanned        = EXCLUDED.barcode_scanned,
                   scanned_barcode        = EXCLUDED.scanned_barcode,
                   out_of_bounds_handover = EXCLUDED.out_of_bounds_handover,
                   service_code           = EXCLUDED.service_code,
                   declared_value_cents   = EXCLUDED.declared_value_cents,
                   device_timestamp       = EXCLUDED.device_timestamp"#
        )
        .bind(pop.id)
        .bind(pop.tenant_id)
        .bind(pop.shipment_id)
        .bind(pop.task_id)
        .bind(pop.driver_id)
        .bind(status)
        .bind(pop.capture_lat)
        .bind(pop.capture_lng)
        .bind(pop.geofence_verified)
        .bind(pop.out_of_bounds_handover)
        .bind(&pop.photo_s3_key)
        .bind(pop.photo_size_bytes.map(|v| v as i64))
        .bind(pop.actual_weight_g)
        .bind(pop.declared_weight_g)
        .bind(pop.barcode_scanned)
        .bind(&pop.scanned_barcode)
        .bind(&pop.service_code)
        .bind(pop.declared_value_cents)
        .bind(pop.device_timestamp)
        .bind(pop.captured_at)
        .bind(pop.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
