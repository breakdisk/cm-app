use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use logisticos_types::TenantId;

use crate::domain::{entities::TrackingRecord, repositories::TrackingRepository};

#[derive(sqlx::FromRow)]
struct TrackingRow {
    shipment_id:         Uuid,
    tenant_id:           Uuid,
    tracking_number:     String,
    current_status:      String,
    status_history:      serde_json::Value,
    origin_address:      String,
    destination_address: String,
    driver_id:           Option<Uuid>,
    driver_name:         Option<String>,
    driver_phone:        Option<String>,
    driver_position:     Option<serde_json::Value>,
    estimated_delivery:  Option<chrono::DateTime<chrono::Utc>>,
    delivered_at:        Option<chrono::DateTime<chrono::Utc>>,
    pod_id:              Option<Uuid>,
    recipient_name:      Option<String>,
    attempt_number:      i16,
    next_attempt_at:     Option<chrono::DateTime<chrono::Utc>>,
    reschedule_count:    i32,
    created_at:          chrono::DateTime<chrono::Utc>,
    updated_at:          chrono::DateTime<chrono::Utc>,
}

impl TryFrom<TrackingRow> for TrackingRecord {
    type Error = anyhow::Error;

    fn try_from(r: TrackingRow) -> Result<Self, Self::Error> {
        use crate::domain::entities::TrackingStatus;
        let status: TrackingStatus = serde_json::from_value(serde_json::Value::String(r.current_status))?;
        let status_history = serde_json::from_value(r.status_history)?;
        let driver_position = r.driver_position.map(serde_json::from_value).transpose()?;

        Ok(TrackingRecord {
            shipment_id:         r.shipment_id,
            tenant_id:           TenantId::from_uuid(r.tenant_id),
            tracking_number:     r.tracking_number,
            current_status:      status,
            status_history,
            origin_address:      r.origin_address,
            destination_address: r.destination_address,
            driver_id:           r.driver_id,
            driver_name:         r.driver_name,
            driver_phone:        r.driver_phone,
            driver_position,
            estimated_delivery:  r.estimated_delivery,
            delivered_at:        r.delivered_at,
            pod_id:              r.pod_id,
            recipient_name:      r.recipient_name,
            attempt_number:      r.attempt_number as u8,
            next_attempt_at:     r.next_attempt_at,
            reschedule_count:    r.reschedule_count,
            created_at:          r.created_at,
            updated_at:          r.updated_at,
        })
    }
}

pub struct PgTrackingRepository {
    pool: PgPool,
}

impl PgTrackingRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl TrackingRepository for PgTrackingRepository {
    async fn find_by_shipment_id(&self, shipment_id: Uuid) -> anyhow::Result<Option<TrackingRecord>> {
        let row = sqlx::query_as::<_, TrackingRow>(
            r#"
            SELECT shipment_id, tenant_id, tracking_number, current_status,
                   status_history, origin_address, destination_address,
                   driver_id, driver_name, driver_phone,
                   driver_position, estimated_delivery, delivered_at,
                   pod_id, recipient_name, attempt_number, next_attempt_at,
                   reschedule_count, created_at, updated_at
            FROM tracking.shipment_tracking
            WHERE shipment_id = $1
            "#
        )
        .bind(shipment_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(TrackingRecord::try_from).transpose()
    }

    async fn find_by_tracking_number(&self, tracking_number: &str) -> anyhow::Result<Option<TrackingRecord>> {
        let row = sqlx::query_as::<_, TrackingRow>(
            r#"
            SELECT shipment_id, tenant_id, tracking_number, current_status,
                   status_history, origin_address, destination_address,
                   driver_id, driver_name, driver_phone,
                   driver_position, estimated_delivery, delivered_at,
                   pod_id, recipient_name, attempt_number, next_attempt_at,
                   reschedule_count, created_at, updated_at
            FROM tracking.shipment_tracking
            WHERE tracking_number = $1
            "#
        )
        .bind(tracking_number)
        .fetch_optional(&self.pool)
        .await?;
        row.map(TrackingRecord::try_from).transpose()
    }

    async fn list_by_tenant(
        &self,
        tenant_id: &TenantId,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<TrackingRecord>> {
        let rows = sqlx::query_as::<_, TrackingRow>(
            r#"
            SELECT shipment_id, tenant_id, tracking_number, current_status,
                   status_history, origin_address, destination_address,
                   driver_id, driver_name, driver_phone,
                   driver_position, estimated_delivery, delivered_at,
                   pod_id, recipient_name, attempt_number, next_attempt_at,
                   reschedule_count, created_at, updated_at
            FROM tracking.shipment_tracking
            WHERE tenant_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#
        )
        .bind(tenant_id.inner())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(TrackingRecord::try_from).collect()
    }

    async fn save(&self, r: &TrackingRecord) -> anyhow::Result<()> {
        let status_str = serde_json::to_value(&r.current_status)?
            .as_str().unwrap_or("pending").to_owned();
        let history_json  = serde_json::to_value(&r.status_history)?;
        let position_json = r.driver_position
            .as_ref()
            .map(serde_json::to_value)
            .transpose()?;

        sqlx::query(
            r#"
            INSERT INTO tracking.shipment_tracking (
                shipment_id, tenant_id, tracking_number, current_status,
                status_history, origin_address, destination_address,
                driver_id, driver_name, driver_phone,
                driver_position, estimated_delivery, delivered_at,
                pod_id, recipient_name, attempt_number, next_attempt_at,
                reschedule_count, created_at, updated_at
            ) VALUES (
                $1, $2, $3, $4,
                $5, $6, $7,
                $8, $9, $10,
                $11, $12, $13,
                $14, $15, $16, $17,
                $18, $19, $20
            )
            ON CONFLICT (shipment_id) DO UPDATE SET
                current_status     = EXCLUDED.current_status,
                status_history     = EXCLUDED.status_history,
                driver_id          = EXCLUDED.driver_id,
                driver_name        = EXCLUDED.driver_name,
                driver_phone       = EXCLUDED.driver_phone,
                driver_position    = EXCLUDED.driver_position,
                estimated_delivery = EXCLUDED.estimated_delivery,
                delivered_at       = EXCLUDED.delivered_at,
                pod_id             = EXCLUDED.pod_id,
                recipient_name     = EXCLUDED.recipient_name,
                attempt_number     = EXCLUDED.attempt_number,
                next_attempt_at    = EXCLUDED.next_attempt_at,
                reschedule_count   = EXCLUDED.reschedule_count,
                updated_at         = EXCLUDED.updated_at
            "#
        )
        .bind(r.shipment_id)
        .bind(r.tenant_id.inner())
        .bind(&r.tracking_number)
        .bind(&status_str)
        .bind(history_json)
        .bind(&r.origin_address)
        .bind(&r.destination_address)
        .bind(r.driver_id)
        .bind(&r.driver_name)
        .bind(&r.driver_phone)
        .bind(position_json)
        .bind(r.estimated_delivery)
        .bind(r.delivered_at)
        .bind(r.pod_id)
        .bind(&r.recipient_name)
        .bind(r.attempt_number as i16)
        .bind(r.next_attempt_at)
        .bind(r.reschedule_count)
        .bind(r.created_at)
        .bind(r.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn reschedule(
        &self,
        tracking_number: &str,
        preferred_date: chrono::NaiveDate,
        preferred_time_slot: Option<&str>,
        reason: &str,
    ) -> anyhow::Result<()> {
        let hour: u32 = match preferred_time_slot {
            Some("afternoon") => 13,
            _ => 9, // "morning", "anytime", or None → 09:00 UTC
        };
        let next_attempt = preferred_date
            .and_hms_opt(hour, 0, 0)
            .map(|dt| chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(dt, chrono::Utc));
        let now = chrono::Utc::now();

        sqlx::query(
            r#"
            UPDATE tracking.shipment_tracking
            SET reschedule_count = reschedule_count + 1,
                next_attempt_at  = $1,
                updated_at       = $2
            WHERE tracking_number = $3
            "#
        )
        .bind(next_attempt)
        .bind(now)
        .bind(tracking_number)
        .execute(&self.pool)
        .await?;

        // Look up shipment_id and tenant_id for the event insert.
        let ids = sqlx::query_as::<_, (Uuid, Uuid)>(
            "SELECT shipment_id, tenant_id FROM tracking.shipment_tracking WHERE tracking_number = $1"
        )
        .bind(tracking_number)
        .fetch_optional(&self.pool)
        .await?;

        if let Some((shipment_id, tenant_id)) = ids {
            sqlx::query(
                r#"
                INSERT INTO delivery_experience.tracking_events
                    (id, tenant_id, shipment_id, tracking_number, event_type, description, occurred_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                "#
            )
            .bind(Uuid::new_v4())
            .bind(tenant_id)
            .bind(shipment_id)
            .bind(tracking_number)
            .bind("reschedule_requested")
            .bind(format!("Delivery rescheduled: {reason}"))
            .bind(now)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }
}
