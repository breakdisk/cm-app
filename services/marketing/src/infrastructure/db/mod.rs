use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use logisticos_types::TenantId;

use crate::domain::{
    entities::{Campaign, CampaignId, CampaignStatus, Channel},
    repositories::CampaignRepository,
};

#[derive(sqlx::FromRow)]
struct CampaignRow {
    id:              Uuid,
    tenant_id:       Uuid,
    name:            String,
    description:     Option<String>,
    channel:         String,
    template:        serde_json::Value,
    targeting:       serde_json::Value,
    status:          String,
    scheduled_at:    Option<chrono::DateTime<chrono::Utc>>,
    sent_at:         Option<chrono::DateTime<chrono::Utc>>,
    completed_at:    Option<chrono::DateTime<chrono::Utc>>,
    total_sent:      i64,
    total_delivered: i64,
    total_failed:    i64,
    created_by:      Uuid,
    created_at:      chrono::DateTime<chrono::Utc>,
    updated_at:      chrono::DateTime<chrono::Utc>,
}

impl TryFrom<CampaignRow> for Campaign {
    type Error = anyhow::Error;
    fn try_from(r: CampaignRow) -> Result<Self, Self::Error> {
        Ok(Campaign {
            id:              CampaignId::from_uuid(r.id),
            tenant_id:       TenantId::from_uuid(r.tenant_id),
            name:            r.name,
            description:     r.description,
            channel:         serde_json::from_value(serde_json::Value::String(r.channel))?,
            template:        serde_json::from_value(r.template)?,
            targeting:       serde_json::from_value(r.targeting)?,
            status:          serde_json::from_value(serde_json::Value::String(r.status))?,
            scheduled_at:    r.scheduled_at,
            sent_at:         r.sent_at,
            completed_at:    r.completed_at,
            total_sent:      r.total_sent as u64,
            total_delivered: r.total_delivered as u64,
            total_failed:    r.total_failed as u64,
            created_by:      r.created_by,
            created_at:      r.created_at,
            updated_at:      r.updated_at,
        })
    }
}

pub struct PgCampaignRepository {
    pool: PgPool,
}

impl PgCampaignRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl CampaignRepository for PgCampaignRepository {
    async fn find_by_id(&self, id: &CampaignId) -> anyhow::Result<Option<Campaign>> {
        let row = sqlx::query_as::<_, CampaignRow>(
            r#"
            SELECT id, tenant_id, name, description, channel, template, targeting, status,
                   scheduled_at, sent_at, completed_at,
                   total_sent, total_delivered, total_failed,
                   created_by, created_at, updated_at
            FROM marketing.campaigns WHERE id = $1
            "#
        )
        .bind(id.inner())
        .fetch_optional(&self.pool)
        .await?;
        row.map(Campaign::try_from).transpose()
    }

    async fn list(&self, tenant_id: &TenantId, limit: i64, offset: i64) -> anyhow::Result<Vec<Campaign>> {
        let rows = sqlx::query_as::<_, CampaignRow>(
            r#"
            SELECT id, tenant_id, name, description, channel, template, targeting, status,
                   scheduled_at, sent_at, completed_at,
                   total_sent, total_delivered, total_failed,
                   created_by, created_at, updated_at
            FROM marketing.campaigns
            WHERE tenant_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#
        )
        .bind(tenant_id.inner()).bind(limit).bind(offset)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(Campaign::try_from).collect()
    }

    async fn list_by_status(&self, tenant_id: &TenantId, status: &CampaignStatus) -> anyhow::Result<Vec<Campaign>> {
        let status_str = serde_json::to_value(status)?.as_str().unwrap_or("draft").to_owned();
        let rows = sqlx::query_as::<_, CampaignRow>(
            r#"
            SELECT id, tenant_id, name, description, channel, template, targeting, status,
                   scheduled_at, sent_at, completed_at,
                   total_sent, total_delivered, total_failed,
                   created_by, created_at, updated_at
            FROM marketing.campaigns
            WHERE tenant_id = $1 AND status = $2
            ORDER BY created_at DESC
            "#
        )
        .bind(tenant_id.inner()).bind(status_str)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(Campaign::try_from).collect()
    }

    async fn save(&self, c: &Campaign) -> anyhow::Result<()> {
        let channel_str = serde_json::to_value(&c.channel)?.as_str().unwrap_or("sms").to_owned();
        let status_str  = serde_json::to_value(&c.status)?.as_str().unwrap_or("draft").to_owned();
        let template    = serde_json::to_value(&c.template)?;
        let targeting   = serde_json::to_value(&c.targeting)?;

        sqlx::query(
            r#"
            INSERT INTO marketing.campaigns (
                id, tenant_id, name, description, channel, template, targeting, status,
                scheduled_at, sent_at, completed_at,
                total_sent, total_delivered, total_failed,
                created_by, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
            ON CONFLICT (id) DO UPDATE SET
                name             = EXCLUDED.name,
                description      = EXCLUDED.description,
                channel          = EXCLUDED.channel,
                template         = EXCLUDED.template,
                targeting        = EXCLUDED.targeting,
                status           = EXCLUDED.status,
                scheduled_at     = EXCLUDED.scheduled_at,
                sent_at          = EXCLUDED.sent_at,
                completed_at     = EXCLUDED.completed_at,
                total_sent       = EXCLUDED.total_sent,
                total_delivered  = EXCLUDED.total_delivered,
                total_failed     = EXCLUDED.total_failed,
                updated_at       = EXCLUDED.updated_at
            "#
        )
        .bind(c.id.inner()).bind(c.tenant_id.inner()).bind(&c.name).bind(&c.description)
        .bind(channel_str).bind(template).bind(targeting).bind(status_str)
        .bind(c.scheduled_at).bind(c.sent_at).bind(c.completed_at)
        .bind(c.total_sent as i64).bind(c.total_delivered as i64).bind(c.total_failed as i64)
        .bind(c.created_by).bind(c.created_at).bind(c.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
