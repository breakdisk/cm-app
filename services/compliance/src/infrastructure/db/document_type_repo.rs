use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;
use crate::domain::{entities::DocumentType, repositories::DocumentTypeRepository};

#[derive(sqlx::FromRow)]
struct DocumentTypeRow {
    id:                Uuid,
    code:              String,
    jurisdiction:      String,
    applicable_to:     Vec<String>,
    name:              String,
    description:       Option<String>,
    is_required:       bool,
    has_expiry:        bool,
    warn_days_before:  i32,
    grace_period_days: i32,
    vehicle_classes:   Option<Vec<String>>,
}

impl From<DocumentTypeRow> for DocumentType {
    fn from(r: DocumentTypeRow) -> Self {
        Self {
            id: r.id, code: r.code, jurisdiction: r.jurisdiction,
            applicable_to: r.applicable_to, name: r.name,
            description: r.description, is_required: r.is_required,
            has_expiry: r.has_expiry, warn_days_before: r.warn_days_before,
            grace_period_days: r.grace_period_days, vehicle_classes: r.vehicle_classes,
        }
    }
}

pub struct PgDocumentTypeRepository { pool: PgPool }

impl PgDocumentTypeRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl DocumentTypeRepository for PgDocumentTypeRepository {
    async fn find_by_code(&self, code: &str) -> anyhow::Result<Option<DocumentType>> {
        let row = sqlx::query_as!(
            DocumentTypeRow,
            r#"SELECT id, code, jurisdiction, applicable_to, name, description,
                      is_required, has_expiry, warn_days_before, grace_period_days, vehicle_classes
               FROM compliance.document_types WHERE code = $1"#,
            code
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn list_required_for(&self, entity_type: &str, jurisdiction: &str)
        -> anyhow::Result<Vec<DocumentType>>
    {
        let rows = sqlx::query_as!(
            DocumentTypeRow,
            r#"SELECT id, code, jurisdiction, applicable_to, name, description,
                      is_required, has_expiry, warn_days_before, grace_period_days, vehicle_classes
               FROM compliance.document_types
               WHERE is_required = true
                 AND jurisdiction = $1
                 AND $2 = ANY(applicable_to)
               ORDER BY name"#,
            jurisdiction, entity_type
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<DocumentType>> {
        let row = sqlx::query_as!(
            DocumentTypeRow,
            r#"SELECT id, code, jurisdiction, applicable_to, name, description,
                      is_required, has_expiry, warn_days_before, grace_period_days, vehicle_classes
               FROM compliance.document_types WHERE id = $1"#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }
}
