use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use logisticos_types::TenantId;

use crate::domain::{
    entities::{ConsentRecord, CustomerProfile, CustomerId, ProfileType, Segment, SegmentFilter, SegmentMember},
    repositories::{ConsentRepository, CustomerProfileRepository, ProfileFilter, SegmentRepository},
};

// ---------------------------------------------------------------------------
// Row type — maps PostgreSQL columns to a flat struct, then convert to domain.
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct ProfileRow {
    id:                          Uuid,
    tenant_id:                   Uuid,
    external_customer_id:        Uuid,
    profile_type:                String,
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

        let profile_type = match r.profile_type.as_str() {
            "sender"   => ProfileType::Sender,
            "receiver" => ProfileType::Receiver,
            _          => ProfileType::Unknown,
        };

        Ok(CustomerProfile {
            id:                        CustomerId::from_uuid(r.id),
            tenant_id:                 TenantId::from_uuid(r.tenant_id),
            external_customer_id:      r.external_customer_id,
            profile_type,
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
        let row = sqlx::query_as::<_, ProfileRow>(
            r#"
            SELECT
                id, tenant_id, external_customer_id,
                profile_type,
                name, email, phone,
                total_shipments, successful_deliveries, failed_deliveries,
                first_shipment_at, last_shipment_at,
                total_cod_collected_cents,
                address_history, recent_events,
                clv_score, engagement_score,
                created_at, updated_at
            FROM cdp.customer_profiles
            WHERE id = $1
            "#
        )
        .bind(id.inner())
        .fetch_optional(&self.pool)
        .await?;

        row.map(CustomerProfile::try_from).transpose()
    }

    async fn find_by_external_id(
        &self,
        tenant_id: &TenantId,
        external_id: Uuid,
    ) -> anyhow::Result<Option<CustomerProfile>> {
        let row = sqlx::query_as::<_, ProfileRow>(
            r#"
            SELECT
                id, tenant_id, external_customer_id,
                profile_type,
                name, email, phone,
                total_shipments, successful_deliveries, failed_deliveries,
                first_shipment_at, last_shipment_at,
                total_cod_collected_cents,
                address_history, recent_events,
                clv_score, engagement_score,
                created_at, updated_at
            FROM cdp.customer_profiles
            WHERE tenant_id = $1 AND external_customer_id = $2
            "#
        )
        .bind(tenant_id.inner())
        .bind(external_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(CustomerProfile::try_from).transpose()
    }

    async fn find_by_email(
        &self,
        tenant_id: &TenantId,
        email: &str,
    ) -> anyhow::Result<Option<CustomerProfile>> {
        let row = sqlx::query_as::<_, ProfileRow>(
            r#"
            SELECT
                id, tenant_id, external_customer_id,
                profile_type,
                name, email, phone,
                total_shipments, successful_deliveries, failed_deliveries,
                first_shipment_at, last_shipment_at,
                total_cod_collected_cents,
                address_history, recent_events,
                clv_score, engagement_score,
                created_at, updated_at
            FROM cdp.customer_profiles
            WHERE tenant_id = $1 AND email = $2
            "#
        )
        .bind(tenant_id.inner())
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;

        row.map(CustomerProfile::try_from).transpose()
    }

    async fn save(&self, p: &CustomerProfile) -> anyhow::Result<()> {
        let address_json = serde_json::to_value(&p.address_history)?;
        let events_json  = serde_json::to_value(&p.recent_events)?;

        let profile_type_str = match p.profile_type {
            ProfileType::Sender   => "sender",
            ProfileType::Receiver => "receiver",
            ProfileType::Unknown  => "unknown",
        };

        sqlx::query(
            r#"
            INSERT INTO cdp.customer_profiles (
                id, tenant_id, external_customer_id,
                profile_type,
                name, email, phone,
                total_shipments, successful_deliveries, failed_deliveries,
                first_shipment_at, last_shipment_at,
                total_cod_collected_cents,
                address_history, recent_events,
                clv_score, engagement_score,
                created_at, updated_at
            ) VALUES (
                $1,  $2,  $3,
                $4,
                $5,  $6,  $7,
                $8,  $9,  $10,
                $11, $12, $13,
                $14, $15,
                $16, $17,
                $18, $19
            )
            ON CONFLICT (id) DO UPDATE SET
                profile_type              = EXCLUDED.profile_type,
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
            "#
        )
        .bind(p.id.inner())
        .bind(p.tenant_id.inner())
        .bind(p.external_customer_id)
        .bind(profile_type_str)
        .bind(&p.name)
        .bind(&p.email)
        .bind(&p.phone)
        .bind(p.total_shipments as i32)
        .bind(p.successful_deliveries as i32)
        .bind(p.failed_deliveries as i32)
        .bind(p.first_shipment_at)
        .bind(p.last_shipment_at)
        .bind(p.total_cod_collected_cents)
        .bind(address_json)
        .bind(events_json)
        .bind(p.clv_score)
        .bind(p.engagement_score)
        .bind(p.created_at)
        .bind(p.updated_at)
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

        let days_inactive = filter.days_inactive.map(|d| d as i64);

        let rows = sqlx::query_as::<_, ProfileRow>(
            r#"
            SELECT
                id, tenant_id, external_customer_id,
                profile_type,
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
              AND ($6::text IS NULL OR profile_type = $6)
              AND ($9::bigint IS NULL OR last_shipment_at < NOW() - ($9 * INTERVAL '1 day'))
            ORDER BY last_shipment_at DESC NULLS LAST
            LIMIT $7 OFFSET $8
            "#
        )
        .bind(tenant_id.inner())
        .bind(name_pattern)
        .bind(&filter.email)
        .bind(&filter.phone)
        .bind(filter.min_clv)
        .bind(&filter.profile_type)
        .bind(filter.limit)
        .bind(filter.offset)
        .bind(days_inactive)
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
        let rows = sqlx::query_as::<_, ProfileRow>(
            r#"
            SELECT
                id, tenant_id, external_customer_id,
                profile_type,
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
            "#
        )
        .bind(tenant_id.inner())
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(CustomerProfile::try_from)
            .collect()
    }

    async fn count(&self, tenant_id: &TenantId) -> anyhow::Result<i64> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM cdp.customer_profiles WHERE tenant_id = $1"
        )
        .bind(tenant_id.inner())
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }
}

// ---------------------------------------------------------------------------
// PgConsentRepository
// ---------------------------------------------------------------------------

pub struct PgConsentRepository {
    pool: PgPool,
}

impl PgConsentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct ConsentRow {
    consent_type: String,
    granted:      bool,
    channel:      Option<String>,
    granted_at:   chrono::DateTime<chrono::Utc>,
    revoked_at:   Option<chrono::DateTime<chrono::Utc>>,
}

impl From<ConsentRow> for ConsentRecord {
    fn from(r: ConsentRow) -> Self {
        Self {
            consent_type: r.consent_type,
            granted:      r.granted,
            channel:      r.channel,
            granted_at:   r.granted_at,
            revoked_at:   r.revoked_at,
        }
    }
}

#[async_trait]
impl ConsentRepository for PgConsentRepository {
    async fn upsert(
        &self,
        tenant_id: &TenantId,
        customer_id: Uuid,
        consent_type: &str,
        granted: bool,
        channel: Option<&str>,
        ip_address: Option<&str>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO cdp.consent_records
                (tenant_id, customer_id, consent_type, granted, channel, ip_address, granted_at, revoked_at)
            VALUES ($1, $2, $3, $4, $5, $6, NOW(), NULL)
            ON CONFLICT (tenant_id, customer_id, consent_type)
            DO UPDATE SET
                granted    = EXCLUDED.granted,
                channel    = EXCLUDED.channel,
                ip_address = EXCLUDED.ip_address,
                granted_at = CASE WHEN EXCLUDED.granted THEN NOW() ELSE cdp.consent_records.granted_at END,
                revoked_at = CASE WHEN EXCLUDED.granted THEN NULL ELSE NOW() END
            "#,
        )
        .bind(tenant_id.inner())
        .bind(customer_id)
        .bind(consent_type)
        .bind(granted)
        .bind(channel)
        .bind(ip_address)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_for_customer(
        &self,
        tenant_id: &TenantId,
        customer_id: Uuid,
    ) -> anyhow::Result<Vec<ConsentRecord>> {
        let rows = sqlx::query_as::<_, ConsentRow>(
            r#"
            SELECT consent_type, granted, channel, granted_at, revoked_at
            FROM cdp.consent_records
            WHERE tenant_id = $1 AND customer_id = $2
            "#,
        )
        .bind(tenant_id.inner())
        .bind(customer_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(ConsentRecord::from).collect())
    }
}

// ---------------------------------------------------------------------------
// PgSegmentRepository
// ---------------------------------------------------------------------------

pub struct PgSegmentRepository {
    pool: PgPool,
}

impl PgSegmentRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[derive(sqlx::FromRow)]
struct SegmentRow {
    id:              Uuid,
    tenant_id:       Uuid,
    name:            String,
    description:     String,
    filter_criteria: serde_json::Value,
    customer_count:  i32,
    is_dynamic:      bool,
    last_computed:   Option<chrono::DateTime<chrono::Utc>>,
    created_at:      chrono::DateTime<chrono::Utc>,
    updated_at:      chrono::DateTime<chrono::Utc>,
}

impl TryFrom<SegmentRow> for Segment {
    type Error = anyhow::Error;

    fn try_from(r: SegmentRow) -> Result<Self, Self::Error> {
        let filter: SegmentFilter = serde_json::from_value(r.filter_criteria)?;
        Ok(Segment {
            id:             r.id,
            tenant_id:      TenantId::from_uuid(r.tenant_id),
            name:           r.name,
            description:    r.description,
            filter,
            customer_count: r.customer_count,
            is_dynamic:     r.is_dynamic,
            last_computed:  r.last_computed,
            created_at:     r.created_at,
            updated_at:     r.updated_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct MemberRow {
    external_customer_id: Uuid,
    name:                 Option<String>,
    email:                Option<String>,
    phone:                Option<String>,
    clv_score:            f32,
    engagement_score:     f32,
    last_shipment_at:     Option<chrono::DateTime<chrono::Utc>>,
}

impl From<MemberRow> for SegmentMember {
    fn from(r: MemberRow) -> Self {
        Self {
            external_customer_id: r.external_customer_id,
            name:                 r.name,
            email:                r.email,
            phone:                r.phone,
            clv_score:            r.clv_score,
            engagement_score:     r.engagement_score,
            last_shipment_at:     r.last_shipment_at,
        }
    }
}


#[async_trait]
impl SegmentRepository for PgSegmentRepository {
    async fn create(&self, segment: &Segment) -> anyhow::Result<()> {
        let filter_json = serde_json::to_value(&segment.filter)?;
        sqlx::query(
            r#"
            INSERT INTO cdp.segments
                (id, tenant_id, name, description, filter_criteria,
                 customer_count, is_dynamic, last_computed, created_at, updated_at)
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
            "#
        )
        .bind(segment.id)
        .bind(segment.tenant_id.inner())
        .bind(&segment.name)
        .bind(&segment.description)
        .bind(filter_json)
        .bind(segment.customer_count)
        .bind(segment.is_dynamic)
        .bind(segment.last_computed)
        .bind(segment.created_at)
        .bind(segment.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find_by_id(&self, tenant_id: &TenantId, id: Uuid) -> anyhow::Result<Option<Segment>> {
        let row = sqlx::query_as::<_, SegmentRow>(
            r#"
            SELECT id, tenant_id, name, description, filter_criteria,
                   customer_count, is_dynamic, last_computed, created_at, updated_at
            FROM cdp.segments
            WHERE tenant_id = $1 AND id = $2
            "#
        )
        .bind(tenant_id.inner())
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(Segment::try_from).transpose()
    }

    async fn list(&self, tenant_id: &TenantId) -> anyhow::Result<Vec<Segment>> {
        let rows = sqlx::query_as::<_, SegmentRow>(
            r#"
            SELECT id, tenant_id, name, description, filter_criteria,
                   customer_count, is_dynamic, last_computed, created_at, updated_at
            FROM cdp.segments
            WHERE tenant_id = $1
            ORDER BY name ASC
            "#
        )
        .bind(tenant_id.inner())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(Segment::try_from).collect()
    }

    async fn update(&self, segment: &Segment) -> anyhow::Result<()> {
        let filter_json = serde_json::to_value(&segment.filter)?;
        sqlx::query(
            r#"
            UPDATE cdp.segments
            SET name = $1, description = $2, filter_criteria = $3, updated_at = NOW()
            WHERE tenant_id = $4 AND id = $5
            "#
        )
        .bind(&segment.name)
        .bind(&segment.description)
        .bind(filter_json)
        .bind(segment.tenant_id.inner())
        .bind(segment.id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete(&self, tenant_id: &TenantId, id: Uuid) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM cdp.segments WHERE tenant_id = $1 AND id = $2")
            .bind(tenant_id.inner())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn query_members(
        &self,
        tenant_id: &TenantId,
        filter: &SegmentFilter,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<SegmentMember>> {
        // Churn-tier / max-churn-score require a lateral join; build conditionally.
        let need_churn = filter.churn_tier.is_some() || filter.max_churn_score.is_some();

        let churn_join = if need_churn {
            r#"
            LEFT JOIN LATERAL (
                SELECT score, tier
                FROM cdp.churn_scores cs
                WHERE cs.customer_id = cp.external_customer_id
                  AND cs.tenant_id   = cp.tenant_id
                ORDER BY cs.computed_at DESC LIMIT 1
            ) cs ON true
            "#
        } else {
            ""
        };

        let mut churn_clauses = Vec::new();
        if filter.churn_tier.is_some() { churn_clauses.push("cs.tier = $3"); }
        if filter.max_churn_score.is_some() { churn_clauses.push("cs.score <= $4"); }
        let churn_filter = if churn_clauses.is_empty() { String::new() }
                           else { format!("AND {}", churn_clauses.join(" AND ")) };

        let sql = format!(
            r#"
            SELECT
                cp.external_customer_id, cp.name, cp.email, cp.phone,
                cp.clv_score, cp.engagement_score, cp.last_shipment_at
            FROM cdp.customer_profiles cp
            {churn_join}
            WHERE cp.tenant_id = $1
              AND ($2::float4 IS NULL OR cp.clv_score >= $2)
              {churn_filter}
              AND ($5::float4 IS NULL OR cp.clv_score  <= $5)
              AND ($6::float4 IS NULL OR cp.engagement_score >= $6)
              AND ($7::int    IS NULL OR (cp.last_shipment_at IS NULL OR cp.last_shipment_at <= NOW() - ($7 * INTERVAL '1 day')))
              AND ($8::text   IS NULL OR cp.profile_type = $8)
              AND ($9::int    IS NULL OR cp.successful_deliveries >= $9)
            ORDER BY cp.clv_score DESC
            LIMIT $10 OFFSET $11
            "#
        );

        let rows = sqlx::query_as::<_, MemberRow>(&sql)
            .bind(tenant_id.inner())
            .bind(filter.min_clv)
            .bind(filter.churn_tier.as_deref())
            .bind(filter.max_churn_score)
            .bind(filter.max_clv)
            .bind(filter.min_engagement)
            .bind(filter.inactive_days)
            .bind(filter.profile_type.as_deref())
            .bind(filter.min_shipments)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.into_iter().map(SegmentMember::from).collect())
    }

    async fn count_members(
        &self,
        tenant_id: &TenantId,
        filter: &SegmentFilter,
    ) -> anyhow::Result<i64> {
        let need_churn = filter.churn_tier.is_some() || filter.max_churn_score.is_some();

        let churn_join = if need_churn {
            r#"
            LEFT JOIN LATERAL (
                SELECT score, tier
                FROM cdp.churn_scores cs
                WHERE cs.customer_id = cp.external_customer_id
                  AND cs.tenant_id   = cp.tenant_id
                ORDER BY cs.computed_at DESC LIMIT 1
            ) cs ON true
            "#
        } else {
            ""
        };

        let mut churn_clauses = Vec::new();
        if filter.churn_tier.is_some()      { churn_clauses.push("cs.tier = $3"); }
        if filter.max_churn_score.is_some() { churn_clauses.push("cs.score <= $4"); }
        let churn_filter = if churn_clauses.is_empty() { String::new() }
                           else { format!("AND {}", churn_clauses.join(" AND ")) };

        let sql = format!(
            r#"
            SELECT COUNT(*)
            FROM cdp.customer_profiles cp
            {churn_join}
            WHERE cp.tenant_id = $1
              AND ($2::float4 IS NULL OR cp.clv_score >= $2)
              {churn_filter}
              AND ($5::float4 IS NULL OR cp.clv_score  <= $5)
              AND ($6::float4 IS NULL OR cp.engagement_score >= $6)
              AND ($7::int    IS NULL OR (cp.last_shipment_at IS NULL OR cp.last_shipment_at <= NOW() - ($7 * INTERVAL '1 day')))
              AND ($8::text   IS NULL OR cp.profile_type = $8)
              AND ($9::int    IS NULL OR cp.successful_deliveries >= $9)
            "#
        );

        let row: (i64,) = sqlx::query_as(&sql)
            .bind(tenant_id.inner())
            .bind(filter.min_clv)
            .bind(filter.churn_tier.as_deref())
            .bind(filter.max_churn_score)
            .bind(filter.max_clv)
            .bind(filter.min_engagement)
            .bind(filter.inactive_days)
            .bind(filter.profile_type.as_deref())
            .bind(filter.min_shipments)
            .fetch_one(&self.pool)
            .await?;

        Ok(row.0)
    }

    async fn refresh_count(&self, id: Uuid, count: i64) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE cdp.segments SET customer_count = $1, last_computed = NOW() WHERE id = $2"
        )
        .bind(count as i32)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn is_member(
        &self,
        tenant_id: &TenantId,
        segment_id: Uuid,
        customer_id: Uuid,
    ) -> anyhow::Result<bool> {
        // Load the segment's filter criteria, then run the live query constrained to this customer.
        let row = sqlx::query_as::<_, SegmentRow>(
            r#"
            SELECT id, tenant_id, name, description, filter_criteria,
                   customer_count, is_dynamic, last_computed, created_at, updated_at
            FROM cdp.segments
            WHERE tenant_id = $1 AND id = $2
            "#
        )
        .bind(tenant_id.inner())
        .bind(segment_id)
        .fetch_optional(&self.pool)
        .await?;

        let seg = match row.map(Segment::try_from).transpose()? {
            Some(s) => s,
            None    => return Ok(false),
        };

        // Check membership using single-customer variant of count query.
        let need_churn = seg.filter.churn_tier.is_some() || seg.filter.max_churn_score.is_some();
        let churn_join = if need_churn {
            r#"
            LEFT JOIN LATERAL (
                SELECT score, tier
                FROM cdp.churn_scores cs
                WHERE cs.customer_id = cp.external_customer_id
                  AND cs.tenant_id   = cp.tenant_id
                ORDER BY cs.computed_at DESC LIMIT 1
            ) cs ON true
            "#
        } else {
            ""
        };

        let mut churn_clauses = Vec::new();
        if seg.filter.churn_tier.is_some()      { churn_clauses.push("cs.tier = $3"); }
        if seg.filter.max_churn_score.is_some() { churn_clauses.push("cs.score <= $4"); }
        let churn_filter = if churn_clauses.is_empty() { String::new() }
                           else { format!("AND {}", churn_clauses.join(" AND ")) };

        let sql = format!(
            r#"
            SELECT COUNT(*)
            FROM cdp.customer_profiles cp
            {churn_join}
            WHERE cp.tenant_id = $1
              AND cp.external_customer_id = $10
              AND ($2::float4 IS NULL OR cp.clv_score >= $2)
              {churn_filter}
              AND ($5::float4 IS NULL OR cp.clv_score  <= $5)
              AND ($6::float4 IS NULL OR cp.engagement_score >= $6)
              AND ($7::int    IS NULL OR (cp.last_shipment_at IS NULL OR cp.last_shipment_at <= NOW() - ($7 * INTERVAL '1 day')))
              AND ($8::text   IS NULL OR cp.profile_type = $8)
              AND ($9::int    IS NULL OR cp.successful_deliveries >= $9)
            "#
        );

        let row: (i64,) = sqlx::query_as(&sql)
            .bind(tenant_id.inner())
            .bind(seg.filter.min_clv)
            .bind(seg.filter.churn_tier.as_deref())
            .bind(seg.filter.max_churn_score)
            .bind(seg.filter.max_clv)
            .bind(seg.filter.min_engagement)
            .bind(seg.filter.inactive_days)
            .bind(seg.filter.profile_type.as_deref())
            .bind(seg.filter.min_shipments)
            .bind(customer_id)
            .fetch_one(&self.pool)
            .await?;

        Ok(row.0 > 0)
    }

    async fn segment_ids_for_customer(
        &self,
        tenant_id: &TenantId,
        customer_id: Uuid,
    ) -> anyhow::Result<Vec<Uuid>> {
        // Load all segments for tenant, then check membership for each.
        // For small-to-medium segment counts this is acceptable; at scale
        // a materialised `cdp.segment_members` table with a nightly job is preferred.
        let rows = sqlx::query_as::<_, SegmentRow>(
            r#"
            SELECT id, tenant_id, name, description, filter_criteria,
                   customer_count, is_dynamic, last_computed, created_at, updated_at
            FROM cdp.segments
            WHERE tenant_id = $1
            "#
        )
        .bind(tenant_id.inner())
        .fetch_all(&self.pool)
        .await?;

        let mut ids = Vec::new();
        for seg_row in rows {
            let seg = Segment::try_from(seg_row)?;
            if self.is_member(tenant_id, seg.id, customer_id).await? {
                ids.push(seg.id);
            }
        }
        Ok(ids)
    }
}
