use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;
use crate::domain::{
    entities::{ComplianceProfile, ComplianceStatus},
    repositories::ComplianceProfileRepository,
};

fn map_profile(r: &sqlx::postgres::PgRow) -> anyhow::Result<ComplianceProfile> {
    let id: Uuid               = r.get("id");
    let tenant_id: Uuid        = r.get("tenant_id");
    let entity_type: String    = r.get("entity_type");
    let entity_id: Uuid        = r.get("entity_id");
    let overall_status: String = r.get("overall_status");
    let jurisdiction: String   = r.get("jurisdiction");
    let last_reviewed_at: Option<chrono::DateTime<chrono::Utc>> = r.get("last_reviewed_at");
    let reviewed_by: Option<Uuid>   = r.get("reviewed_by");
    let suspended_at: Option<chrono::DateTime<chrono::Utc>> = r.get("suspended_at");
    let created_at: chrono::DateTime<chrono::Utc> = r.get("created_at");
    let updated_at: chrono::DateTime<chrono::Utc> = r.get("updated_at");
    Ok(ComplianceProfile {
        id, tenant_id, entity_type, entity_id,
        overall_status: ComplianceStatus::from_str(&overall_status)?,
        jurisdiction, last_reviewed_at, reviewed_by, suspended_at, created_at, updated_at,
    })
}

pub struct PgComplianceProfileRepository { pool: PgPool }

impl PgComplianceProfileRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
    pub fn pool(&self) -> &PgPool { &self.pool }
}

#[async_trait]
impl ComplianceProfileRepository for PgComplianceProfileRepository {
    async fn find_by_entity(&self, tenant_id: Uuid, entity_type: &str, entity_id: Uuid)
        -> anyhow::Result<Option<ComplianceProfile>>
    {
        let row = sqlx::query(
            r#"SELECT id, tenant_id, entity_type, entity_id, overall_status, jurisdiction,
                      last_reviewed_at, reviewed_by, suspended_at, created_at, updated_at
               FROM compliance.compliance_profiles
               WHERE tenant_id = $1 AND entity_type = $2 AND entity_id = $3"#,
        )
        .bind(tenant_id)
        .bind(entity_type)
        .bind(entity_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| map_profile(&r)).transpose()
    }

    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<ComplianceProfile>> {
        let row = sqlx::query(
            r#"SELECT id, tenant_id, entity_type, entity_id, overall_status, jurisdiction,
                      last_reviewed_at, reviewed_by, suspended_at, created_at, updated_at
               FROM compliance.compliance_profiles WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| map_profile(&r)).transpose()
    }

    async fn list_by_tenant(
        &self,
        tenant_id: Uuid,
        status_filter: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<ComplianceProfile>>
    {
        let rows = sqlx::query(
            r#"SELECT id, tenant_id, entity_type, entity_id, overall_status, jurisdiction,
                      last_reviewed_at, reviewed_by, suspended_at, created_at, updated_at
               FROM compliance.compliance_profiles
               WHERE tenant_id = $1
                 AND ($2::text IS NULL OR overall_status = $2)
               ORDER BY created_at DESC
               LIMIT $3 OFFSET $4"#,
        )
        .bind(tenant_id)
        .bind(status_filter)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(map_profile).collect()
    }

    async fn save(&self, p: &ComplianceProfile) -> anyhow::Result<()> {
        sqlx::query(
            r#"INSERT INTO compliance.compliance_profiles
               (id, tenant_id, entity_type, entity_id, overall_status, jurisdiction,
                last_reviewed_at, reviewed_by, suspended_at, created_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
               ON CONFLICT (id) DO UPDATE SET
                 overall_status   = EXCLUDED.overall_status,
                 last_reviewed_at = EXCLUDED.last_reviewed_at,
                 reviewed_by      = EXCLUDED.reviewed_by,
                 suspended_at     = EXCLUDED.suspended_at,
                 updated_at       = EXCLUDED.updated_at"#,
        )
        .bind(p.id)
        .bind(p.tenant_id)
        .bind(&p.entity_type)
        .bind(p.entity_id)
        .bind(p.overall_status.as_str())
        .bind(&p.jurisdiction)
        .bind(p.last_reviewed_at)
        .bind(p.reviewed_by)
        .bind(p.suspended_at)
        .bind(p.created_at)
        .bind(p.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
