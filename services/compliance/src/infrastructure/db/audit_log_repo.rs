use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;
use crate::domain::{entities::ComplianceAuditLog, repositories::AuditLogRepository};

#[derive(sqlx::FromRow)]
struct AuditLogRow {
    id:                    Uuid,
    tenant_id:             Uuid,
    compliance_profile_id: Uuid,
    document_id:           Option<Uuid>,
    event_type:            String,
    actor_id:              Uuid,
    actor_type:            String,
    notes:                 Option<String>,
    created_at:            chrono::DateTime<chrono::Utc>,
}

impl TryFrom<AuditLogRow> for ComplianceAuditLog {
    type Error = anyhow::Error;

    fn try_from(r: AuditLogRow) -> anyhow::Result<Self> {
        Ok(Self {
            id:                    r.id,
            tenant_id:             r.tenant_id,
            compliance_profile_id: r.compliance_profile_id,
            document_id:           r.document_id,
            event_type:            r.event_type,
            actor_id:              r.actor_id,
            actor_type:            r.actor_type,
            notes:                 r.notes,
            created_at:            r.created_at,
        })
    }
}

pub struct PgAuditLogRepository { pool: PgPool }

impl PgAuditLogRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl AuditLogRepository for PgAuditLogRepository {
    async fn append(&self, entry: &ComplianceAuditLog) -> anyhow::Result<()> {
        sqlx::query!(
            r#"INSERT INTO compliance.compliance_audit_log
               (id, tenant_id, compliance_profile_id, document_id, event_type, actor_id, actor_type, notes, created_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)"#,
            entry.id, entry.tenant_id, entry.compliance_profile_id, entry.document_id,
            &entry.event_type, entry.actor_id, &entry.actor_type,
            entry.notes, entry.created_at
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_by_profile(
        &self,
        profile_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<ComplianceAuditLog>> {
        let rows = sqlx::query_as!(
            AuditLogRow,
            r#"SELECT id, tenant_id, compliance_profile_id, document_id, event_type,
                      actor_id, actor_type, notes, created_at
               FROM compliance.compliance_audit_log
               WHERE compliance_profile_id = $1
               ORDER BY created_at DESC
               LIMIT $2 OFFSET $3"#,
            profile_id, limit, offset
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(ComplianceAuditLog::try_from)
            .collect::<anyhow::Result<Vec<_>>>()
    }
}
