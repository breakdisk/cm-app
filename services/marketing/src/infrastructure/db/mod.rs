use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use logisticos_types::TenantId;

use crate::domain::{
    entities::{Campaign, CampaignId, CampaignRecipient, CampaignStatus, Channel, WeeklyStat},
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

    async fn weekly_stats(&self, tenant_id: &TenantId) -> anyhow::Result<Vec<WeeklyStat>> {
        #[derive(sqlx::FromRow)]
        struct StatRow {
            day:     chrono::NaiveDate,
            channel: String,
            count:   i64,
        }

        let rows = sqlx::query_as::<_, StatRow>(
            r#"
            SELECT DATE(sent_at)  AS day,
                   channel        AS channel,
                   SUM(total_sent)::bigint AS count
            FROM marketing.campaigns
            WHERE tenant_id = $1
              AND sent_at   >= NOW() - INTERVAL '7 days'
              AND total_sent > 0
            GROUP BY DATE(sent_at), channel
            ORDER BY day ASC
            "#
        )
        .bind(tenant_id.inner())
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| WeeklyStat {
                day:     r.day.to_string(),
                channel: r.channel,
                count:   r.count,
            })
            .collect())
    }

    async fn patch_recipients(
        &self,
        id:         &CampaignId,
        recipients: Vec<CampaignRecipient>,
    ) -> anyhow::Result<()> {
        // Serialise recipients into JSON and merge them into the targeting JSONB
        // column using jsonb_set — this avoids a full round-trip read+rewrite.
        let recipients_json = serde_json::to_value(&recipients)?;
        sqlx::query(
            r#"
            UPDATE marketing.campaigns
               SET targeting   = jsonb_set(targeting, '{recipients}', $2::jsonb, true),
                   updated_at  = NOW()
             WHERE id = $1
            "#
        )
        .bind(id.inner())
        .bind(recipients_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find_campaigns_due(&self, limit: i64) -> anyhow::Result<Vec<Campaign>> {
        let rows = sqlx::query_as::<_, CampaignRow>(
            r#"
            SELECT id, tenant_id, name, description, channel, template, targeting, status,
                   scheduled_at, sent_at, completed_at,
                   total_sent, total_delivered, total_failed,
                   created_by, created_at, updated_at
            FROM marketing.campaigns
            WHERE status = 'scheduled'
              AND scheduled_at <= NOW()
            ORDER BY scheduled_at ASC
            LIMIT $1
            "#
        )
        .bind(limit)
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

// ---------------------------------------------------------------------------
// PgAbTestRepository
// ---------------------------------------------------------------------------

use crate::domain::entities::{AbTest, AbVariant, AbVariantStats};

pub struct PgAbTestRepository { pool: PgPool }

impl PgAbTestRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }

    pub async fn create(&self, test: &AbTest) -> anyhow::Result<()> {
        let variants_json = serde_json::to_value(&test.variants)?;
        sqlx::query(
            r#"
            INSERT INTO marketing.ab_tests
                (id, tenant_id, campaign_id, name, variants, started_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#
        )
        .bind(test.id).bind(test.tenant_id).bind(test.campaign_id)
        .bind(&test.name).bind(variants_json).bind(test.started_at)
        .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn find_by_campaign(&self, campaign_id: Uuid) -> anyhow::Result<Option<AbTest>> {
        #[derive(sqlx::FromRow)]
        struct Row {
            id: Uuid, tenant_id: Uuid, campaign_id: Uuid, name: String,
            variants: serde_json::Value, winner_variant: Option<String>,
            started_at: chrono::DateTime<chrono::Utc>,
            concluded_at: Option<chrono::DateTime<chrono::Utc>>,
        }
        let row = sqlx::query_as::<_, Row>(
            "SELECT id, tenant_id, campaign_id, name, variants, winner_variant, started_at, concluded_at
             FROM marketing.ab_tests WHERE campaign_id = $1 LIMIT 1"
        )
        .bind(campaign_id).fetch_optional(&self.pool).await?;

        row.map(|r| -> anyhow::Result<AbTest> {
            Ok(AbTest {
                id: r.id, tenant_id: r.tenant_id, campaign_id: r.campaign_id, name: r.name,
                variants: serde_json::from_value(r.variants)?,
                winner_variant: r.winner_variant,
                started_at: r.started_at, concluded_at: r.concluded_at,
            })
        }).transpose()
    }

    /// Aggregate send performance per variant from `marketing.send_log`.
    pub async fn get_stats(&self, campaign_id: Uuid) -> anyhow::Result<Vec<AbVariantStats>> {
        #[derive(sqlx::FromRow)]
        struct Row { variant: String, sent: i64, delivered: i64, opened: i64, clicked: i64 }
        let rows = sqlx::query_as::<_, Row>(
            r#"
            SELECT variant,
                   COUNT(*)                                    AS sent,
                   COUNT(*) FILTER (WHERE delivered_at IS NOT NULL) AS delivered,
                   COUNT(*) FILTER (WHERE opened_at    IS NOT NULL) AS opened,
                   COUNT(*) FILTER (WHERE clicked_at   IS NOT NULL) AS clicked
            FROM marketing.send_log
            WHERE campaign_id = $1
            GROUP BY variant
            ORDER BY variant
            "#
        )
        .bind(campaign_id).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(|r| AbVariantStats {
            variant: r.variant, sent: r.sent, delivered: r.delivered,
            opened: r.opened, clicked: r.clicked,
        }).collect())
    }

    pub async fn set_winner(&self, campaign_id: Uuid, variant: &str) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE marketing.ab_tests SET winner_variant = $1, concluded_at = NOW()
             WHERE campaign_id = $2"
        )
        .bind(variant).bind(campaign_id).execute(&self.pool).await?;
        Ok(())
    }

    /// Write a batch of send-log rows for variant tracking (called at activation).
    pub async fn log_variant_sends(
        &self,
        campaign_id: Uuid,
        tenant_id:   Uuid,
        channel:     &str,
        template_id: &str,
        variant:     &str,
        customer_ids: &[Uuid],
    ) -> anyhow::Result<()> {
        for &cid in customer_ids {
            sqlx::query(
                r#"INSERT INTO marketing.send_log
                   (tenant_id, campaign_id, customer_id, channel, template_id, variant, status)
                   VALUES ($1, $2, $3, $4, $5, $6, 'queued')"#
            )
            .bind(tenant_id).bind(campaign_id).bind(cid)
            .bind(channel).bind(template_id).bind(variant)
            .execute(&self.pool).await?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// PgJourneyRepository
// ---------------------------------------------------------------------------

use crate::domain::entities::{Journey, JourneyEnrollment, JourneyStatus, JourneyStep};

pub struct PgJourneyRepository { pool: PgPool }

impl PgJourneyRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }

    pub async fn save(&self, j: &Journey) -> anyhow::Result<()> {
        let status = serde_json::to_value(&j.status)?.as_str().unwrap_or("draft").to_owned();
        sqlx::query(
            r#"
            INSERT INTO marketing.journeys (id, tenant_id, name, description, trigger, status, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name, description = EXCLUDED.description,
                trigger = EXCLUDED.trigger, status = EXCLUDED.status, updated_at = EXCLUDED.updated_at
            "#
        )
        .bind(j.id).bind(j.tenant_id).bind(&j.name).bind(&j.description)
        .bind(&j.trigger).bind(status).bind(j.created_at).bind(j.updated_at)
        .execute(&self.pool).await?;

        // Replace steps: delete then re-insert
        sqlx::query("DELETE FROM marketing.journey_steps WHERE journey_id = $1")
            .bind(j.id).execute(&self.pool).await?;

        for step in &j.steps {
            sqlx::query(
                r#"INSERT INTO marketing.journey_steps
                   (id, journey_id, step_order, step_type, campaign_id, wait_days,
                    condition_type, condition_campaign_id, yes_next_order, no_next_order)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#
            )
            .bind(step.id).bind(step.journey_id).bind(step.step_order).bind(&step.step_type)
            .bind(step.campaign_id).bind(step.wait_days)
            .bind(&step.condition_type).bind(step.condition_campaign_id)
            .bind(step.yes_next_order).bind(step.no_next_order)
            .execute(&self.pool).await?;
        }
        Ok(())
    }

    pub async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<Journey>> {
        #[derive(sqlx::FromRow)]
        struct JRow {
            id: Uuid, tenant_id: Uuid, name: String, description: Option<String>,
            trigger: serde_json::Value, status: String,
            created_at: chrono::DateTime<chrono::Utc>,
            updated_at: chrono::DateTime<chrono::Utc>,
        }
        #[derive(sqlx::FromRow)]
        struct SRow {
            id: Uuid, journey_id: Uuid, step_order: i32, step_type: String,
            campaign_id: Option<Uuid>, wait_days: Option<i32>,
            condition_type: Option<String>, condition_campaign_id: Option<Uuid>,
            yes_next_order: Option<i32>, no_next_order: Option<i32>,
        }

        let Some(jr) = sqlx::query_as::<_, JRow>(
            "SELECT id, tenant_id, name, description, trigger, status, created_at, updated_at
             FROM marketing.journeys WHERE id = $1"
        ).bind(id).fetch_optional(&self.pool).await? else { return Ok(None) };

        let step_rows = sqlx::query_as::<_, SRow>(
            "SELECT id, journey_id, step_order, step_type, campaign_id, wait_days,
                    condition_type, condition_campaign_id, yes_next_order, no_next_order
             FROM marketing.journey_steps WHERE journey_id = $1 ORDER BY step_order ASC"
        ).bind(id).fetch_all(&self.pool).await?;

        let steps = step_rows.into_iter().map(|s| JourneyStep {
            id: s.id, journey_id: s.journey_id, step_order: s.step_order,
            step_type: s.step_type, campaign_id: s.campaign_id, wait_days: s.wait_days,
            condition_type: s.condition_type, condition_campaign_id: s.condition_campaign_id,
            yes_next_order: s.yes_next_order, no_next_order: s.no_next_order,
        }).collect();

        let status: JourneyStatus = serde_json::from_value(serde_json::Value::String(jr.status))?;
        Ok(Some(Journey {
            id: jr.id, tenant_id: jr.tenant_id, name: jr.name, description: jr.description,
            trigger: jr.trigger, status, steps,
            created_at: jr.created_at, updated_at: jr.updated_at,
        }))
    }

    pub async fn list(&self, tenant_id: Uuid) -> anyhow::Result<Vec<Journey>> {
        #[derive(sqlx::FromRow)]
        struct JRow {
            id: Uuid, tenant_id: Uuid, name: String, description: Option<String>,
            trigger: serde_json::Value, status: String,
            created_at: chrono::DateTime<chrono::Utc>,
            updated_at: chrono::DateTime<chrono::Utc>,
        }
        let rows = sqlx::query_as::<_, JRow>(
            "SELECT id, tenant_id, name, description, trigger, status, created_at, updated_at
             FROM marketing.journeys WHERE tenant_id = $1 ORDER BY created_at DESC"
        ).bind(tenant_id).fetch_all(&self.pool).await?;

        let mut journeys = Vec::with_capacity(rows.len());
        for jr in rows {
            let status: JourneyStatus = serde_json::from_value(serde_json::Value::String(jr.status))?;
            journeys.push(Journey {
                id: jr.id, tenant_id: jr.tenant_id, name: jr.name, description: jr.description,
                trigger: jr.trigger, status, steps: vec![],  // steps not loaded in list view
                created_at: jr.created_at, updated_at: jr.updated_at,
            });
        }
        Ok(journeys)
    }

    pub async fn delete(&self, id: Uuid) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM marketing.journeys WHERE id = $1")
            .bind(id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn save_enrollment(&self, e: &JourneyEnrollment) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO marketing.journey_enrollments
                (id, journey_id, tenant_id, customer_id, current_step_order, status, next_action_at, enrolled_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW())
            ON CONFLICT (journey_id, customer_id) DO UPDATE SET
                current_step_order = EXCLUDED.current_step_order,
                status             = EXCLUDED.status,
                next_action_at     = EXCLUDED.next_action_at,
                updated_at         = NOW()
            "#
        )
        .bind(e.id).bind(e.journey_id).bind(e.tenant_id).bind(e.customer_id)
        .bind(e.current_step_order).bind(&e.status).bind(e.next_action_at).bind(e.enrolled_at)
        .execute(&self.pool).await?;
        Ok(())
    }

    /// Fetch enrollments whose next_action_at is due, up to `limit`.
    pub async fn find_due_enrollments(&self, limit: i64) -> anyhow::Result<Vec<JourneyEnrollment>> {
        #[derive(sqlx::FromRow)]
        struct Row {
            id: Uuid, journey_id: Uuid, tenant_id: Uuid, customer_id: Uuid,
            current_step_order: Option<i32>, status: String,
            next_action_at: Option<chrono::DateTime<chrono::Utc>>,
            enrolled_at: chrono::DateTime<chrono::Utc>,
        }
        let rows = sqlx::query_as::<_, Row>(
            r#"SELECT id, journey_id, tenant_id, customer_id, current_step_order, status,
                      next_action_at, enrolled_at
               FROM marketing.journey_enrollments
               WHERE status = 'active' AND next_action_at <= NOW()
               ORDER BY next_action_at ASC LIMIT $1"#
        ).bind(limit).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(|r| JourneyEnrollment {
            id: r.id, journey_id: r.journey_id, tenant_id: r.tenant_id, customer_id: r.customer_id,
            current_step_order: r.current_step_order, status: r.status,
            next_action_at: r.next_action_at, enrolled_at: r.enrolled_at,
        }).collect())
    }

    pub async fn advance_enrollment(
        &self,
        enrollment_id:    Uuid,
        next_step_order:  Option<i32>,
        next_action_at:   Option<chrono::DateTime<chrono::Utc>>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE marketing.journey_enrollments
             SET current_step_order = $2, next_action_at = $3, updated_at = NOW()
             WHERE id = $1"
        )
        .bind(enrollment_id).bind(next_step_order).bind(next_action_at)
        .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn complete_enrollment(&self, enrollment_id: Uuid) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE marketing.journey_enrollments
             SET status = 'completed', next_action_at = NULL, updated_at = NOW()
             WHERE id = $1"
        )
        .bind(enrollment_id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn list_enrollments(&self, journey_id: Uuid) -> anyhow::Result<Vec<JourneyEnrollment>> {
        #[derive(sqlx::FromRow)]
        struct Row {
            id: Uuid, journey_id: Uuid, tenant_id: Uuid, customer_id: Uuid,
            current_step_order: Option<i32>, status: String,
            next_action_at: Option<chrono::DateTime<chrono::Utc>>,
            enrolled_at: chrono::DateTime<chrono::Utc>,
        }
        let rows = sqlx::query_as::<_, Row>(
            "SELECT id, journey_id, tenant_id, customer_id, current_step_order, status,
                    next_action_at, enrolled_at
             FROM marketing.journey_enrollments WHERE journey_id = $1 ORDER BY enrolled_at DESC"
        ).bind(journey_id).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(|r| JourneyEnrollment {
            id: r.id, journey_id: r.journey_id, tenant_id: r.tenant_id, customer_id: r.customer_id,
            current_step_order: r.current_step_order, status: r.status,
            next_action_at: r.next_action_at, enrolled_at: r.enrolled_at,
        }).collect())
    }
}
