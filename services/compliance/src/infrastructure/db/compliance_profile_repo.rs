use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;
use crate::domain::{
    entities::{ComplianceProfile, ComplianceStatus},
    repositories::ComplianceProfileRepository,
};

#[derive(sqlx::FromRow)]
struct ComplianceProfileRow {
    id:               Uuid,
    tenant_id:        Uuid,
    entity_type:      String,
    entity_id:        Uuid,
    overall_status:   String,
    jurisdiction:     String,
    last_reviewed_at: Option<chrono::DateTime<chrono::Utc>>,
    reviewed_by:      Option<Uuid>,
    suspended_at:     Option<chrono::DateTime<chrono::Utc>>,
    created_at:       chrono::DateTime<chrono::Utc>,
    updated_at:       chrono::DateTime<chrono::Utc>,
}

impl TryFrom<ComplianceProfileRow> for ComplianceProfile {
    type Error = anyhow::Error;

    fn try_from(r: ComplianceProfileRow) -> anyhow::Result<Self> {
        Ok(Self {
            id:               r.id,
            tenant_id:        r.tenant_id,
            entity_type:      r.entity_type,
            entity_id:        r.entity_id,
            overall_status:   ComplianceStatus::from_str(&r.overall_status)?,
            jurisdiction:     r.jurisdiction,
            last_reviewed_at: r.last_reviewed_at,
            reviewed_by:      r.reviewed_by,
            suspended_at:     r.suspended_at,
            created_at:       r.created_at,
            updated_at:       r.updated_at,
        })
    }
}

pub struct PgComplianceProfileRepository { pool: PgPool }

impl PgComplianceProfileRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
    /// Exposes pool for health check.
    pub fn pool(&self) -> &PgPool { &self.pool }
}

#[async_trait]
impl ComplianceProfileRepository for PgComplianceProfileRepository {
    async fn find_by_entity(&self, tenant_id: Uuid, entity_type: &str, entity_id: Uuid)
        -> anyhow::Result<Option<ComplianceProfile>>
    {
        let row = sqlx::query_as!(
            ComplianceProfileRow,
            r#"SELECT id, tenant_id, entity_type, entity_id, overall_status, jurisdiction,
                      last_reviewed_at, reviewed_by, suspended_at, created_at, updated_at
               FROM compliance.compliance_profiles
               WHERE tenant_id = $1 AND entity_type = $2 AND entity_id = $3"#,
            tenant_id, entity_type, entity_id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(ComplianceProfile::try_from).transpose()?)
    }

    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<ComplianceProfile>> {
        let row = sqlx::query_as!(
            ComplianceProfileRow,
            r#"SELECT id, tenant_id, entity_type, entity_id, overall_status, jurisdiction,
                      last_reviewed_at, reviewed_by, suspended_at, created_at, updated_at
               FROM compliance.compliance_profiles WHERE id = $1"#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(ComplianceProfile::try_from).transpose()?)
    }

    async fn list_by_tenant(
        &self,
        tenant_id: Uuid,
        status_filter: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<ComplianceProfile>>
    {
        let rows = sqlx::query_as!(
            ComplianceProfileRow,
            r#"SELECT id, tenant_id, entity_type, entity_id, overall_status, jurisdiction,
                      last_reviewed_at, reviewed_by, suspended_at, created_at, updated_at
               FROM compliance.compliance_profiles
               WHERE tenant_id = $1
                 AND ($2::text IS NULL OR overall_status = $2)
               ORDER BY created_at DESC
               LIMIT $3 OFFSET $4"#,
            tenant_id, status_filter, limit, offset
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(ComplianceProfile::try_from)
            .collect::<anyhow::Result<Vec<_>>>()
    }

    async fn save(&self, p: &ComplianceProfile) -> anyhow::Result<()> {
        sqlx::query!(
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
            p.id, p.tenant_id, &p.entity_type, p.entity_id,
            p.overall_status.as_str(), &p.jurisdiction,
            p.last_reviewed_at, p.reviewed_by, p.suspended_at,
            p.created_at, p.updated_at
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
