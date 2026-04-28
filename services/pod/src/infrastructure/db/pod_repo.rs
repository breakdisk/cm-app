use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;
use serde_json;
use crate::domain::{
    entities::{ProofOfDelivery, PodStatus, PodPhoto},
    repositories::PodRepository,
};

pub struct PgPodRepository {
    pool: PgPool,
}

impl PgPodRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[derive(sqlx::FromRow)]
struct PodRow {
    id:                  Uuid,
    tenant_id:           Uuid,
    shipment_id:         Uuid,
    task_id:             Uuid,
    driver_id:           Uuid,
    status:              String,
    signature_data:      Option<String>,
    recipient_name:      String,
    photos:              serde_json::Value,   // JSONB array of PodPhoto
    capture_lat:         f64,
    capture_lng:         f64,
    geofence_verified:   bool,
    otp_verified:        bool,
    otp_id:              Option<Uuid>,
    cod_collected_cents: Option<i64>,
    requires_photo:      bool,
    requires_signature:  bool,
    captured_at:         chrono::DateTime<chrono::Utc>,
    created_at:          chrono::DateTime<chrono::Utc>,
}

fn parse_status(s: &str) -> PodStatus {
    match s {
        "submitted" => PodStatus::Submitted,
        "verified"  => PodStatus::Verified,
        "disputed"  => PodStatus::Disputed,
        _           => PodStatus::Draft,
    }
}

fn status_str(s: PodStatus) -> &'static str {
    match s {
        PodStatus::Draft      => "draft",
        PodStatus::Submitted  => "submitted",
        PodStatus::Verified   => "verified",
        PodStatus::Disputed   => "disputed",
    }
}

impl From<PodRow> for ProofOfDelivery {
    fn from(r: PodRow) -> Self {
        let photos: Vec<PodPhoto> = serde_json::from_value(r.photos).unwrap_or_default();
        ProofOfDelivery {
            id: r.id,
            tenant_id: r.tenant_id,
            shipment_id: r.shipment_id,
            task_id: r.task_id,
            driver_id: r.driver_id,
            status: parse_status(&r.status),
            signature_data: r.signature_data,
            recipient_name: r.recipient_name,
            photos,
            capture_lat: r.capture_lat,
            capture_lng: r.capture_lng,
            geofence_verified: r.geofence_verified,
            otp_verified: r.otp_verified,
            otp_id: r.otp_id,
            cod_collected_cents: r.cod_collected_cents,
            requires_photo: r.requires_photo,
            requires_signature: r.requires_signature,
            captured_at: r.captured_at,
            created_at: r.created_at,
        }
    }
}

#[async_trait]
impl PodRepository for PgPodRepository {
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<ProofOfDelivery>> {
        let row = sqlx::query_as::<_, PodRow>(
            r#"SELECT id, tenant_id, shipment_id, task_id, driver_id, status,
                      signature_data, recipient_name, photos,
                      capture_lat, capture_lng, geofence_verified,
                      otp_verified, otp_id, cod_collected_cents,
                      requires_photo, requires_signature,
                      captured_at, created_at
               FROM pod.proofs WHERE id = $1"#
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(ProofOfDelivery::from))
    }

    async fn find_by_shipment(&self, shipment_id: Uuid) -> anyhow::Result<Option<ProofOfDelivery>> {
        let row = sqlx::query_as::<_, PodRow>(
            r#"SELECT id, tenant_id, shipment_id, task_id, driver_id, status,
                      signature_data, recipient_name, photos,
                      capture_lat, capture_lng, geofence_verified,
                      otp_verified, otp_id, cod_collected_cents,
                      requires_photo, requires_signature,
                      captured_at, created_at
               FROM pod.proofs WHERE shipment_id = $1
               ORDER BY created_at DESC LIMIT 1"#
        )
        .bind(shipment_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(ProofOfDelivery::from))
    }

    async fn save(&self, pod: &ProofOfDelivery) -> anyhow::Result<()> {
        let status = status_str(pod.status);
        let photos = serde_json::to_value(&pod.photos)?;
        sqlx::query(
            r#"INSERT INTO pod.proofs
                   (id, tenant_id, shipment_id, task_id, driver_id, status,
                    signature_data, recipient_name, photos,
                    capture_lat, capture_lng, geofence_verified,
                    otp_verified, otp_id, cod_collected_cents,
                    requires_photo, requires_signature,
                    captured_at, created_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)
               ON CONFLICT (id) DO UPDATE SET
                   status              = EXCLUDED.status,
                   signature_data      = EXCLUDED.signature_data,
                   photos              = EXCLUDED.photos,
                   otp_verified        = EXCLUDED.otp_verified,
                   otp_id              = EXCLUDED.otp_id,
                   cod_collected_cents = EXCLUDED.cod_collected_cents"#
        )
        .bind(pod.id)
        .bind(pod.tenant_id)
        .bind(pod.shipment_id)
        .bind(pod.task_id)
        .bind(pod.driver_id)
        .bind(status)
        .bind(&pod.signature_data)
        .bind(&pod.recipient_name)
        .bind(photos)
        .bind(pod.capture_lat)
        .bind(pod.capture_lng)
        .bind(pod.geofence_verified)
        .bind(pod.otp_verified)
        .bind(pod.otp_id)
        .bind(pod.cod_collected_cents)
        .bind(pod.requires_photo)
        .bind(pod.requires_signature)
        .bind(pod.captured_at)
        .bind(pod.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
