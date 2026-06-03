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

pub mod hub_transfer;

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use logisticos_types::{
    TenantId, PalletId, ContainerId, PalletStatus, ContainerStatus, TransportMode,
    awb::{Awb, ChildAwb},
};

use crate::{
    application::services::{
        HubRepository, InductionRepository,
        pallet_service::{PalletRepository, ContainerRepository},
    },
    domain::entities::{
        Hub, HubId, InductionId, InductionStatus, ParcelInduction,
        pallet::Pallet,
        container::Container,
    },
};

// ---------------------------------------------------------------------------
// Row types
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
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

#[derive(sqlx::FromRow)]
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
        let row = sqlx::query_as::<_, HubRow>(
            r#"
            SELECT id, tenant_id, name, address,
                   lat, lng, capacity, current_load,
                   serving_zones, is_active, created_at, updated_at
            FROM hub_ops.hubs
            WHERE id = $1
            "#
        )
        .bind(id.inner())
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Hub::from))
    }

    /// Return all active hubs for a tenant, ordered by name.
    async fn list(&self, tenant_id: &TenantId) -> anyhow::Result<Vec<Hub>> {
        let rows = sqlx::query_as::<_, HubRow>(
            r#"
            SELECT id, tenant_id, name, address,
                   lat, lng, capacity, current_load,
                   serving_zones, is_active, created_at, updated_at
            FROM hub_ops.hubs
            WHERE tenant_id = $1
              AND is_active = true
            ORDER BY name ASC
            "#
        )
        .bind(tenant_id.inner())
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Hub::from).collect())
    }

    /// Upsert a hub record.  The ON CONFLICT clause updates only mutable
    /// operational columns — `name`, `address`, `lat`, `lng`, and `capacity`
    /// are intentionally excluded from the UPDATE to protect against accidental
    /// overwrite of configuration data.
    async fn save(&self, hub: &Hub) -> anyhow::Result<()> {
        sqlx::query(
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
            "#
        )
        .bind(hub.id.inner())
        .bind(hub.tenant_id.inner())
        .bind(&hub.name)
        .bind(&hub.address)
        .bind(hub.lat)
        .bind(hub.lng)
        .bind(hub.capacity as i32)
        .bind(hub.current_load as i32)
        .bind(&hub.serving_zones)
        .bind(hub.is_active)
        .bind(hub.created_at)
        .bind(hub.updated_at)
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
        let row = sqlx::query_as::<_, InductionRow>(
            r#"
            SELECT id, hub_id, tenant_id, shipment_id, tracking_number,
                   status, zone, bay, inducted_by,
                   inducted_at, sorted_at, dispatched_at
            FROM hub_ops.parcel_inductions
            WHERE id = $1
            "#
        )
        .bind(id.inner())
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(ParcelInduction::from))
    }

    /// Find the most recent induction record for a given shipment, regardless
    /// of which hub it was inducted into.  Returns the first row (LIMIT 1)
    /// ordered by `inducted_at DESC` so newer records take precedence.
    async fn find_by_shipment(&self, shipment_id: Uuid) -> anyhow::Result<Option<ParcelInduction>> {
        let row = sqlx::query_as::<_, InductionRow>(
            r#"
            SELECT id, hub_id, tenant_id, shipment_id, tracking_number,
                   status, zone, bay, inducted_by,
                   inducted_at, sorted_at, dispatched_at
            FROM hub_ops.parcel_inductions
            WHERE shipment_id = $1
            ORDER BY inducted_at DESC
            LIMIT 1
            "#
        )
        .bind(shipment_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(ParcelInduction::from))
    }

    /// Return all active (inducted or sorted) induction records for a hub,
    /// ordered with most recently inducted parcels first.
    async fn list_active(&self, hub_id: &HubId) -> anyhow::Result<Vec<ParcelInduction>> {
        let rows = sqlx::query_as::<_, InductionRow>(
            r#"
            SELECT id, hub_id, tenant_id, shipment_id, tracking_number,
                   status, zone, bay, inducted_by,
                   inducted_at, sorted_at, dispatched_at
            FROM hub_ops.parcel_inductions
            WHERE hub_id = $1
              AND status IN ('inducted', 'sorted')
            ORDER BY inducted_at DESC
            "#
        )
        .bind(hub_id.inner())
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

        sqlx::query(
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
            "#
        )
        .bind(i.id.inner())
        .bind(i.hub_id.inner())
        .bind(i.tenant_id.inner())
        .bind(i.shipment_id)
        .bind(&i.tracking_number)
        .bind(status)
        .bind(&i.zone)
        .bind(&i.bay)
        .bind(i.inducted_by)
        .bind(i.inducted_at)
        .bind(i.sorted_at)
        .bind(i.dispatched_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers: enum → DB string
// ---------------------------------------------------------------------------

fn pallet_status_str(s: &PalletStatus) -> &'static str {
    match s {
        PalletStatus::Open      => "open",
        PalletStatus::Sealed    => "sealed",
        PalletStatus::Loaded    => "loaded",
        PalletStatus::InTransit => "in_transit",
        PalletStatus::Arrived   => "arrived",
        PalletStatus::Broken    => "broken",
    }
}

fn pallet_status_from_str(s: &str) -> PalletStatus {
    match s {
        "sealed"     => PalletStatus::Sealed,
        "loaded"     => PalletStatus::Loaded,
        "in_transit" => PalletStatus::InTransit,
        "arrived"    => PalletStatus::Arrived,
        "broken"     => PalletStatus::Broken,
        _            => PalletStatus::Open,
    }
}

fn container_status_str(s: &ContainerStatus) -> &'static str {
    match s {
        ContainerStatus::Planning      => "planning",
        ContainerStatus::Manifested    => "manifested",
        ContainerStatus::Loading       => "loading",
        ContainerStatus::Sealed        => "sealed",
        ContainerStatus::InTransit     => "in_transit",
        ContainerStatus::ArrivedAtPort => "arrived_at_port",
        ContainerStatus::Customs       => "customs",
        ContainerStatus::Released      => "released",
        ContainerStatus::Deconsolidated => "deconsolidated",
        ContainerStatus::Delivered     => "delivered",
    }
}

fn container_status_from_str(s: &str) -> ContainerStatus {
    match s {
        "manifested"      => ContainerStatus::Manifested,
        "loading"         => ContainerStatus::Loading,
        "sealed"          => ContainerStatus::Sealed,
        "in_transit"      => ContainerStatus::InTransit,
        "arrived_at_port" => ContainerStatus::ArrivedAtPort,
        "customs"         => ContainerStatus::Customs,
        "released"        => ContainerStatus::Released,
        "deconsolidated"  => ContainerStatus::Deconsolidated,
        "delivered"       => ContainerStatus::Delivered,
        _                 => ContainerStatus::Planning,
    }
}

fn transport_mode_str(m: &TransportMode) -> &'static str {
    match m {
        TransportMode::Road                              => "road",
        TransportMode::SeaFcl | TransportMode::SeaLcl   => "sea",
        TransportMode::AirUld | TransportMode::AirLoose => "air",
    }
}

fn transport_mode_from_str(s: &str) -> TransportMode {
    match s {
        "sea" => TransportMode::SeaFcl,
        "air" => TransportMode::AirUld,
        _     => TransportMode::Road,
    }
}

// ---------------------------------------------------------------------------
// PgPalletRepository
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct PalletRow {
    id:                 Uuid,
    tenant_id:          Uuid,
    origin_hub_id:      Uuid,
    destination_hub_id: Option<Uuid>,
    total_weight_grams: i32,
    status:             String,
    sealed_at:          Option<chrono::DateTime<chrono::Utc>>,
    sealed_by:          Option<Uuid>,
    created_at:         chrono::DateTime<chrono::Utc>,
    updated_at:         chrono::DateTime<chrono::Utc>,
}

pub struct PgPalletRepository {
    pub pool: PgPool,
}

impl PgPalletRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }

    async fn load_pieces(&self, pallet_id: Uuid) -> anyhow::Result<Vec<ChildAwb>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT piece_awb FROM hub_ops.pallet_pieces WHERE pallet_id = $1 ORDER BY loaded_at"
        )
        .bind(pallet_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|(awb,)| ChildAwb::parse(&awb).map_err(|e| anyhow::anyhow!("{e}")))
            .collect()
    }

    fn row_to_pallet(&self, r: PalletRow, pieces: Vec<ChildAwb>) -> Pallet {
        Pallet {
            id:                 PalletId::from_uuid(r.id),
            tenant_id:          TenantId::from_uuid(r.tenant_id),
            origin_hub_id:      logisticos_types::HubId::from_uuid(r.origin_hub_id),
            destination_hub:    r.destination_hub_id.map(logisticos_types::HubId::from_uuid),
            pieces,
            total_weight_grams: r.total_weight_grams as u32,
            status:             pallet_status_from_str(&r.status),
            created_at:         r.created_at,
            sealed_at:          r.sealed_at,
            sealed_by:          r.sealed_by,
            updated_at:         r.updated_at,
        }
    }
}

#[async_trait]
impl PalletRepository for PgPalletRepository {
    async fn find_by_id(&self, id: &PalletId) -> anyhow::Result<Option<Pallet>> {
        let row = sqlx::query_as::<_, PalletRow>(
            r#"SELECT id, tenant_id, origin_hub_id, destination_hub_id,
                      total_weight_grams, status, sealed_at, sealed_by,
                      created_at, updated_at
               FROM hub_ops.pallets WHERE id = $1"#
        )
        .bind(id.inner())
        .fetch_optional(&self.pool)
        .await?;

        match row {
            None    => Ok(None),
            Some(r) => {
                let pieces = self.load_pieces(r.id).await?;
                Ok(Some(self.row_to_pallet(r, pieces)))
            }
        }
    }

    async fn find_open_for_hub(
        &self,
        hub_id:             &logisticos_types::HubId,
        destination_hub_id: Option<&logisticos_types::HubId>,
    ) -> anyhow::Result<Option<Pallet>> {
        let row = match destination_hub_id {
            Some(dest) => sqlx::query_as::<_, PalletRow>(
                r#"SELECT id, tenant_id, origin_hub_id, destination_hub_id,
                          total_weight_grams, status, sealed_at, sealed_by,
                          created_at, updated_at
                   FROM hub_ops.pallets
                   WHERE origin_hub_id = $1 AND destination_hub_id = $2 AND status = 'open'
                   ORDER BY created_at ASC LIMIT 1"#
            )
            .bind(hub_id.inner()).bind(dest.inner())
            .fetch_optional(&self.pool)
            .await?,

            None => sqlx::query_as::<_, PalletRow>(
                r#"SELECT id, tenant_id, origin_hub_id, destination_hub_id,
                          total_weight_grams, status, sealed_at, sealed_by,
                          created_at, updated_at
                   FROM hub_ops.pallets
                   WHERE origin_hub_id = $1 AND destination_hub_id IS NULL AND status = 'open'
                   ORDER BY created_at ASC LIMIT 1"#
            )
            .bind(hub_id.inner())
            .fetch_optional(&self.pool)
            .await?,
        };

        match row {
            None    => Ok(None),
            Some(r) => {
                let pieces = self.load_pieces(r.id).await?;
                Ok(Some(self.row_to_pallet(r, pieces)))
            }
        }
    }

    async fn save(&self, pallet: &Pallet) -> anyhow::Result<()> {
        let status = pallet_status_str(&pallet.status);
        sqlx::query(
            r#"INSERT INTO hub_ops.pallets
               (id, tenant_id, origin_hub_id, destination_hub_id,
                total_weight_grams, status, sealed_at, sealed_by, created_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
               ON CONFLICT (id) DO UPDATE SET
                total_weight_grams = EXCLUDED.total_weight_grams,
                status             = EXCLUDED.status,
                sealed_at          = EXCLUDED.sealed_at,
                sealed_by          = EXCLUDED.sealed_by,
                updated_at         = EXCLUDED.updated_at"#
        )
        .bind(pallet.id.inner())
        .bind(pallet.tenant_id.inner())
        .bind(pallet.origin_hub_id.inner())
        .bind(pallet.destination_hub.as_ref().map(|h| h.inner()))
        .bind(pallet.total_weight_grams as i32)
        .bind(status)
        .bind(pallet.sealed_at)
        .bind(pallet.sealed_by)
        .bind(pallet.created_at)
        .bind(pallet.updated_at)
        .execute(&self.pool)
        .await?;

        // Insert new pieces — append-only; ignore already-stored ones.
        for piece in &pallet.pieces {
            sqlx::query(
                "INSERT INTO hub_ops.pallet_pieces (pallet_id, tenant_id, piece_awb)
                 VALUES ($1,$2,$3)
                 ON CONFLICT (piece_awb) DO NOTHING"
            )
            .bind(pallet.id.inner())
            .bind(pallet.tenant_id.inner())
            .bind(piece.as_str())
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// PgPalletContainerRepository  (ContainerRepository trait for PalletService)
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct ContainerRow2 {
    id:                 Uuid,
    tenant_id:          Uuid,
    transport_mode:     String,
    carrier_ref:        Option<String>,
    origin_hub_id:      Uuid,
    destination_hub_id: Uuid,
    master_awbs:        Vec<String>,
    status:             String,
    departed_at:        Option<chrono::DateTime<chrono::Utc>>,
    estimated_arrival:  Option<chrono::DateTime<chrono::Utc>>,
    arrived_at:         Option<chrono::DateTime<chrono::Utc>>,
    created_at:         chrono::DateTime<chrono::Utc>,
    updated_at:         chrono::DateTime<chrono::Utc>,
}

pub struct PgPalletContainerRepository {
    pub pool: PgPool,
}

impl PgPalletContainerRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }

    async fn load_pallet_ids(&self, container_id: Uuid) -> anyhow::Result<Vec<PalletId>> {
        let rows: Vec<(Uuid,)> = sqlx::query_as(
            "SELECT pallet_id FROM hub_ops.container_pallets WHERE container_id = $1"
        )
        .bind(container_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(id,)| PalletId::from_uuid(id)).collect())
    }

    async fn load_loose_pieces(&self, container_id: Uuid) -> anyhow::Result<Vec<ChildAwb>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT piece_awb FROM hub_ops.container_loose_pieces WHERE container_id = $1"
        )
        .bind(container_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|(awb,)| ChildAwb::parse(&awb).map_err(|e| anyhow::anyhow!("{e}")))
            .collect()
    }

    fn row_to_container(&self, r: ContainerRow2, pallets: Vec<PalletId>, loose: Vec<ChildAwb>) -> Container {
        let master_awbs: Vec<Awb> = r.master_awbs.iter()
            .filter_map(|s| Awb::parse(s).ok())
            .collect();
        Container {
            id:                ContainerId::from_uuid(r.id),
            tenant_id:         TenantId::from_uuid(r.tenant_id),
            transport_mode:    transport_mode_from_str(&r.transport_mode),
            carrier_ref:       r.carrier_ref,
            origin_hub_id:     logisticos_types::HubId::from_uuid(r.origin_hub_id),
            destination_hub:   logisticos_types::HubId::from_uuid(r.destination_hub_id),
            pallets,
            loose_pieces:      loose,
            master_awbs,
            truck_spec_id:     None,
            status:            container_status_from_str(&r.status),
            departed_at:       r.departed_at,
            estimated_arrival: r.estimated_arrival,
            arrived_at:        r.arrived_at,
            created_at:        r.created_at,
            updated_at:        r.updated_at,
        }
    }
}

#[async_trait]
impl ContainerRepository for PgPalletContainerRepository {
    async fn find_by_id(&self, id: &ContainerId) -> anyhow::Result<Option<Container>> {
        let row = sqlx::query_as::<_, ContainerRow2>(
            r#"SELECT id, tenant_id, transport_mode, carrier_ref,
                      origin_hub_id, destination_hub_id, master_awbs,
                      status, departed_at, estimated_arrival, arrived_at,
                      created_at, updated_at
               FROM hub_ops.containers WHERE id = $1"#
        )
        .bind(id.inner())
        .fetch_optional(&self.pool)
        .await?;

        match row {
            None    => Ok(None),
            Some(r) => {
                let id_uuid = r.id;
                let pallets = self.load_pallet_ids(id_uuid).await?;
                let loose   = self.load_loose_pieces(id_uuid).await?;
                Ok(Some(self.row_to_container(r, pallets, loose)))
            }
        }
    }

    async fn save(&self, c: &Container) -> anyhow::Result<()> {
        let status       = container_status_str(&c.status);
        let mode         = transport_mode_str(&c.transport_mode);
        let master_awbs: Vec<&str> = c.master_awbs.iter().map(|a| a.as_str()).collect();

        sqlx::query(
            r#"INSERT INTO hub_ops.containers
               (id, tenant_id, transport_mode, carrier_ref,
                origin_hub_id, destination_hub_id, master_awbs,
                status, departed_at, estimated_arrival, arrived_at,
                created_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
               ON CONFLICT (id) DO UPDATE SET
                master_awbs       = EXCLUDED.master_awbs,
                status            = EXCLUDED.status,
                departed_at       = EXCLUDED.departed_at,
                estimated_arrival = EXCLUDED.estimated_arrival,
                arrived_at        = EXCLUDED.arrived_at,
                updated_at        = EXCLUDED.updated_at"#
        )
        .bind(c.id.inner())
        .bind(c.tenant_id.inner())
        .bind(mode)
        .bind(&c.carrier_ref)
        .bind(c.origin_hub_id.inner())
        .bind(c.destination_hub.inner())
        .bind(&master_awbs)
        .bind(status)
        .bind(c.departed_at)
        .bind(c.estimated_arrival)
        .bind(c.arrived_at)
        .bind(c.created_at)
        .bind(c.updated_at)
        .execute(&self.pool)
        .await?;

        // Insert pallet links — ignore duplicates.
        for pallet_id in &c.pallets {
            sqlx::query(
                "INSERT INTO hub_ops.container_pallets (container_id, pallet_id, tenant_id)
                 VALUES ($1,$2,$3)
                 ON CONFLICT (container_id, pallet_id) DO NOTHING"
            )
            .bind(c.id.inner())
            .bind(pallet_id.inner())
            .bind(c.tenant_id.inner())
            .execute(&self.pool)
            .await?;
        }

        // Insert loose pieces — ignore duplicates.
        for piece in &c.loose_pieces {
            sqlx::query(
                "INSERT INTO hub_ops.container_loose_pieces (container_id, tenant_id, piece_awb)
                 VALUES ($1,$2,$3)
                 ON CONFLICT (piece_awb) DO NOTHING"
            )
            .bind(c.id.inner())
            .bind(c.tenant_id.inner())
            .bind(piece.as_str())
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    async fn resolve_awbs_to_shipment_ids(&self, awbs: &[String]) -> anyhow::Result<Vec<uuid::Uuid>> {
        if awbs.is_empty() { return Ok(vec![]); }
        let rows: Vec<(uuid::Uuid,)> = sqlx::query_as(
            "SELECT DISTINCT shipment_id FROM hub_ops.parcel_inductions \
             WHERE tracking_number = ANY($1)"
        )
        .bind(awbs)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }
}
