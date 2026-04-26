/// Agent session persistence — immutable audit log of all agent runs.
use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::entities::{AgentSession, AgentType, SessionStatus};
use logisticos_types::TenantId;

#[async_trait]
pub trait SessionRepository: Send + Sync {
    async fn save(&self, session: &AgentSession) -> anyhow::Result<()>;
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<AgentSession>>;
    async fn list_by_tenant(&self, tenant_id: Uuid, limit: i64, offset: i64) -> anyhow::Result<Vec<AgentSession>>;
    async fn list_escalated(&self, tenant_id: Uuid) -> anyhow::Result<Vec<AgentSession>>;
    /// Aggregate counts + 24-hour invocation breakdown for the AI Agents
    /// dashboard. Single round-trip — counts run as one SQL query, the
    /// 24-bucket time series as a second.
    async fn aggregate(&self, tenant_id: Uuid) -> anyhow::Result<AggregateStats>;
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AggregateStats {
    pub total_today:       i64,
    pub completed_today:   i64,
    pub escalated_today:   i64,
    pub failed_today:      i64,
    pub success_rate_pct:  f64,   // completed / total today, in percent
    pub by_type_today:     std::collections::HashMap<String, i64>,
    pub hourly_24h:        Vec<HourlyBucket>,  // last 24 hours, oldest first
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct HourlyBucket {
    /// Hour of the day (0..23) in the server's tz. The chart bins by hour
    /// label rather than absolute timestamp so a 24-hour rolling window
    /// renders intuitively.
    pub hour:    i32,
    pub total:   i64,
    /// Per-agent-type counts so the dashboard can stack the bar (dispatch,
    /// support, fraud, etc.). Empty map for hours with no activity.
    pub by_type: std::collections::HashMap<String, i64>,
}

pub struct PgSessionRepository {
    pool: PgPool,
}

impl PgSessionRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

fn map_row(r: &sqlx::postgres::PgRow) -> AgentSession {
    let id: Uuid = r.get("id");
    let tenant_id: Uuid = r.get("tenant_id");
    let agent_type_str: String = r.get("agent_type");
    let status_str: String = r.get("status");
    AgentSession {
        id,
        tenant_id: TenantId::from_uuid(tenant_id),
        agent_type: serde_json::from_value(serde_json::Value::String(agent_type_str))
            .unwrap_or(AgentType::OnDemand),
        status: serde_json::from_value(serde_json::Value::String(status_str))
            .unwrap_or(SessionStatus::Failed),
        trigger:          r.get("trigger_data"),
        messages:         serde_json::from_value(r.get::<serde_json::Value, _>("messages")).unwrap_or_default(),
        actions:          serde_json::from_value(r.get::<serde_json::Value, _>("actions")).unwrap_or_default(),
        outcome:          r.get("outcome"),
        escalation_reason: r.get("escalation_reason"),
        confidence_score: r.get("confidence_score"),
        model_used:       r.get("model_used"),
        started_at:       r.get("started_at"),
        completed_at:     r.get("completed_at"),
    }
}

#[async_trait]
impl SessionRepository for PgSessionRepository {
    async fn save(&self, s: &AgentSession) -> anyhow::Result<()> {
        let agent_type = serde_json::to_value(&s.agent_type)?
            .as_str().unwrap_or("on_demand").to_owned();
        let status = serde_json::to_value(&s.status)?
            .as_str().unwrap_or("running").to_owned();
        let messages = serde_json::to_value(&s.messages)?;
        let actions  = serde_json::to_value(&s.actions)?;

        sqlx::query(
            r#"
            INSERT INTO ai.agent_sessions (
                id, tenant_id, agent_type, status, trigger_data, messages, actions,
                outcome, escalation_reason, confidence_score, model_used,
                started_at, completed_at
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
            ON CONFLICT (id) DO UPDATE SET
                status            = EXCLUDED.status,
                messages          = EXCLUDED.messages,
                actions           = EXCLUDED.actions,
                outcome           = EXCLUDED.outcome,
                escalation_reason = EXCLUDED.escalation_reason,
                confidence_score  = EXCLUDED.confidence_score,
                completed_at      = EXCLUDED.completed_at
            "#,
        )
        .bind(s.id)
        .bind(s.tenant_id.inner())
        .bind(&agent_type)
        .bind(&status)
        .bind(&s.trigger)
        .bind(&messages)
        .bind(&actions)
        .bind(&s.outcome)
        .bind(&s.escalation_reason)
        .bind(s.confidence_score)
        .bind(&s.model_used)
        .bind(s.started_at)
        .bind(s.completed_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<AgentSession>> {
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, agent_type, status, trigger_data,
                   messages, actions, outcome, escalation_reason,
                   confidence_score, model_used, started_at, completed_at
            FROM ai.agent_sessions WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.as_ref().map(map_row))
    }

    async fn list_by_tenant(&self, tenant_id: Uuid, limit: i64, offset: i64) -> anyhow::Result<Vec<AgentSession>> {
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, agent_type, status, trigger_data,
                   messages, actions, outcome, escalation_reason,
                   confidence_score, model_used, started_at, completed_at
            FROM ai.agent_sessions
            WHERE tenant_id = $1
            ORDER BY started_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(tenant_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(map_row).collect())
    }

    async fn list_escalated(&self, tenant_id: Uuid) -> anyhow::Result<Vec<AgentSession>> {
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, agent_type, status, trigger_data,
                   messages, actions, outcome, escalation_reason,
                   confidence_score, model_used, started_at, completed_at
            FROM ai.agent_sessions
            WHERE tenant_id = $1 AND status = 'human_escalated'
            ORDER BY started_at DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(map_row).collect())
    }

    async fn aggregate(&self, tenant_id: Uuid) -> anyhow::Result<AggregateStats> {
        // Today's counts grouped by status + agent_type, in one round trip.
        // CURRENT_DATE works in the server's tz which matches what the rest
        // of the dashboard uses for "today" semantics.
        let count_rows = sqlx::query(
            r#"
            SELECT
                status,
                agent_type,
                COUNT(*) AS n
            FROM ai.agent_sessions
            WHERE tenant_id = $1
              AND started_at >= CURRENT_DATE
            GROUP BY status, agent_type
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;

        let mut total_today      = 0_i64;
        let mut completed_today  = 0_i64;
        let mut escalated_today  = 0_i64;
        let mut failed_today     = 0_i64;
        let mut by_type_today: std::collections::HashMap<String, i64> = Default::default();

        for r in &count_rows {
            let status: String = r.get("status");
            let agent_type: String = r.get("agent_type");
            let n: i64 = r.get("n");
            total_today += n;
            *by_type_today.entry(agent_type).or_insert(0) += n;
            match status.as_str() {
                "completed"        => completed_today += n,
                "human_escalated"  => escalated_today += n,
                "failed"           => failed_today    += n,
                _                  => {} // running
            }
        }

        let success_rate_pct = if total_today > 0 {
            completed_today as f64 / total_today as f64 * 100.0
        } else { 0.0 };

        // 24-hour rolling buckets. EXTRACT(hour FROM started_at) bins by
        // wall-clock hour; the cutoff `started_at >= NOW() - INTERVAL '24 hours'`
        // keeps the window genuinely rolling rather than "since midnight".
        let bucket_rows = sqlx::query(
            r#"
            SELECT
                EXTRACT(HOUR FROM started_at)::INT AS hour,
                agent_type,
                COUNT(*)::BIGINT                    AS n
            FROM ai.agent_sessions
            WHERE tenant_id = $1
              AND started_at >= NOW() - INTERVAL '24 hours'
            GROUP BY hour, agent_type
            ORDER BY hour ASC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;

        // Build a 24-slot vec so the chart always renders a full day even
        // when some hours have zero activity.
        let mut hourly: Vec<HourlyBucket> = (0..24)
            .map(|h| HourlyBucket { hour: h, total: 0, by_type: Default::default() })
            .collect();
        for r in &bucket_rows {
            let hour: i32 = r.get("hour");
            let agent_type: String = r.get("agent_type");
            let n: i64 = r.get("n");
            if let Some(slot) = hourly.get_mut(hour as usize) {
                slot.total += n;
                *slot.by_type.entry(agent_type).or_insert(0) += n;
            }
        }

        Ok(AggregateStats {
            total_today,
            completed_today,
            escalated_today,
            failed_today,
            success_rate_pct,
            by_type_today,
            hourly_24h: hourly,
        })
    }
}
