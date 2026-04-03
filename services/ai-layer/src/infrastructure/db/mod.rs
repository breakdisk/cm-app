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
}
