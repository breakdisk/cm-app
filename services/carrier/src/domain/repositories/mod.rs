use async_trait::async_trait;
use logisticos_types::TenantId;
use crate::domain::entities::{Carrier, CarrierId};

#[async_trait]
pub trait CarrierRepository: Send + Sync {
    async fn find_by_id(&self, id: &CarrierId) -> anyhow::Result<Option<Carrier>>;
    async fn find_by_code(&self, tenant_id: &TenantId, code: &str) -> anyhow::Result<Option<Carrier>>;
    async fn find_by_contact_email(&self, tenant_id: &TenantId, email: &str) -> anyhow::Result<Option<Carrier>>;
    async fn list(&self, tenant_id: &TenantId, limit: i64, offset: i64) -> anyhow::Result<Vec<Carrier>>;
    async fn list_active(&self, tenant_id: &TenantId) -> anyhow::Result<Vec<Carrier>>;
    async fn save(&self, carrier: &Carrier) -> anyhow::Result<()>;
}
