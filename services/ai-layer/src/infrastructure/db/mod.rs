/// Agent session persistence — immutable audit log of all agent runs.
use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::entities::{AgentSession, AgentType, SessionStatus};

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

#[async_trait]
impl SessionRepository for PgSessionRepository {
    async fn save(&self, s: &AgentSession) -> anyhow::Result<()> {
        let agent_type = serde_json::to_value(&s.agent_type)?.as_str().unwrap_or("on_demand").to_owned();
        let status     = serde_json::to_value(&s.status)?.as_str().unwrap_or("running").to_owned();
        let messages   = serde_json::to_value(&s.messages)?;
        let actions    = serde_json::to_value(&s.actions)?;

        sqlx::query!(
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
            s.id,
            s.tenant_id.inner(),
            agent_type,
            status,
            s.trigger,
            messages,
            actions,
            s.outcome,
            s.escalation_reason,
            s.confidence_score,
            s.model_used,
            s.started_at,
            s.completed_at,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<AgentSession>> {
        let row = sqlx::query!(
            r#"
            SELECT id, tenant_id, agent_type, status, trigger_data AS "trigger_data: serde_json::Value",
                   messages AS "messages: serde_json::Value", actions AS "actions: serde_json::Value",
                   outcome, escalation_reason, confidence_score, model_used,
                   started_at, completed_at
            FROM ai.agent_sessions WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| AgentSession {
            id:               r.id,
            tenant_id:        logisticos_types::TenantId::from_uuid(r.tenant_id),
            agent_type:       serde_json::from_value(serde_json::Value::String(r.agent_type)).unwrap_or(AgentType::OnDemand),
            status:           serde_json::from_value(serde_json::Value::String(r.status)).unwrap_or(SessionStatus::Failed),
            trigger:          r.trigger_data,
            messages:         serde_json::from_value(r.messages).unwrap_or_default(),
            actions:          serde_json::from_value(r.actions).unwrap_or_default(),
            outcome:          r.outcome,
            escalation_reason: r.escalation_reason,
            confidence_score: r.confidence_score,
            model_used:       r.model_used,
            started_at:       r.started_at,
            completed_at:     r.completed_at,
        }))
    }

    async fn list_by_tenant(&self, tenant_id: Uuid, limit: i64, offset: i64) -> anyhow::Result<Vec<AgentSession>> {
        // Simplified: return basic session summaries without full message history for list views.
        let rows = sqlx::query!(
            r#"
            SELECT id, tenant_id, agent_type, status, trigger_data AS "trigger_data: serde_json::Value",
                   messages AS "messages: serde_json::Value", actions AS "actions: serde_json::Value",
                   outcome, escalation_reason, confidence_score, model_used,
                   started_at, completed_at
            FROM ai.agent_sessions
            WHERE tenant_id = $1
            ORDER BY started_at DESC
            LIMIT $2 OFFSET $3
            "#,
            tenant_id, limit, offset
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| AgentSession {
            id:               r.id,
            tenant_id:        logisticos_types::TenantId::from_uuid(r.tenant_id),
            agent_type:       serde_json::from_value(serde_json::Value::String(r.agent_type)).unwrap_or(AgentType::OnDemand),
            status:           serde_json::from_value(serde_json::Value::String(r.status)).unwrap_or(SessionStatus::Failed),
            trigger:          r.trigger_data,
            messages:         serde_json::from_value(r.messages).unwrap_or_default(),
            actions:          serde_json::from_value(r.actions).unwrap_or_default(),
            outcome:          r.outcome,
            escalation_reason: r.escalation_reason,
            confidence_score: r.confidence_score,
            model_used:       r.model_used,
            started_at:       r.started_at,
            completed_at:     r.completed_at,
        }).collect())
    }

    async fn list_escalated(&self, tenant_id: Uuid) -> anyhow::Result<Vec<AgentSession>> {
        let rows = sqlx::query!(
            r#"
            SELECT id, tenant_id, agent_type, status, trigger_data AS "trigger_data: serde_json::Value",
                   messages AS "messages: serde_json::Value", actions AS "actions: serde_json::Value",
                   outcome, escalation_reason, confidence_score, model_used,
                   started_at, completed_at
            FROM ai.agent_sessions
            WHERE tenant_id = $1 AND status = 'human_escalated'
            ORDER BY started_at DESC
            "#,
            tenant_id
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| AgentSession {
            id:               r.id,
            tenant_id:        logisticos_types::TenantId::from_uuid(r.tenant_id),
            agent_type:       serde_json::from_value(serde_json::Value::String(r.agent_type)).unwrap_or(AgentType::OnDemand),
            status:           serde_json::from_value(serde_json::Value::String(r.status)).unwrap_or(SessionStatus::Failed),
            trigger:          r.trigger_data,
            messages:         serde_json::from_value(r.messages).unwrap_or_default(),
            actions:          serde_json::from_value(r.actions).unwrap_or_default(),
            outcome:          r.outcome,
            escalation_reason: r.escalation_reason,
            confidence_score: r.confidence_score,
            model_used:       r.model_used,
            started_at:       r.started_at,
            completed_at:     r.completed_at,
        }).collect())
    }
}
