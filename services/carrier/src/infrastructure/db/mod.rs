use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use logisticos_types::TenantId;

use crate::domain::{
    entities::{Carrier, CarrierId, CarrierStatus, PerformanceGrade, SlaCommitment, SlaRecord, SlaStatus, ZoneSlaRow},
    repositories::{CarrierRepository, SlaRecordRepository},
};

#[derive(sqlx::FromRow)]
struct CarrierRow {
    id:                Uuid,
    tenant_id:         Uuid,
    name:              String,
    code:              String,
    contact_email:     String,
    contact_phone:     Option<String>,
    api_endpoint:      Option<String>,
    api_key_hash:      Option<String>,
    status:            String,
    sla:               serde_json::Value,
    rate_cards:        serde_json::Value,
    total_shipments:   i64,
    on_time_count:     i64,
    failed_count:      i64,
    performance_grade: String,
    onboarded_at:      chrono::DateTime<chrono::Utc>,
    updated_at:        chrono::DateTime<chrono::Utc>,
}

impl TryFrom<CarrierRow> for Carrier {
    type Error = anyhow::Error;
    fn try_from(r: CarrierRow) -> Result<Self, Self::Error> {
        Ok(Carrier {
            id:                CarrierId::from_uuid(r.id),
            tenant_id:         TenantId::from_uuid(r.tenant_id),
            name:              r.name,
            code:              r.code,
            contact_email:     r.contact_email,
            contact_phone:     r.contact_phone,
            api_endpoint:      r.api_endpoint,
            api_key_hash:      r.api_key_hash,
            status:            serde_json::from_value(serde_json::Value::String(r.status))?,
            sla:               serde_json::from_value(r.sla)?,
            rate_cards:        serde_json::from_value(r.rate_cards)?,
            total_shipments:   r.total_shipments,
            on_time_count:     r.on_time_count,
            failed_count:      r.failed_count,
            performance_grade: serde_json::from_value(serde_json::Value::String(r.performance_grade))?,
            onboarded_at:      r.onboarded_at,
            updated_at:        r.updated_at,
        })
    }
}

pub struct PgCarrierRepository {
    pool: PgPool,
}

impl PgCarrierRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl CarrierRepository for PgCarrierRepository {
    async fn find_by_id(&self, id: &CarrierId) -> anyhow::Result<Option<Carrier>> {
        let row = sqlx::query_as::<_, CarrierRow>(
            "SELECT id, tenant_id, name, code, contact_email, contact_phone, api_endpoint, api_key_hash, \
             status, sla, rate_cards, total_shipments, on_time_count, failed_count, performance_grade, \
             onboarded_at, updated_at FROM carrier.carriers WHERE id = $1"
        ).bind(id.inner()).fetch_optional(&self.pool).await?;
        row.map(Carrier::try_from).transpose()
    }

    async fn find_by_code(&self, tenant_id: &TenantId, code: &str) -> anyhow::Result<Option<Carrier>> {
        let row = sqlx::query_as::<_, CarrierRow>(
            "SELECT id, tenant_id, name, code, contact_email, contact_phone, api_endpoint, api_key_hash, \
             status, sla, rate_cards, total_shipments, on_time_count, failed_count, performance_grade, \
             onboarded_at, updated_at FROM carrier.carriers WHERE tenant_id = $1 AND code = $2"
        ).bind(tenant_id.inner()).bind(code).fetch_optional(&self.pool).await?;
        row.map(Carrier::try_from).transpose()
    }

    async fn find_by_contact_email(&self, tenant_id: &TenantId, email: &str) -> anyhow::Result<Option<Carrier>> {
        let row = sqlx::query_as::<_, CarrierRow>(
            "SELECT id, tenant_id, name, code, contact_email, contact_phone, api_endpoint, api_key_hash, \
             status, sla, rate_cards, total_shipments, on_time_count, failed_count, performance_grade, \
             onboarded_at, updated_at FROM carrier.carriers \
             WHERE tenant_id = $1 AND lower(contact_email) = lower($2)"
        ).bind(tenant_id.inner()).bind(email).fetch_optional(&self.pool).await?;
        row.map(Carrier::try_from).transpose()
    }

    async fn list(&self, tenant_id: &TenantId, limit: i64, offset: i64) -> anyhow::Result<Vec<Carrier>> {
        let rows = sqlx::query_as::<_, CarrierRow>(
            "SELECT id, tenant_id, name, code, contact_email, contact_phone, api_endpoint, api_key_hash, \
             status, sla, rate_cards, total_shipments, on_time_count, failed_count, performance_grade, \
             onboarded_at, updated_at FROM carrier.carriers \
             WHERE tenant_id = $1 AND status != 'deactivated' \
             ORDER BY name ASC LIMIT $2 OFFSET $3"
        ).bind(tenant_id.inner()).bind(limit).bind(offset).fetch_all(&self.pool).await?;
        rows.into_iter().map(Carrier::try_from).collect()
    }

    async fn list_active(&self, tenant_id: &TenantId) -> anyhow::Result<Vec<Carrier>> {
        let rows = sqlx::query_as::<_, CarrierRow>(
            "SELECT id, tenant_id, name, code, contact_email, contact_phone, api_endpoint, api_key_hash, \
             status, sla, rate_cards, total_shipments, on_time_count, failed_count, performance_grade, \
             onboarded_at, updated_at FROM carrier.carriers \
             WHERE tenant_id = $1 AND status = 'active'"
        ).bind(tenant_id.inner()).fetch_all(&self.pool).await?;
        rows.into_iter().map(Carrier::try_from).collect()
    }

    async fn save(&self, c: &Carrier) -> anyhow::Result<()> {
        let status    = serde_json::to_value(&c.status)?.as_str().unwrap_or("pending_verification").to_owned();
        let grade     = serde_json::to_value(&c.performance_grade)?.as_str().unwrap_or("good").to_owned();
        let sla       = serde_json::to_value(&c.sla)?;
        let rate_cards = serde_json::to_value(&c.rate_cards)?;

        sqlx::query(
            r#"
            INSERT INTO carrier.carriers (
                id, tenant_id, name, code, contact_email, contact_phone,
                api_endpoint, api_key_hash, status, sla, rate_cards,
                total_shipments, on_time_count, failed_count, performance_grade,
                onboarded_at, updated_at
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name, contact_email = EXCLUDED.contact_email,
                contact_phone = EXCLUDED.contact_phone, api_endpoint = EXCLUDED.api_endpoint,
                api_key_hash = EXCLUDED.api_key_hash, status = EXCLUDED.status,
                sla = EXCLUDED.sla, rate_cards = EXCLUDED.rate_cards,
                total_shipments = EXCLUDED.total_shipments, on_time_count = EXCLUDED.on_time_count,
                failed_count = EXCLUDED.failed_count, performance_grade = EXCLUDED.performance_grade,
                updated_at = EXCLUDED.updated_at
            "#
        )
        .bind(c.id.inner()).bind(c.tenant_id.inner()).bind(&c.name).bind(&c.code).bind(&c.contact_email).bind(&c.contact_phone)
        .bind(&c.api_endpoint).bind(&c.api_key_hash).bind(status).bind(sla).bind(rate_cards)
        .bind(c.total_shipments).bind(c.on_time_count).bind(c.failed_count).bind(grade)
        .bind(c.onboarded_at).bind(c.updated_at)
        .execute(&self.pool).await?;
        Ok(())
    }
}

// ── SLA Record Repository ─────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct SlaRecordRow {
    id:             Uuid,
    tenant_id:      Uuid,
    carrier_id:     Uuid,
    shipment_id:    Uuid,
    zone:           String,
    service_level:  String,
    promised_by:    DateTime<Utc>,
    delivered_at:   Option<DateTime<Utc>>,
    status:         String,
    on_time:        Option<bool>,
    failure_reason: Option<String>,
    created_at:     DateTime<Utc>,
}

impl From<SlaRecordRow> for SlaRecord {
    fn from(r: SlaRecordRow) -> Self {
        let status = match r.status.as_str() {
            "delivered" => SlaStatus::Delivered,
            "failed"    => SlaStatus::Failed,
            _           => SlaStatus::InTransit,
        };
        SlaRecord {
            id:             r.id,
            tenant_id:      r.tenant_id,
            carrier_id:     r.carrier_id,
            shipment_id:    r.shipment_id,
            zone:           r.zone,
            service_level:  r.service_level,
            promised_by:    r.promised_by,
            delivered_at:   r.delivered_at,
            status,
            on_time:        r.on_time,
            failure_reason: r.failure_reason,
            created_at:     r.created_at,
        }
    }
}

pub struct PgSlaRecordRepository {
    pool: PgPool,
}

impl PgSlaRecordRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl SlaRecordRepository for PgSlaRecordRepository {
    async fn create(&self, r: &SlaRecord) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO carrier.sla_records
                (id, tenant_id, carrier_id, shipment_id, zone, service_level,
                 promised_by, status, created_at)
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
            ON CONFLICT (carrier_id, shipment_id) DO NOTHING
            "#
        )
        .bind(r.id).bind(r.tenant_id).bind(r.carrier_id).bind(r.shipment_id)
        .bind(&r.zone).bind(&r.service_level).bind(r.promised_by)
        .bind(r.status.as_str()).bind(r.created_at)
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn find_by_shipment(&self, shipment_id: Uuid) -> anyhow::Result<Option<SlaRecord>> {
        let row = sqlx::query_as::<_, SlaRecordRow>(
            "SELECT id, tenant_id, carrier_id, shipment_id, zone, service_level, \
             promised_by, delivered_at, status, on_time, failure_reason, created_at \
             FROM carrier.sla_records WHERE shipment_id = $1 LIMIT 1"
        ).bind(shipment_id).fetch_optional(&self.pool).await?;
        Ok(row.map(SlaRecord::from))
    }

    async fn save_outcome(&self, r: &SlaRecord) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE carrier.sla_records \
             SET delivered_at = $1, status = $2, on_time = $3, failure_reason = $4 \
             WHERE id = $5"
        )
        .bind(r.delivered_at).bind(r.status.as_str()).bind(r.on_time).bind(&r.failure_reason)
        .bind(r.id)
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn list_by_carrier(&self, carrier_id: Uuid, limit: i64, offset: i64) -> anyhow::Result<Vec<SlaRecord>> {
        let rows = sqlx::query_as::<_, SlaRecordRow>(
            "SELECT id, tenant_id, carrier_id, shipment_id, zone, service_level, \
             promised_by, delivered_at, status, on_time, failure_reason, created_at \
             FROM carrier.sla_records WHERE carrier_id = $1 \
             ORDER BY created_at DESC LIMIT $2 OFFSET $3"
        ).bind(carrier_id).bind(limit).bind(offset).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(SlaRecord::from).collect())
    }

    async fn zone_summary(
        &self,
        carrier_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> anyhow::Result<Vec<ZoneSlaRow>> {
        #[derive(sqlx::FromRow)]
        struct Row { zone: String, total: i64, on_time_count: i64, failed_count: i64 }

        let rows = sqlx::query_as::<_, Row>(
            r#"
            SELECT
                zone,
                COUNT(*)                                      AS total,
                COUNT(*) FILTER (WHERE on_time = true)        AS on_time_count,
                COUNT(*) FILTER (WHERE on_time = false)       AS failed_count
            FROM carrier.sla_records
            WHERE carrier_id = $1
              AND created_at >= $2
              AND created_at <  $3
              AND status != 'in_transit'
            GROUP BY zone
            ORDER BY total DESC
            "#
        ).bind(carrier_id).bind(from).bind(to).fetch_all(&self.pool).await?;

        Ok(rows.into_iter().map(|r| {
            let on_time_rate = if r.total > 0 {
                r.on_time_count as f64 / r.total as f64 * 100.0
            } else { 0.0 };
            ZoneSlaRow {
                zone:         r.zone,
                total:        r.total,
                on_time:      r.on_time_count,
                failed:       r.failed_count,
                on_time_rate,
            }
        }).collect())
    }
}
