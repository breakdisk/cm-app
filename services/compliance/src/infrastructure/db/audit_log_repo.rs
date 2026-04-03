use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;
use crate::domain::{entities::ComplianceAuditLog, repositories::AuditLogRepository};

fn map_log(r: &sqlx::postgres::PgRow) -> ComplianceAuditLog {
    ComplianceAuditLog {
        id:                    r.get("id"),
        tenant_id:             r.get("tenant_id"),
        compliance_profile_id: r.get("compliance_profile_id"),
        document_id:           r.get("document_id"),
        event_type:            r.get("event_type"),
        actor_id:              r.get("actor_id"),
        actor_type:            r.get("actor_type"),
        notes:                 r.get("notes"),
        created_at:            r.get("created_at"),
    }
}

pub struct PgAuditLogRepository { pool: PgPool }

impl PgAuditLogRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl AuditLogRepository for PgAuditLogRepository {
    async fn append(&self, entry: &ComplianceAuditLog) -> anyhow::Result<()> {
        sqlx::query(
            r#"INSERT INTO compliance.compliance_audit_log
               (id, tenant_id, compliance_profile_id, document_id, event_type, actor_id, actor_type, notes, created_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)"#,
        )
        .bind(entry.id)
        .bind(entry.tenant_id)
        .bind(entry.compliance_profile_id)
        .bind(entry.document_id)
        .bind(&entry.event_type)
        .bind(entry.actor_id)
        .bind(&entry.actor_type)
        .bind(entry.notes.as_deref())
        .bind(entry.created_at)
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
        let rows = sqlx::query(
            r#"SELECT id, tenant_id, compliance_profile_id, document_id, event_type,
                      actor_id, actor_type, notes, created_at
               FROM compliance.compliance_audit_log
               WHERE compliance_profile_id = $1
               ORDER BY created_at DESC
               LIMIT $2 OFFSET $3"#,
        )
        .bind(profile_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(map_log).collect())
    }
}
