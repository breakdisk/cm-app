use async_trait::async_trait;
use logisticos_types::{TenantId, UserId};
use crate::domain::entities::{Tenant, User, ApiKey, AuthIdentity, AuthProvider};

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
    /// Look up a user by their E.164-normalised phone number within a tenant.
    /// Used by `otp_verify` to resolve a pre-registered driver without relying
    /// on the synthetic-email fallback.
    async fn find_by_phone(&self, tenant_id: &TenantId, phone: &str) -> anyhow::Result<Option<User>>;
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

#[async_trait]
pub trait AuthIdentityRepository: Send + Sync {
    async fn find_by_provider_subject(
        &self,
        provider: AuthProvider,
        subject: &str,
    ) -> anyhow::Result<Option<AuthIdentity>>;

    async fn list_for_user(&self, user_id: &UserId) -> anyhow::Result<Vec<AuthIdentity>>;

    async fn insert(&self, identity: &AuthIdentity) -> anyhow::Result<()>;
}
