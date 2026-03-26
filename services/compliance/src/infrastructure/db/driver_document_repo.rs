use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;
use crate::domain::{
    entities::{DriverDocument, DocumentStatus},
    repositories::DriverDocumentRepository,
};

#[derive(sqlx::FromRow)]
struct DriverDocumentRow {
    id:                    Uuid,
    compliance_profile_id: Uuid,
    document_type_id:      Uuid,
    document_number:       String,
    issue_date:            Option<chrono::NaiveDate>,
    expiry_date:           Option<chrono::NaiveDate>,
    file_url:              String,
    status:                String,
    rejection_reason:      Option<String>,
    reviewed_by:           Option<Uuid>,
    reviewed_at:           Option<chrono::DateTime<chrono::Utc>>,
    submitted_at:          chrono::DateTime<chrono::Utc>,
    updated_at:            chrono::DateTime<chrono::Utc>,
}

impl TryFrom<DriverDocumentRow> for DriverDocument {
    type Error = anyhow::Error;

    fn try_from(r: DriverDocumentRow) -> anyhow::Result<Self> {
        Ok(Self {
            id: r.id,
            compliance_profile_id: r.compliance_profile_id,
            document_type_id: r.document_type_id,
            document_number: r.document_number,
            issue_date: r.issue_date,
            expiry_date: r.expiry_date,
            file_url: r.file_url,
            status: DocumentStatus::from_str(&r.status)?,
            rejection_reason: r.rejection_reason,
            reviewed_by: r.reviewed_by,
            reviewed_at: r.reviewed_at,
            submitted_at: r.submitted_at,
            updated_at: r.updated_at,
        })
    }
}

pub struct PgDriverDocumentRepository { pool: PgPool }

impl PgDriverDocumentRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl DriverDocumentRepository for PgDriverDocumentRepository {
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<DriverDocument>> {
        let row = sqlx::query_as!(
            DriverDocumentRow,
            r#"SELECT id, compliance_profile_id, document_type_id, document_number,
                      issue_date, expiry_date, file_url, status, rejection_reason,
                      reviewed_by, reviewed_at, submitted_at, updated_at
               FROM compliance.driver_documents WHERE id = $1"#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(DriverDocument::try_from).transpose()?)
    }

    async fn list_by_profile(&self, profile_id: Uuid) -> anyhow::Result<Vec<DriverDocument>> {
        let rows = sqlx::query_as!(
            DriverDocumentRow,
            r#"SELECT id, compliance_profile_id, document_type_id, document_number,
                      issue_date, expiry_date, file_url, status, rejection_reason,
                      reviewed_by, reviewed_at, submitted_at, updated_at
               FROM compliance.driver_documents
               WHERE compliance_profile_id = $1
               ORDER BY submitted_at DESC"#,
            profile_id
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(DriverDocument::try_from)
            .collect::<anyhow::Result<Vec<_>>>()
    }

    async fn find_expiring(&self, within_days: i32) -> anyhow::Result<Vec<DriverDocument>> {
        let rows = sqlx::query_as!(
            DriverDocumentRow,
            r#"SELECT id, compliance_profile_id, document_type_id, document_number,
                      issue_date, expiry_date, file_url, status, rejection_reason,
                      reviewed_by, reviewed_at, submitted_at, updated_at
               FROM compliance.driver_documents
               WHERE status = 'approved'
                 AND expiry_date IS NOT NULL
                 AND expiry_date - CURRENT_DATE <= $1
                 AND expiry_date >= CURRENT_DATE"#,
            within_days
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(DriverDocument::try_from)
            .collect::<anyhow::Result<Vec<_>>>()
    }

    async fn find_expired(&self) -> anyhow::Result<Vec<DriverDocument>> {
        let rows = sqlx::query_as!(
            DriverDocumentRow,
            r#"SELECT id, compliance_profile_id, document_type_id, document_number,
                      issue_date, expiry_date, file_url, status, rejection_reason,
                      reviewed_by, reviewed_at, submitted_at, updated_at
               FROM compliance.driver_documents
               WHERE status = 'approved'
                 AND expiry_date IS NOT NULL
                 AND expiry_date < CURRENT_DATE"#
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(DriverDocument::try_from)
            .collect::<anyhow::Result<Vec<_>>>()
    }

    async fn list_pending_review(&self, tenant_id: Option<Uuid>, limit: i64, offset: i64)
        -> anyhow::Result<Vec<DriverDocument>>
    {
        let rows = sqlx::query_as!(
            DriverDocumentRow,
            r#"SELECT d.id, d.compliance_profile_id, d.document_type_id, d.document_number,
                      d.issue_date, d.expiry_date, d.file_url, d.status, d.rejection_reason,
                      d.reviewed_by, d.reviewed_at, d.submitted_at, d.updated_at
               FROM compliance.driver_documents d
               JOIN compliance.compliance_profiles p ON p.id = d.compliance_profile_id
               WHERE d.status IN ('submitted', 'under_review')
                 AND ($1::uuid IS NULL OR p.tenant_id = $1)
               ORDER BY d.submitted_at ASC
               LIMIT $2 OFFSET $3"#,
            tenant_id, limit, offset
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(DriverDocument::try_from)
            .collect::<anyhow::Result<Vec<_>>>()
    }

    async fn save(&self, doc: &DriverDocument) -> anyhow::Result<()> {
        sqlx::query!(
            r#"INSERT INTO compliance.driver_documents
               (id, compliance_profile_id, document_type_id, document_number,
                issue_date, expiry_date, file_url, status, rejection_reason,
                reviewed_by, reviewed_at, submitted_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
               ON CONFLICT (id) DO UPDATE SET
                 status           = EXCLUDED.status,
                 rejection_reason = EXCLUDED.rejection_reason,
                 reviewed_by      = EXCLUDED.reviewed_by,
                 reviewed_at      = EXCLUDED.reviewed_at,
                 updated_at       = EXCLUDED.updated_at"#,
            doc.id, doc.compliance_profile_id, doc.document_type_id, &doc.document_number,
            doc.issue_date, doc.expiry_date, &doc.file_url, doc.status.as_str(),
            doc.rejection_reason, doc.reviewed_by, doc.reviewed_at,
            doc.submitted_at, doc.updated_at
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
