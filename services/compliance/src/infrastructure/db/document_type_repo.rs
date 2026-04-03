use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;
use crate::domain::{entities::DocumentType, repositories::DocumentTypeRepository};

fn map_doc_type(r: &sqlx::postgres::PgRow) -> DocumentType {
    DocumentType {
        id:                r.get("id"),
        code:              r.get("code"),
        jurisdiction:      r.get("jurisdiction"),
        applicable_to:     r.get("applicable_to"),
        name:              r.get("name"),
        description:       r.get("description"),
        is_required:       r.get("is_required"),
        has_expiry:        r.get("has_expiry"),
        warn_days_before:  r.get("warn_days_before"),
        grace_period_days: r.get("grace_period_days"),
        vehicle_classes:   r.get("vehicle_classes"),
    }
}

pub struct PgDocumentTypeRepository { pool: PgPool }

impl PgDocumentTypeRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl DocumentTypeRepository for PgDocumentTypeRepository {
    async fn find_by_code(&self, code: &str) -> anyhow::Result<Option<DocumentType>> {
        let row = sqlx::query(
            r#"SELECT id, code, jurisdiction, applicable_to, name, description,
                      is_required, has_expiry, warn_days_before, grace_period_days, vehicle_classes
               FROM compliance.document_types WHERE code = $1"#,
        )
        .bind(code)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| map_doc_type(&r)))
    }

    async fn list_required_for(&self, entity_type: &str, jurisdiction: &str)
        -> anyhow::Result<Vec<DocumentType>>
    {
        let rows = sqlx::query(
            r#"SELECT id, code, jurisdiction, applicable_to, name, description,
                      is_required, has_expiry, warn_days_before, grace_period_days, vehicle_classes
               FROM compliance.document_types
               WHERE is_required = true
                 AND jurisdiction = $1
                 AND $2 = ANY(applicable_to)
               ORDER BY name"#,
        )
        .bind(jurisdiction)
        .bind(entity_type)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(map_doc_type).collect())
    }

    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<DocumentType>> {
        let row = sqlx::query(
            r#"SELECT id, code, jurisdiction, applicable_to, name, description,
                      is_required, has_expiry, warn_days_before, grace_period_days, vehicle_classes
               FROM compliance.document_types WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| map_doc_type(&r)))
    }
}
