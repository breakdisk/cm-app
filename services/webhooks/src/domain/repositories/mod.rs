use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::entities::{Webhook, DeliveryAttempt};

#[async_trait]
pub trait WebhookRepository: Send + Sync {
    async fn save(&self, webhook: &Webhook) -> anyhow::Result<()>;
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<Webhook>>;
    async fn list_by_tenant(&self, tenant_id: Uuid) -> anyhow::Result<Vec<Webhook>>;
    async fn delete(&self, id: Uuid) -> anyhow::Result<()>;

    /// Hot path for the dispatcher: every consumed Kafka event runs this.
    /// Uses the GIN index on `events` so the lookup stays O(log n) per
    /// (tenant, event_type) pair. Includes the `*` wildcard via overlap.
    async fn find_subscribers(
        &self,
        tenant_id: Uuid,
        event_type: &str,
    ) -> anyhow::Result<Vec<Webhook>>;

    /// Bump cumulative counters + update last_delivery_at after a delivery
    /// attempt. Single update so the dispatcher doesn't race with itself
    /// on concurrent events to the same webhook.
    async fn record_attempt(
        &self,
        webhook_id: Uuid,
        success: bool,
        status_code: i32,
    ) -> anyhow::Result<()>;
}

#[async_trait]
pub trait DeliveryRepository: Send + Sync {
    async fn save(&self, attempt: &DeliveryAttempt) -> anyhow::Result<()>;
    async fn list_by_webhook(
        &self,
        webhook_id: Uuid,
        limit: i64,
    ) -> anyhow::Result<Vec<DeliveryAttempt>>;
}
