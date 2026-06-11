//! Postgres repository implementations for the cross-border hub transfer
//! entities (HubScan, HubLocation, HubInventory, HubTransferManifest,
//! HubRoutingConfig). Mirrors the upsert/FromRow style of the sibling repos in
//! `super`.
//!
//! Tables: hub_ops.hub_scans (append-only), hub_ops.hub_locations,
//! hub_ops.hub_inventory, hub_ops.hub_transfer_manifests,
//! hub_ops.hub_routing_configs.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use logisticos_types::{
    awb::{Awb, ChildAwb},
    ContainerId, HubId, PalletId, ShipmentId, TenantId, TransportMode,
};

use crate::application::services::hub_transfer_repositories::{
    HubInventoryRepository, HubLocationRepository, HubRoutingConfigRepository, HubScanRepository,
    HubTransferManifestRepository,
};
use crate::domain::entities::{
    CustomsStatus, HubInventory, HubLocation, HubRoutingConfig, HubScan, HubTransferManifest,
    InventoryUnit, RoutingType, ScanException, ScanType,
};

// ── enum <-> string mappers ───────────────────────────────────────────────────

fn scan_type_str(s: ScanType) -> &'static str {
    match s {
        ScanType::InboundReceive         => "inbound_receive",
        ScanType::PalletAssign           => "pallet_assign",
        ScanType::OutboundLoad           => "outbound_load",
        ScanType::ContainerDeconsolidate => "container_deconsolidate",
        ScanType::LocalSortAssign        => "local_sort_assign",
        ScanType::ExceptionFlag          => "exception_flag",
    }
}

fn scan_type_from_str(s: &str) -> ScanType {
    match s {
        "pallet_assign"           => ScanType::PalletAssign,
        "outbound_load"           => ScanType::OutboundLoad,
        "container_deconsolidate" => ScanType::ContainerDeconsolidate,
        "local_sort_assign"       => ScanType::LocalSortAssign,
        "exception_flag"          => ScanType::ExceptionFlag,
        _                         => ScanType::InboundReceive,
    }
}

fn scan_exception_str(e: ScanException) -> &'static str {
    match e {
        ScanException::Missing        => "missing",
        ScanException::Damaged        => "damaged",
        ScanException::WeightMismatch => "weight_mismatch",
    }
}

fn scan_exception_from_str(s: &str) -> Option<ScanException> {
    match s {
        "missing"         => Some(ScanException::Missing),
        "damaged"         => Some(ScanException::Damaged),
        "weight_mismatch" => Some(ScanException::WeightMismatch),
        _                 => None,
    }
}

fn transport_mode_str(m: TransportMode) -> &'static str {
    match m {
        TransportMode::Road     => "road",
        TransportMode::SeaFcl   => "sea_fcl",
        TransportMode::SeaLcl   => "sea_lcl",
        TransportMode::AirUld   => "air_uld",
        TransportMode::AirLoose => "air_loose",
    }
}

fn transport_mode_from_str(s: &str) -> TransportMode {
    match s {
        "sea_fcl"   => TransportMode::SeaFcl,
        "sea_lcl"   => TransportMode::SeaLcl,
        "air_uld"   => TransportMode::AirUld,
        "air_loose" => TransportMode::AirLoose,
        _           => TransportMode::Road,
    }
}

fn customs_status_str(s: CustomsStatus) -> &'static str {
    match s {
        CustomsStatus::NotRequired => "not_required",
        CustomsStatus::Pending     => "pending",
        CustomsStatus::Hold        => "hold",
        CustomsStatus::Cleared     => "cleared",
    }
}

fn customs_status_from_str(s: &str) -> CustomsStatus {
    match s {
        "pending" => CustomsStatus::Pending,
        "hold"    => CustomsStatus::Hold,
        "cleared" => CustomsStatus::Cleared,
        _         => CustomsStatus::NotRequired,
    }
}

fn routing_type_str(r: RoutingType) -> &'static str {
    match r {
        RoutingType::OwnDriver => "own_driver",
        RoutingType::Carrier   => "carrier",
        RoutingType::Auto      => "auto",
    }
}

fn routing_type_from_str(s: &str) -> RoutingType {
    match s {
        "carrier" => RoutingType::Carrier,
        "auto"    => RoutingType::Auto,
        _         => RoutingType::OwnDriver,
    }
}

/// (unit_kind, unit_ref) DB encoding of an InventoryUnit.
fn inventory_unit_parts(unit: &InventoryUnit) -> (&'static str, String) {
    match unit {
        InventoryUnit::Piece(awb)       => ("piece", awb.as_str().to_string()),
        InventoryUnit::Consolidated(id) => ("consolidated", id.inner().to_string()),
    }
}

fn inventory_unit_from_parts(kind: &str, reference: &str) -> anyhow::Result<InventoryUnit> {
    match kind {
        "piece" => Ok(InventoryUnit::Piece(
            ChildAwb::parse(reference).map_err(|e| anyhow::anyhow!("bad piece awb '{reference}': {e}"))?,
        )),
        "consolidated" => Ok(InventoryUnit::Consolidated(PalletId::from_uuid(
            Uuid::parse_str(reference).map_err(|e| anyhow::anyhow!("bad pallet id '{reference}': {e}"))?,
        ))),
        other => Err(anyhow::anyhow!("unknown inventory unit kind '{other}'")),
    }
}

// ── HubScan ───────────────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct HubScanRow {
    id:               Uuid,
    tenant_id:        Uuid,
    hub_id:           Uuid,
    piece_awb:        String,
    master_awb:       String,
    shipment_id:      Uuid,
    scan_type:        String,
    scanned_by:       Uuid,
    device_timestamp: DateTime<Utc>,
    server_timestamp: DateTime<Utc>,
    pallet_id:        Option<Uuid>,
    container_id:     Option<Uuid>,
    exception:        Option<String>,
}

impl HubScanRow {
    fn into_entity(self) -> anyhow::Result<HubScan> {
        Ok(HubScan {
            id:               self.id,
            tenant_id:        TenantId::from_uuid(self.tenant_id),
            hub_id:           HubId::from_uuid(self.hub_id),
            piece_awb:        ChildAwb::parse(&self.piece_awb).map_err(|e| anyhow::anyhow!("{e}"))?,
            master_awb:       Awb::parse(&self.master_awb).map_err(|e| anyhow::anyhow!("{e}"))?,
            shipment_id:      ShipmentId::from_uuid(self.shipment_id),
            scan_type:        scan_type_from_str(&self.scan_type),
            scanned_by:       self.scanned_by,
            device_timestamp: self.device_timestamp,
            server_timestamp: self.server_timestamp,
            pallet_id:        self.pallet_id.map(PalletId::from_uuid),
            container_id:     self.container_id.map(ContainerId::from_uuid),
            exception:        self.exception.as_deref().and_then(scan_exception_from_str),
        })
    }
}

pub struct PgHubScanRepository {
    pub pool: PgPool,
}

impl PgHubScanRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl HubScanRepository for PgHubScanRepository {
    async fn append(&self, scan: &HubScan) -> anyhow::Result<()> {
        sqlx::query(
            r#"INSERT INTO hub_ops.hub_scans
               (id, tenant_id, hub_id, piece_awb, master_awb, shipment_id, scan_type,
                scanned_by, device_timestamp, server_timestamp, pallet_id, container_id, exception)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)"#,
        )
        .bind(scan.id)
        .bind(scan.tenant_id.inner())
        .bind(scan.hub_id.inner())
        .bind(scan.piece_awb.as_str())
        .bind(scan.master_awb.as_str())
        .bind(scan.shipment_id.inner())
        .bind(scan_type_str(scan.scan_type))
        .bind(scan.scanned_by)
        .bind(scan.device_timestamp)
        .bind(scan.server_timestamp)
        .bind(scan.pallet_id.as_ref().map(|p| p.inner()))
        .bind(scan.container_id.as_ref().map(|c| c.inner()))
        .bind(scan.exception.map(scan_exception_str))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_for_shipment(
        &self,
        shipment_id: Uuid,
        tenant_id:   Uuid,
    ) -> anyhow::Result<Vec<HubScan>> {
        let rows: Vec<HubScanRow> = sqlx::query_as(
            r#"SELECT id, tenant_id, hub_id, piece_awb, master_awb, shipment_id, scan_type,
                      scanned_by, device_timestamp, server_timestamp, pallet_id, container_id, exception
               FROM hub_ops.hub_scans
               WHERE shipment_id = $1 AND tenant_id = $2
               ORDER BY server_timestamp ASC"#,
        )
        .bind(shipment_id)
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(HubScanRow::into_entity).collect()
    }
}

// ── HubLocation ───────────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct HubLocationRow {
    id:           Uuid,
    tenant_id:    Uuid,
    hub_id:       Uuid,
    zone_id:      String,
    aisle:        Option<String>,
    rack:         Option<String>,
    shelf:        Option<String>,
    bin:          Option<String>,
    location_tag: String,
    capacity_cbm: Option<f64>,
    is_active:    bool,
    created_at:   DateTime<Utc>,
}

impl From<HubLocationRow> for HubLocation {
    fn from(r: HubLocationRow) -> Self {
        HubLocation {
            id:           r.id,
            tenant_id:    TenantId::from_uuid(r.tenant_id),
            hub_id:       HubId::from_uuid(r.hub_id),
            zone_id:      r.zone_id,
            aisle:        r.aisle,
            rack:         r.rack,
            shelf:        r.shelf,
            bin:          r.bin,
            location_tag: r.location_tag,
            capacity_cbm: r.capacity_cbm,
            is_active:    r.is_active,
            created_at:   r.created_at,
        }
    }
}

pub struct PgHubLocationRepository {
    pub pool: PgPool,
}

impl PgHubLocationRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl HubLocationRepository for PgHubLocationRepository {
    async fn save(&self, location: &HubLocation) -> anyhow::Result<()> {
        sqlx::query(
            r#"INSERT INTO hub_ops.hub_locations
               (id, tenant_id, hub_id, zone_id, aisle, rack, shelf, bin,
                location_tag, capacity_cbm, is_active, created_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
               ON CONFLICT (id) DO UPDATE SET
                 zone_id      = EXCLUDED.zone_id,
                 aisle        = EXCLUDED.aisle,
                 rack         = EXCLUDED.rack,
                 shelf        = EXCLUDED.shelf,
                 bin          = EXCLUDED.bin,
                 location_tag = EXCLUDED.location_tag,
                 capacity_cbm = EXCLUDED.capacity_cbm,
                 is_active    = EXCLUDED.is_active"#,
        )
        .bind(location.id)
        .bind(location.tenant_id.inner())
        .bind(location.hub_id.inner())
        .bind(&location.zone_id)
        .bind(&location.aisle)
        .bind(&location.rack)
        .bind(&location.shelf)
        .bind(&location.bin)
        .bind(&location.location_tag)
        .bind(location.capacity_cbm)
        .bind(location.is_active)
        .bind(location.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<HubLocation>> {
        let row: Option<HubLocationRow> = sqlx::query_as(
            r#"SELECT id, tenant_id, hub_id, zone_id, aisle, rack, shelf, bin,
                      location_tag, capacity_cbm, is_active, created_at
               FROM hub_ops.hub_locations WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn list_for_hub(
        &self,
        hub_id:  Uuid,
        zone_id: Option<&str>,
    ) -> anyhow::Result<Vec<HubLocation>> {
        let rows: Vec<HubLocationRow> = match zone_id {
            Some(zone) => sqlx::query_as(
                r#"SELECT id, tenant_id, hub_id, zone_id, aisle, rack, shelf, bin,
                          location_tag, capacity_cbm, is_active, created_at
                   FROM hub_ops.hub_locations
                   WHERE hub_id = $1 AND zone_id = $2
                   ORDER BY location_tag"#,
            )
            .bind(hub_id)
            .bind(zone)
            .fetch_all(&self.pool)
            .await?,
            None => sqlx::query_as(
                r#"SELECT id, tenant_id, hub_id, zone_id, aisle, rack, shelf, bin,
                          location_tag, capacity_cbm, is_active, created_at
                   FROM hub_ops.hub_locations
                   WHERE hub_id = $1
                   ORDER BY zone_id, location_tag"#,
            )
            .bind(hub_id)
            .fetch_all(&self.pool)
            .await?,
        };
        Ok(rows.into_iter().map(Into::into).collect())
    }
}

// ── HubInventory ──────────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct HubInventoryRow {
    id:            Uuid,
    tenant_id:     Uuid,
    hub_id:        Uuid,
    location_id:   Uuid,
    unit_kind:     String,
    unit_ref:      String,
    batch_number:  Option<String>,
    checked_in_by: Uuid,
    checked_in_at: DateTime<Utc>,
    updated_at:    DateTime<Utc>,
}

impl HubInventoryRow {
    fn into_entity(self) -> anyhow::Result<HubInventory> {
        Ok(HubInventory {
            id:             self.id,
            tenant_id:      TenantId::from_uuid(self.tenant_id),
            hub_id:         HubId::from_uuid(self.hub_id),
            location_id:    self.location_id,
            inventory_unit: inventory_unit_from_parts(&self.unit_kind, &self.unit_ref)?,
            batch_number:   self.batch_number,
            checked_in_by:  self.checked_in_by,
            checked_in_at:  self.checked_in_at,
            updated_at:     self.updated_at,
        })
    }
}

pub struct PgHubInventoryRepository {
    pub pool: PgPool,
}

impl PgHubInventoryRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl HubInventoryRepository for PgHubInventoryRepository {
    async fn upsert(&self, inventory: &HubInventory) -> anyhow::Result<()> {
        let (unit_kind, unit_ref) = inventory_unit_parts(&inventory.inventory_unit);
        sqlx::query(
            r#"INSERT INTO hub_ops.hub_inventory
               (id, tenant_id, hub_id, location_id, unit_kind, unit_ref,
                batch_number, checked_in_by, checked_in_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
               ON CONFLICT (tenant_id, unit_kind, unit_ref) DO UPDATE SET
                 location_id  = EXCLUDED.location_id,
                 hub_id       = EXCLUDED.hub_id,
                 batch_number = EXCLUDED.batch_number,
                 updated_at   = EXCLUDED.updated_at"#,
        )
        .bind(inventory.id)
        .bind(inventory.tenant_id.inner())
        .bind(inventory.hub_id.inner())
        .bind(inventory.location_id)
        .bind(unit_kind)
        .bind(&unit_ref)
        .bind(&inventory.batch_number)
        .bind(inventory.checked_in_by)
        .bind(inventory.checked_in_at)
        .bind(inventory.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn move_to(&self, inventory_id: Uuid, to_location_id: Uuid) -> anyhow::Result<u64> {
        let result = sqlx::query(
            "UPDATE hub_ops.hub_inventory SET location_id = $2, updated_at = NOW() WHERE id = $1",
        )
        .bind(inventory_id)
        .bind(to_location_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    async fn list_at_location(&self, location_id: Uuid) -> anyhow::Result<Vec<HubInventory>> {
        let rows: Vec<HubInventoryRow> = sqlx::query_as(
            r#"SELECT id, tenant_id, hub_id, location_id, unit_kind, unit_ref,
                      batch_number, checked_in_by, checked_in_at, updated_at
               FROM hub_ops.hub_inventory
               WHERE location_id = $1
               ORDER BY checked_in_at"#,
        )
        .bind(location_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(HubInventoryRow::into_entity).collect()
    }

    async fn find_by_unit(
        &self,
        tenant_id: Uuid,
        unit:      &InventoryUnit,
    ) -> anyhow::Result<Option<HubInventory>> {
        let (unit_kind, unit_ref) = inventory_unit_parts(unit);
        let row: Option<HubInventoryRow> = sqlx::query_as(
            r#"SELECT id, tenant_id, hub_id, location_id, unit_kind, unit_ref,
                      batch_number, checked_in_by, checked_in_at, updated_at
               FROM hub_ops.hub_inventory
               WHERE tenant_id = $1 AND unit_kind = $2 AND unit_ref = $3"#,
        )
        .bind(tenant_id)
        .bind(unit_kind)
        .bind(&unit_ref)
        .fetch_optional(&self.pool)
        .await?;
        row.map(HubInventoryRow::into_entity).transpose()
    }
}

// ── HubTransferManifest ───────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct ManifestRow {
    id:                  Uuid,
    tenant_id:           Uuid,
    container_id:        Uuid,
    origin_hub_id:       Uuid,
    destination_hub_id:  Uuid,
    transport_mode:      String,
    carrier_booking_ref: Option<String>,
    customs_filing_ref:  Option<String>,
    customs_status:      String,
    duties_total_cents:  Option<i64>,
    broker_payload:      Option<serde_json::Value>,
    cleared_by:          Option<Uuid>,
    cleared_at:          Option<DateTime<Utc>>,
    created_at:          DateTime<Utc>,
    updated_at:          DateTime<Utc>,
}

impl From<ManifestRow> for HubTransferManifest {
    fn from(r: ManifestRow) -> Self {
        HubTransferManifest {
            id:                  r.id,
            tenant_id:           TenantId::from_uuid(r.tenant_id),
            container_id:        ContainerId::from_uuid(r.container_id),
            origin_hub_id:       HubId::from_uuid(r.origin_hub_id),
            destination_hub_id:  HubId::from_uuid(r.destination_hub_id),
            transport_mode:      transport_mode_from_str(&r.transport_mode),
            carrier_booking_ref: r.carrier_booking_ref,
            customs_filing_ref:  r.customs_filing_ref,
            customs_status:      customs_status_from_str(&r.customs_status),
            duties_total_cents:  r.duties_total_cents,
            broker_payload:      r.broker_payload,
            cleared_by:          r.cleared_by,
            cleared_at:          r.cleared_at,
            created_at:          r.created_at,
            updated_at:          r.updated_at,
        }
    }
}

pub struct PgHubTransferManifestRepository {
    pub pool: PgPool,
}

impl PgHubTransferManifestRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl HubTransferManifestRepository for PgHubTransferManifestRepository {
    async fn save(&self, m: &HubTransferManifest) -> anyhow::Result<()> {
        sqlx::query(
            r#"INSERT INTO hub_ops.hub_transfer_manifests
               (id, tenant_id, container_id, origin_hub_id, destination_hub_id, transport_mode,
                carrier_booking_ref, customs_filing_ref, customs_status, duties_total_cents,
                broker_payload, cleared_by, cleared_at, created_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
               ON CONFLICT (container_id) DO UPDATE SET
                 carrier_booking_ref = EXCLUDED.carrier_booking_ref,
                 customs_filing_ref  = EXCLUDED.customs_filing_ref,
                 customs_status      = EXCLUDED.customs_status,
                 duties_total_cents  = EXCLUDED.duties_total_cents,
                 broker_payload      = EXCLUDED.broker_payload,
                 cleared_by          = EXCLUDED.cleared_by,
                 cleared_at          = EXCLUDED.cleared_at,
                 updated_at          = EXCLUDED.updated_at"#,
        )
        .bind(m.id)
        .bind(m.tenant_id.inner())
        .bind(m.container_id.inner())
        .bind(m.origin_hub_id.inner())
        .bind(m.destination_hub_id.inner())
        .bind(transport_mode_str(m.transport_mode))
        .bind(&m.carrier_booking_ref)
        .bind(&m.customs_filing_ref)
        .bind(customs_status_str(m.customs_status))
        .bind(m.duties_total_cents)
        .bind(&m.broker_payload)
        .bind(m.cleared_by)
        .bind(m.cleared_at)
        .bind(m.created_at)
        .bind(m.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find_by_container(
        &self,
        container_id: Uuid,
    ) -> anyhow::Result<Option<HubTransferManifest>> {
        let row: Option<ManifestRow> = sqlx::query_as(
            r#"SELECT id, tenant_id, container_id, origin_hub_id, destination_hub_id, transport_mode,
                      carrier_booking_ref, customs_filing_ref, customs_status, duties_total_cents,
                      broker_payload, cleared_by, cleared_at, created_at, updated_at
               FROM hub_ops.hub_transfer_manifests WHERE container_id = $1"#,
        )
        .bind(container_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn list_in_customs(
        &self,
        hub_id:    Uuid,
        tenant_id: Uuid,
    ) -> anyhow::Result<Vec<HubTransferManifest>> {
        let rows: Vec<ManifestRow> = sqlx::query_as(
            r#"SELECT id, tenant_id, container_id, origin_hub_id, destination_hub_id, transport_mode,
                      carrier_booking_ref, customs_filing_ref, customs_status, duties_total_cents,
                      broker_payload, cleared_by, cleared_at, created_at, updated_at
               FROM hub_ops.hub_transfer_manifests
               WHERE destination_hub_id = $1 AND tenant_id = $2 AND customs_status = 'hold'
               ORDER BY updated_at"#,
        )
        .bind(hub_id)
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }
}

// ── HubRoutingConfig ──────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct RoutingRow {
    id:                        Uuid,
    tenant_id:                 Uuid,
    hub_id:                    Uuid,
    destination_zone:          String,
    routing_type:              String,
    carrier_id:                Option<Uuid>,
    auto_fallback_window_mins: i32,
    created_at:                DateTime<Utc>,
    updated_at:                DateTime<Utc>,
}

impl From<RoutingRow> for HubRoutingConfig {
    fn from(r: RoutingRow) -> Self {
        HubRoutingConfig {
            id:                        r.id,
            tenant_id:                 TenantId::from_uuid(r.tenant_id),
            hub_id:                    HubId::from_uuid(r.hub_id),
            destination_zone:          r.destination_zone,
            routing_type:              routing_type_from_str(&r.routing_type),
            carrier_id:                r.carrier_id,
            auto_fallback_window_mins: r.auto_fallback_window_mins,
            created_at:                r.created_at,
            updated_at:                r.updated_at,
        }
    }
}

pub struct PgHubRoutingConfigRepository {
    pub pool: PgPool,
}

impl PgHubRoutingConfigRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl HubRoutingConfigRepository for PgHubRoutingConfigRepository {
    async fn save(&self, c: &HubRoutingConfig) -> anyhow::Result<()> {
        sqlx::query(
            r#"INSERT INTO hub_ops.hub_routing_configs
               (id, tenant_id, hub_id, destination_zone, routing_type, carrier_id,
                auto_fallback_window_mins, created_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
               ON CONFLICT (tenant_id, hub_id, destination_zone) DO UPDATE SET
                 routing_type              = EXCLUDED.routing_type,
                 carrier_id                = EXCLUDED.carrier_id,
                 auto_fallback_window_mins = EXCLUDED.auto_fallback_window_mins,
                 updated_at                = EXCLUDED.updated_at"#,
        )
        .bind(c.id)
        .bind(c.tenant_id.inner())
        .bind(c.hub_id.inner())
        .bind(&c.destination_zone)
        .bind(routing_type_str(c.routing_type))
        .bind(c.carrier_id)
        .bind(c.auto_fallback_window_mins)
        .bind(c.created_at)
        .bind(c.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn find(
        &self,
        tenant_id:        Uuid,
        hub_id:           Uuid,
        destination_zone: &str,
    ) -> anyhow::Result<Option<HubRoutingConfig>> {
        let row: Option<RoutingRow> = sqlx::query_as(
            r#"SELECT id, tenant_id, hub_id, destination_zone, routing_type, carrier_id,
                      auto_fallback_window_mins, created_at, updated_at
               FROM hub_ops.hub_routing_configs
               WHERE tenant_id = $1 AND hub_id = $2 AND destination_zone = $3"#,
        )
        .bind(tenant_id)
        .bind(hub_id)
        .bind(destination_zone)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn list_for_hub(
        &self,
        hub_id:    Uuid,
        tenant_id: Uuid,
    ) -> anyhow::Result<Vec<HubRoutingConfig>> {
        let rows: Vec<RoutingRow> = sqlx::query_as(
            r#"SELECT id, tenant_id, hub_id, destination_zone, routing_type, carrier_id,
                      auto_fallback_window_mins, created_at, updated_at
               FROM hub_ops.hub_routing_configs
               WHERE hub_id = $1 AND tenant_id = $2
               ORDER BY destination_zone"#,
        )
        .bind(hub_id)
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }
}
