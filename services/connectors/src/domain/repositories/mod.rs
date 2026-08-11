use async_trait::async_trait;
use logisticos_errors::AppResult;
use uuid::Uuid;

use crate::domain::entities::ConnectorCredentials;

#[async_trait]
pub trait CredentialsRepository: Send + Sync {
    /// Find active credentials for a tenant+platform combination.
    async fn find(
        &self,
        tenant_id: Uuid,
        platform: &str,
    ) -> AppResult<Option<ConnectorCredentials>>;

    /// Insert or update credentials (upsert on tenant_id + platform).
    async fn upsert(&self, creds: &ConnectorCredentials) -> AppResult<()>;

    /// Soft-delete by marking is_active = false.
    async fn delete(&self, tenant_id: Uuid, platform: &str) -> AppResult<()>;

    /// Take ownership of up to `limit` connectors whose catalog sync is due.
    ///
    /// **Claiming, not listing.** The row's `last_synced_at` is stamped in the
    /// same statement that selects it, under `FOR UPDATE SKIP LOCKED`, so two
    /// replicas sweeping at the same instant cannot both take the same
    /// connector. That matters more than it sounds: a double claim fetches a
    /// merchant's shop twice simultaneously, and the cost of that lands on
    /// *their* server.
    ///
    /// Deliberately across all tenants — an unattended sweep is an operator
    /// concern, and scoping it per tenant would mean it only runs for tenants
    /// someone remembered to enumerate.
    async fn claim_due_syncs(&self, limit: i64) -> AppResult<Vec<ConnectorCredentials>>;

    /// Record how a claimed sync went. `None` clears a previous error, so a
    /// stale message cannot outlive the fault it described.
    async fn record_sync_result(&self, id: Uuid, error: Option<&str>) -> AppResult<()>;

    /// List all active connectors for a tenant (for the credentials management UI).
    async fn list_for_tenant(&self, tenant_id: Uuid) -> AppResult<Vec<ConnectorCredentials>>;
}
