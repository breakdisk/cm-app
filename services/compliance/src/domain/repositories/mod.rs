use async_trait::async_trait;
use uuid::Uuid;
use crate::domain::entities::{
    ComplianceProfile, DriverDocument, DocumentType, ComplianceAuditLog,
};

/// One row of the admin review queue: a document awaiting review, plus who it
/// belongs to.
///
/// A query projection rather than an entity. compliance owns both tables, so
/// this join is intra-service and free — `list_pending_review` already joined
/// `compliance_profiles` to filter by tenant and simply discarded every column
/// it read. Without them the queue can only say "Profile 3f2a1b9c", which is
/// what it said.
///
/// It stops at `entity_id` deliberately. Turning that uuid into a person's name
/// is not compliance's to do: the name lives in `field_ops.couriers` and
/// `driver_ops.drivers`, and joining across a service boundary is banned. The
/// admin portal already holds both rosters and does that join client-side.
///
/// `#[serde(flatten)]` keeps the document's own fields at the top level, so the
/// wire shape stays a superset of what this endpoint returned before.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PendingReviewItem {
    #[serde(flatten)]
    pub document:       DriverDocument,
    pub entity_id:      Uuid,
    pub entity_type:    String,
    pub jurisdiction:   String,
    /// The profile's verdict as text, straight from the column. Not the
    /// `ComplianceStatus` enum: a status a newer migration adds should reach the
    /// console as itself rather than failing the whole queue's mapping.
    pub overall_status: String,
}

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
    /// The review queue, each row carrying the identity of the profile behind
    /// it. Returns [`PendingReviewItem`] rather than a bare document because a
    /// reviewer cannot act on a document without knowing whose it is; the
    /// bare-document version this replaced is why the queue read as uuids.
    async fn list_pending_review(&self, tenant_id: Option<Uuid>, limit: i64, offset: i64)
        -> anyhow::Result<Vec<PendingReviewItem>>;
    async fn save(&self, doc: &DriverDocument) -> anyhow::Result<()>;
}

#[async_trait]
pub trait DocumentTypeRepository: Send + Sync {
    async fn find_by_code(&self, code: &str) -> anyhow::Result<Option<DocumentType>>;
    async fn list_required_for(&self, entity_type: &str, jurisdiction: &str)
        -> anyhow::Result<Vec<DocumentType>>;
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<DocumentType>>;
    /// The whole catalogue, every jurisdiction.
    ///
    /// The admin console needs it to render a document's *name* — it holds a
    /// `document_type_id` and nothing else, and was printing the first twelve
    /// characters of that uuid where "Driver's Licence" belongs. Unfiltered by
    /// jurisdiction on purpose: a reviewer sees couriers from every jurisdiction
    /// the tenant operates in, and the catalogue is small, static and not
    /// tenant-scoped, so one cached fetch beats a lookup per row.
    async fn list_all(&self) -> anyhow::Result<Vec<DocumentType>>;
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

#[cfg(test)]
mod pending_review_item_tests {
    use super::PendingReviewItem;
    use crate::domain::entities::{DocumentStatus, DriverDocument};
    use uuid::Uuid;

    fn item() -> PendingReviewItem {
        PendingReviewItem {
            document: DriverDocument {
                id:                    Uuid::nil(),
                compliance_profile_id: Uuid::nil(),
                document_type_id:      Uuid::nil(),
                document_number:       "LTO-1".into(),
                issue_date:            None,
                expiry_date:           None,
                file_url:              "s3://bucket/key".into(),
                status:                DocumentStatus::Submitted,
                rejection_reason:      None,
                reviewed_by:           None,
                reviewed_at:           None,
                submitted_at:          chrono::Utc::now(),
                updated_at:            chrono::Utc::now(),
            },
            entity_id:      Uuid::nil(),
            entity_type:    "driver".into(),
            jurisdiction:   "PH".into(),
            overall_status: "under_review".into(),
        }
    }

    /// The console reads `doc.id`, `doc.status`, `doc.submitted_at` and the rest
    /// straight off each row. `#[serde(flatten)]` is what keeps them at the top
    /// level; nesting them under a `document` key would compile, deserialise on
    /// no client, and empty the queue with no error anywhere.
    #[test]
    fn the_documents_own_fields_stay_at_the_top_level() {
        let v = serde_json::to_value(item()).expect("serialises");
        for field in ["id", "compliance_profile_id", "document_type_id",
                      "document_number", "file_url", "status", "submitted_at"] {
            assert!(v.get(field).is_some(), "missing top-level field: {field}");
        }
        assert!(v.get("document").is_none(), "the document must not be nested");
    }

    /// The four columns this projection exists for. Without them the queue can
    /// only say "Profile 3f2a1b9c", which is what it said.
    #[test]
    fn the_identity_of_the_holder_travels_with_the_document() {
        let v = serde_json::to_value(item()).expect("serialises");
        assert_eq!(v["entity_id"], serde_json::json!(Uuid::nil()));
        assert_eq!(v["entity_type"], "driver");
        assert_eq!(v["jurisdiction"], "PH");
        assert_eq!(v["overall_status"], "under_review");
    }
}
