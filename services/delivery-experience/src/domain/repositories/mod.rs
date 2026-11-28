use async_trait::async_trait;
use uuid::Uuid;

use logisticos_types::TenantId;

use crate::domain::entities::TrackingRecord;

#[async_trait]
pub trait TrackingRepository: Send + Sync {
    async fn find_by_shipment_id(&self, shipment_id: Uuid) -> anyhow::Result<Option<TrackingRecord>>;

    /// Public lookup by tracking number — no tenant required; tenant is read from the record.
    async fn find_by_tracking_number(&self, tracking_number: &str) -> anyhow::Result<Option<TrackingRecord>>;

    /// `owner_id` of `None` lists the whole tenant; `Some` only that owner's.
    async fn list_by_tenant(
        &self,
        tenant_id: &TenantId,
        owner_id: Option<Uuid>,
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

    /// Persist one feedback row for a shipment. Returns `false` (and writes
    /// nothing) when that shipment already has feedback.
    async fn save_feedback(&self, feedback: &NewFeedback) -> anyhow::Result<bool>;

    /// Record customer's self-confirmation of receipt.
    /// Sets `customer_confirmed_at` timestamp on the tracking record.
    async fn confirm_customer_receipt(&self, tracking_number: &str) -> anyhow::Result<()>;

    /// Record a customer's request to have their receipt emailed.
    /// Stores the requested email address and timestamp; the engagement engine
    /// picks this up via Kafka outbox and sends the actual email.
    async fn record_receipt_email_request(
        &self,
        tracking_number: &str,
        email: &str,
    ) -> anyhow::Result<()>;
}

/// A customer's feedback on one delivered shipment. Tenant, shipment and
/// driver come from the tracking record, never from the request.
#[derive(Debug, Clone)]
pub struct NewFeedback {
    pub tenant_id:       Uuid,
    pub shipment_id:     Uuid,
    /// The driver on the shipment when feedback was left, if one was assigned.
    pub driver_id:       Option<Uuid>,
    pub tracking_number: String,
    pub rating:          i16,
    pub tags:            Vec<String>,
    pub comments:        Option<String>,
}
