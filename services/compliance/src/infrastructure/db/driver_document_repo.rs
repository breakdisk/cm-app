use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;
use crate::domain::{
    entities::{DriverDocument, DocumentStatus},
    repositories::DriverDocumentRepository,
};

fn map_doc(r: &sqlx::postgres::PgRow) -> anyhow::Result<DriverDocument> {
    let id: Uuid                    = r.get("id");
    let compliance_profile_id: Uuid = r.get("compliance_profile_id");
    let document_type_id: Uuid      = r.get("document_type_id");
    let document_number: String     = r.get("document_number");
    let issue_date: Option<chrono::NaiveDate>  = r.get("issue_date");
    let expiry_date: Option<chrono::NaiveDate> = r.get("expiry_date");
    let file_url: String            = r.get("file_url");
    let status: String              = r.get("status");
    let rejection_reason: Option<String>  = r.get("rejection_reason");
    let reviewed_by: Option<Uuid>         = r.get("reviewed_by");
    let reviewed_at: Option<chrono::DateTime<chrono::Utc>> = r.get("reviewed_at");
    let submitted_at: chrono::DateTime<chrono::Utc> = r.get("submitted_at");
    let updated_at: chrono::DateTime<chrono::Utc>   = r.get("updated_at");
    Ok(DriverDocument {
        id, compliance_profile_id, document_type_id, document_number,
        issue_date, expiry_date, file_url,
        status: DocumentStatus::from_str(&status)?,
        rejection_reason, reviewed_by, reviewed_at, submitted_at, updated_at,
    })
}

pub struct PgDriverDocumentRepository { pool: PgPool }

impl PgDriverDocumentRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl DriverDocumentRepository for PgDriverDocumentRepository {
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<DriverDocument>> {
        let row = sqlx::query(
            r#"SELECT id, compliance_profile_id, document_type_id, document_number,
                      issue_date, expiry_date, file_url, status, rejection_reason,
                      reviewed_by, reviewed_at, submitted_at, updated_at
               FROM compliance.driver_documents WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| map_doc(&r)).transpose()
    }

    async fn list_by_profile(&self, profile_id: Uuid) -> anyhow::Result<Vec<DriverDocument>> {
        let rows = sqlx::query(
            r#"SELECT id, compliance_profile_id, document_type_id, document_number,
                      issue_date, expiry_date, file_url, status, rejection_reason,
                      reviewed_by, reviewed_at, submitted_at, updated_at
               FROM compliance.driver_documents
               WHERE compliance_profile_id = $1
               ORDER BY submitted_at DESC"#,
        )
        .bind(profile_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(map_doc).collect()
    }

    async fn find_expiring(&self, within_days: i32) -> anyhow::Result<Vec<DriverDocument>> {
        let rows = sqlx::query(
            r#"SELECT id, compliance_profile_id, document_type_id, document_number,
                      issue_date, expiry_date, file_url, status, rejection_reason,
                      reviewed_by, reviewed_at, submitted_at, updated_at
               FROM compliance.driver_documents
               WHERE status = 'approved'
                 AND expiry_date IS NOT NULL
                 AND expiry_date - CURRENT_DATE <= $1
                 AND expiry_date >= CURRENT_DATE"#,
        )
        .bind(within_days)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(map_doc).collect()
    }

    async fn find_expired(&self) -> anyhow::Result<Vec<DriverDocument>> {
        let rows = sqlx::query(
            r#"SELECT id, compliance_profile_id, document_type_id, document_number,
                      issue_date, expiry_date, file_url, status, rejection_reason,
                      reviewed_by, reviewed_at, submitted_at, updated_at
               FROM compliance.driver_documents
               WHERE status = 'approved'
                 AND expiry_date IS NOT NULL
                 AND expiry_date < CURRENT_DATE"#,
        )
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(map_doc).collect()
    }

    async fn list_pending_review(&self, tenant_id: Option<Uuid>, limit: i64, offset: i64)
        -> anyhow::Result<Vec<DriverDocument>>
    {
        let rows = sqlx::query(
            r#"SELECT d.id, d.compliance_profile_id, d.document_type_id, d.document_number,
                      d.issue_date, d.expiry_date, d.file_url, d.status, d.rejection_reason,
                      d.reviewed_by, d.reviewed_at, d.submitted_at, d.updated_at
               FROM compliance.driver_documents d
               JOIN compliance.compliance_profiles p ON p.id = d.compliance_profile_id
               WHERE d.status IN ('submitted', 'under_review')
                 AND ($1::uuid IS NULL OR p.tenant_id = $1)
               ORDER BY d.submitted_at ASC
               LIMIT $2 OFFSET $3"#,
        )
        .bind(tenant_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(map_doc).collect()
    }

    async fn save(&self, doc: &DriverDocument) -> anyhow::Result<()> {
        sqlx::query(
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
        )
        .bind(doc.id)
        .bind(doc.compliance_profile_id)
        .bind(doc.document_type_id)
        .bind(&doc.document_number)
        .bind(doc.issue_date)
        .bind(doc.expiry_date)
        .bind(&doc.file_url)
        .bind(doc.status.as_str())
        .bind(doc.rejection_reason.as_deref())
        .bind(doc.reviewed_by)
        .bind(doc.reviewed_at)
        .bind(doc.submitted_at)
        .bind(doc.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
