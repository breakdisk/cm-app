//! Postgres `SessionStore` for mesh runs.
//!
//! OmniDeliv persists its own agent sessions rather than writing into
//! `ai.agent_sessions`: each service owns its schema (ADR-0012), and a product
//! tier writing another service's tables is the coupling ADR-0009 rule 2 exists
//! to prevent. The row shape mirrors ai-layer's so the two can be read together
//! later without reconciling two models.

use async_trait::async_trait;
use logisticos_agent_runtime::{store::SessionStore, AgentRole, AgentSession, SessionStatus};
use logisticos_types::TenantId;
use sqlx::{PgPool, Row};
use uuid::Uuid;

pub struct PgMeshSessionStore { pool: PgPool }

impl PgMeshSessionStore {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

fn status_str(s: &SessionStatus) -> &'static str {
    match s {
        SessionStatus::Running        => "running",
        SessionStatus::Completed      => "completed",
        SessionStatus::Failed         => "failed",
        SessionStatus::HumanEscalated => "human_escalated",
    }
}

fn parse_status(s: &str) -> SessionStatus {
    match s {
        "completed"       => SessionStatus::Completed,
        "failed"          => SessionStatus::Failed,
        "human_escalated" => SessionStatus::HumanEscalated,
        _                 => SessionStatus::Running,
    }
}

/// Rebuild the role from its persisted key.
///
/// The allowlist is restored from the role constructors, not from the database:
/// a role reconstructed without its allowlist would be silently unrestricted,
/// so a resumed session would hold authority the original never had. An unknown
/// key fails closed with an empty allowlist rather than open with a full one.
fn role_for(key: &str) -> AgentRole {
    match key {
        omnideliv_mesh::roles::CONCIERGE_KEY    => omnideliv_mesh::roles::concierge(),
        omnideliv_mesh::roles::NUTRITIONIST_KEY => omnideliv_mesh::roles::nutritionist(),
        omnideliv_mesh::roles::FLEET_KEY        => omnideliv_mesh::roles::fleet(),
        other => AgentRole::restricted(other, other, "", std::iter::empty::<&str>()),
    }
}

#[async_trait]
impl SessionStore for PgMeshSessionStore {
    async fn save(&self, s: &AgentSession) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO omnideliv.agent_sessions (
                id, tenant_id, role_key, status, trigger_data, messages, actions,
                outcome, escalation_reason, confidence_score, model_used,
                parent_session_id, started_at, completed_at
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
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
        .bind(s.role.key())
        .bind(status_str(&s.status))
        .bind(&s.trigger)
        .bind(serde_json::to_value(&s.messages)?)
        .bind(serde_json::to_value(&s.actions)?)
        .bind(&s.outcome)
        .bind(&s.escalation_reason)
        .bind(s.confidence_score)
        .bind(&s.model_used)
        .bind(s.parent_session_id)
        .bind(s.started_at)
        .bind(s.completed_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<AgentSession>> {
        let Some(r) = sqlx::query("SELECT * FROM omnideliv.agent_sessions WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
        else {
            return Ok(None);
        };

        let role_key: String = r.get("role_key");
        let status: String = r.get("status");

        Ok(Some(AgentSession {
            id:                r.get("id"),
            tenant_id:         TenantId::from_uuid(r.get("tenant_id")),
            role:              role_for(&role_key),
            status:            parse_status(&status),
            trigger:           r.get("trigger_data"),
            messages:          serde_json::from_value(r.get("messages")).unwrap_or_default(),
            actions:           serde_json::from_value(r.get("actions")).unwrap_or_default(),
            outcome:           r.get("outcome"),
            escalation_reason: r.get("escalation_reason"),
            confidence_score:  r.get("confidence_score"),
            model_used:        r.get("model_used"),
            parent_session_id: r.get("parent_session_id"),
            started_at:        r.get("started_at"),
            completed_at:      r.get("completed_at"),
        }))
    }
}
