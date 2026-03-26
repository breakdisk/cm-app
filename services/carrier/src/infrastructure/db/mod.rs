use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use logisticos_types::TenantId;

use crate::domain::{
    entities::{Carrier, CarrierId, CarrierStatus, PerformanceGrade, SlaCommitment},
    repositories::CarrierRepository,
};

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
        let row = sqlx::query_as!(CarrierRow,
            "SELECT id, tenant_id, name, code, contact_email, contact_phone, api_endpoint, api_key_hash, \
             status, sla, rate_cards, total_shipments, on_time_count, failed_count, performance_grade, \
             onboarded_at, updated_at FROM carrier.carriers WHERE id = $1",
            id.inner()
        ).fetch_optional(&self.pool).await?;
        row.map(Carrier::try_from).transpose()
    }

    async fn find_by_code(&self, tenant_id: &TenantId, code: &str) -> anyhow::Result<Option<Carrier>> {
        let row = sqlx::query_as!(CarrierRow,
            "SELECT id, tenant_id, name, code, contact_email, contact_phone, api_endpoint, api_key_hash, \
             status, sla, rate_cards, total_shipments, on_time_count, failed_count, performance_grade, \
             onboarded_at, updated_at FROM carrier.carriers WHERE tenant_id = $1 AND code = $2",
            tenant_id.inner(), code
        ).fetch_optional(&self.pool).await?;
        row.map(Carrier::try_from).transpose()
    }

    async fn list(&self, tenant_id: &TenantId, limit: i64, offset: i64) -> anyhow::Result<Vec<Carrier>> {
        let rows = sqlx::query_as!(CarrierRow,
            "SELECT id, tenant_id, name, code, contact_email, contact_phone, api_endpoint, api_key_hash, \
             status, sla, rate_cards, total_shipments, on_time_count, failed_count, performance_grade, \
             onboarded_at, updated_at FROM carrier.carriers \
             WHERE tenant_id = $1 AND status != 'deactivated' \
             ORDER BY name ASC LIMIT $2 OFFSET $3",
            tenant_id.inner(), limit, offset
        ).fetch_all(&self.pool).await?;
        rows.into_iter().map(Carrier::try_from).collect()
    }

    async fn list_active(&self, tenant_id: &TenantId) -> anyhow::Result<Vec<Carrier>> {
        let rows = sqlx::query_as!(CarrierRow,
            "SELECT id, tenant_id, name, code, contact_email, contact_phone, api_endpoint, api_key_hash, \
             status, sla, rate_cards, total_shipments, on_time_count, failed_count, performance_grade, \
             onboarded_at, updated_at FROM carrier.carriers \
             WHERE tenant_id = $1 AND status = 'active'",
            tenant_id.inner()
        ).fetch_all(&self.pool).await?;
        rows.into_iter().map(Carrier::try_from).collect()
    }

    async fn save(&self, c: &Carrier) -> anyhow::Result<()> {
        let status    = serde_json::to_value(&c.status)?.as_str().unwrap_or("pending_verification").to_owned();
        let grade     = serde_json::to_value(&c.performance_grade)?.as_str().unwrap_or("good").to_owned();
        let sla       = serde_json::to_value(&c.sla)?;
        let rate_cards = serde_json::to_value(&c.rate_cards)?;

        sqlx::query!(
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
            "#,
            c.id.inner(), c.tenant_id.inner(), c.name, c.code, c.contact_email, c.contact_phone,
            c.api_endpoint, c.api_key_hash, status, sla, rate_cards,
            c.total_shipments, c.on_time_count, c.failed_count, grade,
            c.onboarded_at, c.updated_at,
        ).execute(&self.pool).await?;
        Ok(())
    }
}
