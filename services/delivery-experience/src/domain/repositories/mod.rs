use async_trait::async_trait;
use uuid::Uuid;

use logisticos_types::TenantId;

use crate::domain::entities::TrackingRecord;

#[async_trait]
pub trait TrackingRepository: Send + Sync {
    async fn find_by_shipment_id(&self, shipment_id: Uuid) -> anyhow::Result<Option<TrackingRecord>>;

    /// Public lookup by tracking number — no tenant required; tenant is read from the record.
    async fn find_by_tracking_number(&self, tracking_number: &str) -> anyhow::Result<Option<TrackingRecord>>;

    async fn list_by_tenant(
        &self,
        tenant_id: &TenantId,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<TrackingRecord>>;

    async fn save(&self, record: &TrackingRecord) -> anyhow::Result<()>;

    async fn reschedule(
        &self,
        tracking_number: &str,
        preferred_date: chrono::NaiveDate,
        preferred_time_slot: Option<&str>,
        reason: &str,
    ) -> anyhow::Result<()>;
}
