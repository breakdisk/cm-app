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

    /// List all active connectors for a tenant (for the credentials management UI).
    async fn list_for_tenant(&self, tenant_id: Uuid) -> AppResult<Vec<ConnectorCredentials>>;
}
