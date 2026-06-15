use async_trait::async_trait;
use uuid::Uuid;

use logisticos_types::TenantId;

use crate::domain::entities::{CustomerProfile, CustomerId, Segment, SegmentFilter, SegmentMember};

#[derive(Debug, Clone)]
pub struct ProfileFilter {
    pub name_contains: Option<String>,
    pub email:         Option<String>,
    pub phone:         Option<String>,
    pub min_clv:       Option<f32>,
    /// Filter by profile classification: "sender", "receiver", or "unknown".
    pub profile_type:  Option<String>,
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
            profile_type:  None,
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

// ---------------------------------------------------------------------------
// SegmentRepository
// ---------------------------------------------------------------------------

#[async_trait]
pub trait SegmentRepository: Send + Sync {
    async fn create(&self, segment: &Segment) -> anyhow::Result<()>;
    async fn find_by_id(&self, tenant_id: &TenantId, id: Uuid) -> anyhow::Result<Option<Segment>>;
    async fn list(&self, tenant_id: &TenantId) -> anyhow::Result<Vec<Segment>>;
    async fn update(&self, segment: &Segment) -> anyhow::Result<()>;
    async fn delete(&self, tenant_id: &TenantId, id: Uuid) -> anyhow::Result<()>;

    /// Live query: count + sample members matching filter_criteria at request time.
    async fn query_members(
        &self,
        tenant_id: &TenantId,
        filter: &SegmentFilter,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<SegmentMember>>;

    /// Live count matching filter_criteria.
    async fn count_members(
        &self,
        tenant_id: &TenantId,
        filter: &SegmentFilter,
    ) -> anyhow::Result<i64>;

    /// Update cached customer_count + last_computed on the segment row.
    async fn refresh_count(&self, id: Uuid, count: i64) -> anyhow::Result<()>;

    /// Check whether a specific customer is in a segment (for rule evaluation).
    async fn is_member(
        &self,
        tenant_id: &TenantId,
        segment_id: Uuid,
        customer_id: Uuid,
    ) -> anyhow::Result<bool>;

    /// Return all segment IDs the customer belongs to (for pre-populating RuleContext).
    async fn segment_ids_for_customer(
        &self,
        tenant_id: &TenantId,
        customer_id: Uuid,
    ) -> anyhow::Result<Vec<Uuid>>;
}
