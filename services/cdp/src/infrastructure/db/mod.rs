use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use logisticos_types::TenantId;

use crate::domain::{
    entities::{CustomerProfile, CustomerId},
    repositories::{CustomerProfileRepository, ProfileFilter},
};

// ---------------------------------------------------------------------------
// Row type — maps PostgreSQL columns to a flat struct, then convert to domain.
// ---------------------------------------------------------------------------

struct ProfileRow {
    id:                          Uuid,
    tenant_id:                   Uuid,
    external_customer_id:        Uuid,
    name:                        Option<String>,
    email:                       Option<String>,
    phone:                       Option<String>,
    total_shipments:             i32,
    successful_deliveries:       i32,
    failed_deliveries:           i32,
    first_shipment_at:           Option<chrono::DateTime<chrono::Utc>>,
    last_shipment_at:            Option<chrono::DateTime<chrono::Utc>>,
    total_cod_collected_cents:   i64,
    address_history:             serde_json::Value,
    recent_events:               serde_json::Value,
    clv_score:                   f32,
    engagement_score:            f32,
    created_at:                  chrono::DateTime<chrono::Utc>,
    updated_at:                  chrono::DateTime<chrono::Utc>,
}

impl TryFrom<ProfileRow> for CustomerProfile {
    type Error = anyhow::Error;

    fn try_from(r: ProfileRow) -> Result<Self, Self::Error> {
        let address_history = serde_json::from_value(r.address_history)?;
        let recent_events = serde_json::from_value(r.recent_events)?;

        Ok(CustomerProfile {
            id:                        CustomerId::from_uuid(r.id),
            tenant_id:                 TenantId::from_uuid(r.tenant_id),
            external_customer_id:      r.external_customer_id,
            name:                      r.name,
            email:                     r.email,
            phone:                     r.phone,
            total_shipments:           r.total_shipments as u32,
            successful_deliveries:     r.successful_deliveries as u32,
            failed_deliveries:         r.failed_deliveries as u32,
            first_shipment_at:         r.first_shipment_at,
            last_shipment_at:          r.last_shipment_at,
            total_cod_collected_cents: r.total_cod_collected_cents,
            address_history,
            recent_events,
            clv_score:                 r.clv_score,
            engagement_score:          r.engagement_score,
            created_at:                r.created_at,
            updated_at:                r.updated_at,
        })
    }
}

// ---------------------------------------------------------------------------
// Repository implementation
// ---------------------------------------------------------------------------

pub struct PgCustomerProfileRepository {
    pool: PgPool,
}

impl PgCustomerProfileRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CustomerProfileRepository for PgCustomerProfileRepository {
    async fn find_by_id(&self, id: &CustomerId) -> anyhow::Result<Option<CustomerProfile>> {
        let row = sqlx::query_as!(
            ProfileRow,
            r#"
            SELECT
                id, tenant_id, external_customer_id,
                name, email, phone,
                total_shipments, successful_deliveries, failed_deliveries,
                first_shipment_at, last_shipment_at,
                total_cod_collected_cents,
                address_history, recent_events,
                clv_score, engagement_score,
                created_at, updated_at
            FROM cdp.customer_profiles
            WHERE id = $1
            "#,
            id.inner()
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(CustomerProfile::try_from).transpose()
    }

    async fn find_by_external_id(
        &self,
        tenant_id: &TenantId,
        external_id: Uuid,
    ) -> anyhow::Result<Option<CustomerProfile>> {
        let row = sqlx::query_as!(
            ProfileRow,
            r#"
            SELECT
                id, tenant_id, external_customer_id,
                name, email, phone,
                total_shipments, successful_deliveries, failed_deliveries,
                first_shipment_at, last_shipment_at,
                total_cod_collected_cents,
                address_history, recent_events,
                clv_score, engagement_score,
                created_at, updated_at
            FROM cdp.customer_profiles
            WHERE tenant_id = $1 AND external_customer_id = $2
            "#,
            tenant_id.inner(),
            external_id
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(CustomerProfile::try_from).transpose()
    }

    async fn find_by_email(
        &self,
        tenant_id: &TenantId,
        email: &str,
    ) -> anyhow::Result<Option<CustomerProfile>> {
        let row = sqlx::query_as!(
            ProfileRow,
            r#"
            SELECT
                id, tenant_id, external_customer_id,
                name, email, phone,
                total_shipments, successful_deliveries, failed_deliveries,
                first_shipment_at, last_shipment_at,
                total_cod_collected_cents,
                address_history, recent_events,
                clv_score, engagement_score,
                created_at, updated_at
            FROM cdp.customer_profiles
            WHERE tenant_id = $1 AND email = $2
            "#,
            tenant_id.inner(),
            email
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(CustomerProfile::try_from).transpose()
    }

    async fn save(&self, p: &CustomerProfile) -> anyhow::Result<()> {
        let address_json = serde_json::to_value(&p.address_history)?;
        let events_json  = serde_json::to_value(&p.recent_events)?;

        sqlx::query!(
            r#"
            INSERT INTO cdp.customer_profiles (
                id, tenant_id, external_customer_id,
                name, email, phone,
                total_shipments, successful_deliveries, failed_deliveries,
                first_shipment_at, last_shipment_at,
                total_cod_collected_cents,
                address_history, recent_events,
                clv_score, engagement_score,
                created_at, updated_at
            ) VALUES (
                $1,  $2,  $3,
                $4,  $5,  $6,
                $7,  $8,  $9,
                $10, $11, $12,
                $13, $14,
                $15, $16,
                $17, $18
            )
            ON CONFLICT (id) DO UPDATE SET
                name                      = EXCLUDED.name,
                email                     = EXCLUDED.email,
                phone                     = EXCLUDED.phone,
                total_shipments           = EXCLUDED.total_shipments,
                successful_deliveries     = EXCLUDED.successful_deliveries,
                failed_deliveries         = EXCLUDED.failed_deliveries,
                first_shipment_at         = EXCLUDED.first_shipment_at,
                last_shipment_at          = EXCLUDED.last_shipment_at,
                total_cod_collected_cents = EXCLUDED.total_cod_collected_cents,
                address_history           = EXCLUDED.address_history,
                recent_events             = EXCLUDED.recent_events,
                clv_score                 = EXCLUDED.clv_score,
                engagement_score          = EXCLUDED.engagement_score,
                updated_at                = EXCLUDED.updated_at
            "#,
            p.id.inner(),
            p.tenant_id.inner(),
            p.external_customer_id,
            p.name,
            p.email,
            p.phone,
            p.total_shipments as i32,
            p.successful_deliveries as i32,
            p.failed_deliveries as i32,
            p.first_shipment_at,
            p.last_shipment_at,
            p.total_cod_collected_cents,
            address_json,
            events_json,
            p.clv_score,
            p.engagement_score,
            p.created_at,
            p.updated_at,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn list(
        &self,
        tenant_id: &TenantId,
        filter: &ProfileFilter,
    ) -> anyhow::Result<Vec<CustomerProfile>> {
        // Build dynamic query with optional filters.
        // Using format! for optional clauses; parameters are all typed so no SQL injection risk.
        let name_pattern = filter.name_contains.as_deref()
            .map(|n| format!("%{}%", n.replace('%', "\\%")));

        let rows = sqlx::query_as!(
            ProfileRow,
            r#"
            SELECT
                id, tenant_id, external_customer_id,
                name, email, phone,
                total_shipments, successful_deliveries, failed_deliveries,
                first_shipment_at, last_shipment_at,
                total_cod_collected_cents,
                address_history, recent_events,
                clv_score, engagement_score,
                created_at, updated_at
            FROM cdp.customer_profiles
            WHERE tenant_id = $1
              AND ($2::text IS NULL OR name ILIKE $2)
              AND ($3::text IS NULL OR email = $3)
              AND ($4::text IS NULL OR phone = $4)
              AND ($5::float4 IS NULL OR clv_score >= $5)
            ORDER BY last_shipment_at DESC NULLS LAST
            LIMIT $6 OFFSET $7
            "#,
            tenant_id.inner(),
            name_pattern,
            filter.email,
            filter.phone,
            filter.min_clv,
            filter.limit,
            filter.offset,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(CustomerProfile::try_from)
            .collect()
    }

    async fn top_by_clv(
        &self,
        tenant_id: &TenantId,
        limit: i64,
    ) -> anyhow::Result<Vec<CustomerProfile>> {
        let rows = sqlx::query_as!(
            ProfileRow,
            r#"
            SELECT
                id, tenant_id, external_customer_id,
                name, email, phone,
                total_shipments, successful_deliveries, failed_deliveries,
                first_shipment_at, last_shipment_at,
                total_cod_collected_cents,
                address_history, recent_events,
                clv_score, engagement_score,
                created_at, updated_at
            FROM cdp.customer_profiles
            WHERE tenant_id = $1
            ORDER BY clv_score DESC
            LIMIT $2
            "#,
            tenant_id.inner(),
            limit,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(CustomerProfile::try_from)
            .collect()
    }

    async fn count(&self, tenant_id: &TenantId) -> anyhow::Result<i64> {
        let row = sqlx::query!(
            "SELECT COUNT(*) AS cnt FROM cdp.customer_profiles WHERE tenant_id = $1",
            tenant_id.inner()
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row.cnt.unwrap_or(0))
    }
}
