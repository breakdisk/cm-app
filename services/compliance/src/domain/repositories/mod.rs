use async_trait::async_trait;
use uuid::Uuid;
use crate::domain::entities::{
    ComplianceProfile, DriverDocument, DocumentType, ComplianceAuditLog,
};

#[async_trait]
pub trait ComplianceProfileRepository: Send + Sync {
    async fn find_by_entity(&self, tenant_id: Uuid, entity_type: &str, entity_id: Uuid)
        -> anyhow::Result<Option<ComplianceProfile>>;
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<ComplianceProfile>>;
    async fn list_by_tenant(
        &self,
        tenant_id: Uuid,
        status_filter: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<ComplianceProfile>>;
    async fn save(&self, profile: &ComplianceProfile) -> anyhow::Result<()>;
}

#[async_trait]
pub trait DriverDocumentRepository: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<DriverDocument>>;
    async fn list_by_profile(&self, profile_id: Uuid) -> anyhow::Result<Vec<DriverDocument>>;
    /// Returns all approved documents expiring within `within_days` days across ALL tenants.
    /// Intentionally cross-tenant — called by the system-level ExpiryCheckerService background task.
    async fn find_expiring(&self, within_days: i32) -> anyhow::Result<Vec<DriverDocument>>;
    /// Returns all approved documents where expiry_date < today, across ALL tenants.
    /// Intentionally cross-tenant — called by the system-level ExpiryCheckerService background task.
    async fn find_expired(&self) -> anyhow::Result<Vec<DriverDocument>>;
    async fn list_pending_review(&self, tenant_id: Option<Uuid>, limit: i64, offset: i64)
        -> anyhow::Result<Vec<DriverDocument>>;
    async fn save(&self, doc: &DriverDocument) -> anyhow::Result<()>;
}

#[async_trait]
pub trait DocumentTypeRepository: Send + Sync {
    async fn find_by_code(&self, code: &str) -> anyhow::Result<Option<DocumentType>>;
    async fn list_required_for(&self, entity_type: &str, jurisdiction: &str)
        -> anyhow::Result<Vec<DocumentType>>;
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<DocumentType>>;
}

#[async_trait]
pub trait AuditLogRepository: Send + Sync {
    async fn append(&self, entry: &ComplianceAuditLog) -> anyhow::Result<()>;
    async fn list_by_profile(
        &self,
        profile_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<ComplianceAuditLog>>;
}
