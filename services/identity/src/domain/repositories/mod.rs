use async_trait::async_trait;
use logisticos_types::{TenantId, UserId};
use crate::domain::entities::{Tenant, User, ApiKey};

#[async_trait]
pub trait TenantRepository: Send + Sync {
    async fn find_by_id(&self, id: &TenantId) -> anyhow::Result<Option<Tenant>>;
    async fn find_by_slug(&self, slug: &str) -> anyhow::Result<Option<Tenant>>;
    async fn save(&self, tenant: &Tenant) -> anyhow::Result<()>;
    async fn slug_exists(&self, slug: &str) -> anyhow::Result<bool>;
}

#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn find_by_id(&self, id: &UserId) -> anyhow::Result<Option<User>>;
    async fn find_by_email(&self, tenant_id: &TenantId, email: &str) -> anyhow::Result<Option<User>>;
    async fn save(&self, user: &User) -> anyhow::Result<()>;
    async fn list_by_tenant(&self, tenant_id: &TenantId) -> anyhow::Result<Vec<User>>;
}

#[async_trait]
pub trait ApiKeyRepository: Send + Sync {
    async fn find_by_hash(&self, key_hash: &str) -> anyhow::Result<Option<ApiKey>>;
    async fn save(&self, key: &ApiKey) -> anyhow::Result<()>;
    async fn list_by_tenant(&self, tenant_id: &TenantId) -> anyhow::Result<Vec<ApiKey>>;
    async fn revoke(&self, id: &logisticos_types::ApiKeyId) -> anyhow::Result<()>;
}
