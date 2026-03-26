use async_trait::async_trait;
use uuid::Uuid;

use logisticos_types::TenantId;

use crate::domain::entities::{CustomerProfile, CustomerId};

#[derive(Debug, Clone)]
pub struct ProfileFilter {
    pub name_contains: Option<String>,
    pub email:         Option<String>,
    pub phone:         Option<String>,
    pub min_clv:       Option<f32>,
    pub limit:         i64,
    pub offset:        i64,
}

impl Default for ProfileFilter {
    fn default() -> Self {
        Self {
            name_contains: None,
            email:         None,
            phone:         None,
            min_clv:       None,
            limit:         50,
            offset:        0,
        }
    }
}

#[async_trait]
pub trait CustomerProfileRepository: Send + Sync {
    /// Find profile by internal CDP id.
    async fn find_by_id(&self, id: &CustomerId) -> anyhow::Result<Option<CustomerProfile>>;

    /// Find profile by the external customer id (from order-intake / identity domains).
    async fn find_by_external_id(
        &self,
        tenant_id: &TenantId,
        external_id: Uuid,
    ) -> anyhow::Result<Option<CustomerProfile>>;

    /// Find profile by email within a tenant.
    async fn find_by_email(
        &self,
        tenant_id: &TenantId,
        email: &str,
    ) -> anyhow::Result<Option<CustomerProfile>>;

    /// Upsert — insert or replace entire profile (all fields).
    async fn save(&self, profile: &CustomerProfile) -> anyhow::Result<()>;

    /// List profiles for a tenant with optional filters.
    async fn list(
        &self,
        tenant_id: &TenantId,
        filter: &ProfileFilter,
    ) -> anyhow::Result<Vec<CustomerProfile>>;

    /// Top-N profiles by CLV score — used for analytics dashboards.
    async fn top_by_clv(
        &self,
        tenant_id: &TenantId,
        limit: i64,
    ) -> anyhow::Result<Vec<CustomerProfile>>;

    /// Count profiles for a tenant.
    async fn count(&self, tenant_id: &TenantId) -> anyhow::Result<i64>;
}
