//! PostgreSQL repository implementations for the Hub Operations service.
//!
//! Provides `PgHubRepository` and `PgInductionRepository`, both of which
//! satisfy the `HubRepository` and `InductionRepository` traits defined in
//! `crate::application::services`.
//!
//! Tables
//! ──────
//!   hub_ops.hubs               — physical hub facilities
//!   hub_ops.parcel_inductions  — per-parcel lifecycle within a hub
//!
//! Both `save` methods use `INSERT … ON CONFLICT (id) DO UPDATE SET` for
//! upsert semantics, making them safe to call from both create and update paths.

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use logisticos_types::TenantId;

use crate::{
    application::services::{HubRepository, InductionRepository},
    domain::entities::{Hub, HubId, InductionId, InductionStatus, ParcelInduction},
};

// ---------------------------------------------------------------------------
// Row types
// ---------------------------------------------------------------------------

struct HubRow {
    id:            Uuid,
    tenant_id:     Uuid,
    name:          String,
    address:       String,
    lat:           f64,
    lng:           f64,
    capacity:      i32,
    current_load:  i32,
    serving_zones: Vec<String>,
    is_active:     bool,
    created_at:    chrono::DateTime<chrono::Utc>,
    updated_at:    chrono::DateTime<chrono::Utc>,
}

impl From<HubRow> for Hub {
    fn from(r: HubRow) -> Self {
        Hub {
            id:            HubId::from_uuid(r.id),
            tenant_id:     TenantId::from_uuid(r.tenant_id),
            name:          r.name,
            address:       r.address,
            lat:           r.lat,
            lng:           r.lng,
            capacity:      r.capacity as u32,
            current_load:  r.current_load as u32,
            serving_zones: r.serving_zones,
            is_active:     r.is_active,
            created_at:    r.created_at,
            updated_at:    r.updated_at,
        }
    }
}

struct InductionRow {
    id:              Uuid,
    hub_id:          Uuid,
    tenant_id:       Uuid,
    shipment_id:     Uuid,
    tracking_number: String,
    status:          String,
    zone:            Option<String>,
    bay:             Option<String>,
    inducted_by:     Option<Uuid>,
    inducted_at:     chrono::DateTime<chrono::Utc>,
    sorted_at:       Option<chrono::DateTime<chrono::Utc>>,
    dispatched_at:   Option<chrono::DateTime<chrono::Utc>>,
}

impl From<InductionRow> for ParcelInduction {
    fn from(r: InductionRow) -> Self {
        let status = match r.status.as_str() {
            "sorted"     => InductionStatus::Sorted,
            "dispatched" => InductionStatus::Dispatched,
            "returned"   => InductionStatus::Returned,
            _            => InductionStatus::Inducted,
        };
        ParcelInduction {
            id:              InductionId::from_uuid(r.id),
            hub_id:          HubId::from_uuid(r.hub_id),
            tenant_id:       TenantId::from_uuid(r.tenant_id),
            shipment_id:     r.shipment_id,
            tracking_number: r.tracking_number,
            status,
            zone:            r.zone,
            bay:             r.bay,
            inducted_by:     r.inducted_by,
            inducted_at:     r.inducted_at,
            sorted_at:       r.sorted_at,
            dispatched_at:   r.dispatched_at,
        }
    }
}

// ---------------------------------------------------------------------------
// Helper: InductionStatus → string
// ---------------------------------------------------------------------------

fn induction_status_str(s: &InductionStatus) -> &'static str {
    match s {
        InductionStatus::Inducted   => "inducted",
        InductionStatus::Sorted     => "sorted",
        InductionStatus::Dispatched => "dispatched",
        InductionStatus::Returned   => "returned",
    }
}

// ---------------------------------------------------------------------------
// PgHubRepository
// ---------------------------------------------------------------------------

/// PostgreSQL-backed implementation of `HubRepository`.
pub struct PgHubRepository {
    pub pool: PgPool,
}

impl PgHubRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HubRepository for PgHubRepository {
    /// Look up a single hub by its UUID primary key.
    async fn find_by_id(&self, id: &HubId) -> anyhow::Result<Option<Hub>> {
        let row = sqlx::query_as!(
            HubRow,
            r#"
            SELECT id, tenant_id, name, address,
                   lat, lng, capacity, current_load,
                   serving_zones, is_active, created_at, updated_at
            FROM hub_ops.hubs
            WHERE id = $1
            "#,
            id.inner()
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Hub::from))
    }

    /// Return all active hubs for a tenant, ordered by name.
    async fn list(&self, tenant_id: &TenantId) -> anyhow::Result<Vec<Hub>> {
        let rows = sqlx::query_as!(
            HubRow,
            r#"
            SELECT id, tenant_id, name, address,
                   lat, lng, capacity, current_load,
                   serving_zones, is_active, created_at, updated_at
            FROM hub_ops.hubs
            WHERE tenant_id = $1
              AND is_active = true
            ORDER BY name ASC
            "#,
            tenant_id.inner()
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Hub::from).collect())
    }

    /// Upsert a hub record.  The ON CONFLICT clause updates only mutable
    /// operational columns — `name`, `address`, `lat`, `lng`, and `capacity`
    /// are intentionally excluded from the UPDATE to protect against accidental
    /// overwrite of configuration data.
    async fn save(&self, hub: &Hub) -> anyhow::Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO hub_ops.hubs (
                id, tenant_id, name, address,
                lat, lng, capacity, current_load,
                serving_zones, is_active, created_at, updated_at
            ) VALUES (
                $1, $2, $3, $4,
                $5, $6, $7, $8,
                $9, $10, $11, $12
            )
            ON CONFLICT (id) DO UPDATE SET
                current_load  = EXCLUDED.current_load,
                serving_zones = EXCLUDED.serving_zones,
                is_active     = EXCLUDED.is_active,
                updated_at    = EXCLUDED.updated_at
            "#,
            hub.id.inner(),
            hub.tenant_id.inner(),
            hub.name,
            hub.address,
            hub.lat,
            hub.lng,
            hub.capacity as i32,
            hub.current_load as i32,
            &hub.serving_zones,
            hub.is_active,
            hub.created_at,
            hub.updated_at,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// PgInductionRepository
// ---------------------------------------------------------------------------

/// PostgreSQL-backed implementation of `InductionRepository`.
pub struct PgInductionRepository {
    pub pool: PgPool,
}

impl PgInductionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl InductionRepository for PgInductionRepository {
    /// Look up a single parcel induction by primary key.
    async fn find_by_id(&self, id: &InductionId) -> anyhow::Result<Option<ParcelInduction>> {
        let row = sqlx::query_as!(
            InductionRow,
            r#"
            SELECT id, hub_id, tenant_id, shipment_id, tracking_number,
                   status, zone, bay, inducted_by,
                   inducted_at, sorted_at, dispatched_at
            FROM hub_ops.parcel_inductions
            WHERE id = $1
            "#,
            id.inner()
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(ParcelInduction::from))
    }

    /// Find the most recent induction record for a given shipment, regardless
    /// of which hub it was inducted into.  Returns the first row (LIMIT 1)
    /// ordered by `inducted_at DESC` so newer records take precedence.
    async fn find_by_shipment(&self, shipment_id: Uuid) -> anyhow::Result<Option<ParcelInduction>> {
        let row = sqlx::query_as!(
            InductionRow,
            r#"
            SELECT id, hub_id, tenant_id, shipment_id, tracking_number,
                   status, zone, bay, inducted_by,
                   inducted_at, sorted_at, dispatched_at
            FROM hub_ops.parcel_inductions
            WHERE shipment_id = $1
            ORDER BY inducted_at DESC
            LIMIT 1
            "#,
            shipment_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(ParcelInduction::from))
    }

    /// Return all active (inducted or sorted) induction records for a hub,
    /// ordered with most recently inducted parcels first.
    async fn list_active(&self, hub_id: &HubId) -> anyhow::Result<Vec<ParcelInduction>> {
        let rows = sqlx::query_as!(
            InductionRow,
            r#"
            SELECT id, hub_id, tenant_id, shipment_id, tracking_number,
                   status, zone, bay, inducted_by,
                   inducted_at, sorted_at, dispatched_at
            FROM hub_ops.parcel_inductions
            WHERE hub_id = $1
              AND status IN ('inducted', 'sorted')
            ORDER BY inducted_at DESC
            "#,
            hub_id.inner()
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(ParcelInduction::from).collect())
    }

    /// Upsert an induction record.  The ON CONFLICT clause updates only the
    /// columns that change over the lifecycle of a parcel within the hub:
    /// `status`, `zone`, `bay`, `sorted_at`, `dispatched_at`.  Immutable
    /// columns (`hub_id`, `tenant_id`, `shipment_id`, `tracking_number`,
    /// `inducted_by`, `inducted_at`) are never overwritten.
    async fn save(&self, i: &ParcelInduction) -> anyhow::Result<()> {
        let status = induction_status_str(&i.status);

        sqlx::query!(
            r#"
            INSERT INTO hub_ops.parcel_inductions (
                id, hub_id, tenant_id, shipment_id, tracking_number,
                status, zone, bay, inducted_by,
                inducted_at, sorted_at, dispatched_at
            ) VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8, $9,
                $10, $11, $12
            )
            ON CONFLICT (id) DO UPDATE SET
                status        = EXCLUDED.status,
                zone          = EXCLUDED.zone,
                bay           = EXCLUDED.bay,
                sorted_at     = EXCLUDED.sorted_at,
                dispatched_at = EXCLUDED.dispatched_at
            "#,
            i.id.inner(),
            i.hub_id.inner(),
            i.tenant_id.inner(),
            i.shipment_id,
            i.tracking_number,
            status,
            i.zone,
            i.bay,
            i.inducted_by,
            i.inducted_at,
            i.sorted_at,
            i.dispatched_at,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
