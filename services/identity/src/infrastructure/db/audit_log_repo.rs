use sqlx::{PgPool, Row};
use uuid::Uuid;

pub struct NewAuditEntry {
    pub tenant_id:   Uuid,
    pub actor_id:    Uuid,
    pub actor_email: String,
    pub action:      String,
    pub resource:    String,
}

#[derive(Debug, serde::Serialize)]
pub struct AuditEntry {
    pub id:          Uuid,
    pub tenant_id:   Uuid,
    pub actor_id:    Option<Uuid>,
    pub actor_email: Option<String>,
    pub action:      String,
    pub resource:    String,
    pub ip:          Option<String>,
    pub created_at:  chrono::DateTime<chrono::Utc>,
}

#[derive(Clone)]
pub struct PgAuditLogRepository {
    pool: PgPool,
}

impl PgAuditLogRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }

    pub async fn append(&self, entry: &NewAuditEntry) -> anyhow::Result<()> {
        sqlx::query(
            r#"INSERT INTO identity.tenant_audit_log
               (tenant_id, actor_id, actor_email, action, resource)
               VALUES ($1, $2, $3, $4, $5)"#,
        )
        .bind(entry.tenant_id)
        .bind(entry.actor_id)
        .bind(&entry.actor_email)
        .bind(&entry.action)
        .bind(&entry.resource)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_by_tenant(
        &self,
        tenant_id: Uuid,
        limit: i64,
    ) -> anyhow::Result<Vec<AuditEntry>> {
        let rows = sqlx::query(
            r#"SELECT id, tenant_id, actor_id, actor_email, action, resource, ip, created_at
               FROM identity.tenant_audit_log
               WHERE tenant_id = $1
               ORDER BY created_at DESC
               LIMIT $2"#,
        )
        .bind(tenant_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(|r| AuditEntry {
            id:          r.get("id"),
            tenant_id:   r.get("tenant_id"),
            actor_id:    r.get("actor_id"),
            actor_email: r.get("actor_email"),
            action:      r.get("action"),
            resource:    r.get("resource"),
            ip:          r.get("ip"),
            created_at:  r.get("created_at"),
        }).collect())
    }
}
