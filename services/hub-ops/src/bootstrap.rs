use std::{net::SocketAddr, sync::Arc};
use anyhow::Context;
use sqlx::postgres::PgPoolOptions;
use axum::{
    extract::{Path, Query, State, ws::{Message, WebSocket, WebSocketUpgrade}},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use uuid::Uuid;

use logisticos_auth::jwt::JwtService;
use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::rbac::permissions;
use logisticos_errors::AppError;

use logisticos_events::producer::KafkaProducer;

use crate::{
    application::services::{
        CreateHubCommand, HubRepository, HubService, InductParcelCommand,
        InductionRepository, SortParcelCommand,
        ConsolidationService,
        consolidation_service::{
            ComputePlanCommand, CreateSpecCommand, ConsolidationPlanRepository,
            TruckSpecRepository, UpdatePlacementsBody, UpdateSpecCommand,
        },
        PalletService,
        pallet_service::{
            ArriveContainerCommand, LoadPieceCommand,
            SealPalletCommand, LoadPalletIntoContainerCommand,
        },
        HubTransferService,
        hub_transfer_service::{ClearCustomsCommand, DeconsolidateCommand},
    },
    domain::entities::{
        consolidation::{ConsolidationPlan, TruckSpec},
        HubInventory, HubLocation, HubRoutingConfig, HubScan, InventoryUnit, RoutingType,
        ScanException, ScanType,
    },
    infrastructure::{
        hub_ws::HubBroadcaster,
        db::{
            PgPalletRepository, PgPalletContainerRepository,
            hub_transfer::{
                PgHubInventoryRepository, PgHubLocationRepository, PgHubRoutingConfigRepository,
                PgHubScanRepository, PgHubTransferManifestRepository,
            },
        },
    },
    config::Config,
};

// ---------------------------------------------------------------------------
// PostgreSQL repositories (inline for hub-ops — small surface area)
// ---------------------------------------------------------------------------

struct PgHubRepository { pool: sqlx::PgPool }
struct PgInductionRepository { pool: sqlx::PgPool }

#[derive(sqlx::FromRow)]
struct HubRow {
    id: Uuid, tenant_id: Uuid, name: String, address: String,
    lat: f64, lng: f64, capacity: i32, current_load: i32,
    serving_zones: Vec<String>, is_active: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow)]
struct InductionRow {
    id: Uuid, hub_id: Uuid, tenant_id: Uuid, shipment_id: Uuid,
    tracking_number: String, status: String,
    zone: Option<String>, bay: Option<String>, inducted_by: Option<Uuid>,
    inducted_at: chrono::DateTime<chrono::Utc>,
    sorted_at: Option<chrono::DateTime<chrono::Utc>>,
    dispatched_at: Option<chrono::DateTime<chrono::Utc>>,
}

fn row_to_hub(r: HubRow) -> crate::domain::entities::Hub {
    use crate::domain::entities::{Hub, HubId};
    use logisticos_types::TenantId;
    Hub {
        id: HubId::from_uuid(r.id), tenant_id: TenantId::from_uuid(r.tenant_id),
        name: r.name, address: r.address, lat: r.lat, lng: r.lng,
        capacity: r.capacity as u32, current_load: r.current_load as u32,
        serving_zones: r.serving_zones, is_active: r.is_active,
        created_at: r.created_at, updated_at: r.updated_at,
    }
}

fn row_to_induction(r: InductionRow) -> crate::domain::entities::ParcelInduction {
    use crate::domain::entities::{HubId, InductionId, InductionStatus, ParcelInduction};
    use logisticos_types::TenantId;
    let status = match r.status.as_str() {
        "sorted"     => InductionStatus::Sorted,
        "dispatched" => InductionStatus::Dispatched,
        "returned"   => InductionStatus::Returned,
        _            => InductionStatus::Inducted,
    };
    ParcelInduction {
        id: InductionId::from_uuid(r.id),
        hub_id: HubId::from_uuid(r.hub_id),
        tenant_id: TenantId::from_uuid(r.tenant_id),
        shipment_id: r.shipment_id,
        tracking_number: r.tracking_number,
        status,
        zone: r.zone,
        bay: r.bay,
        inducted_by: r.inducted_by,
        inducted_at: r.inducted_at,
        sorted_at: r.sorted_at,
        dispatched_at: r.dispatched_at,
    }
}

#[async_trait::async_trait]
impl HubRepository for PgHubRepository {
    async fn find_by_id(&self, id: &crate::domain::entities::HubId) -> anyhow::Result<Option<crate::domain::entities::Hub>> {
        let row = sqlx::query_as::<_, HubRow>(
            r#"SELECT id, tenant_id, name, address, lat, lng, capacity, current_load,
                      serving_zones, is_active, created_at, updated_at
               FROM hub_ops.hubs WHERE id = $1"#
        ).bind(id.inner()).fetch_optional(&self.pool).await?;
        Ok(row.map(row_to_hub))
    }
    async fn list(&self, tenant_id: &logisticos_types::TenantId) -> anyhow::Result<Vec<crate::domain::entities::Hub>> {
        let rows = sqlx::query_as::<_, HubRow>(
            r#"SELECT id, tenant_id, name, address, lat, lng, capacity, current_load,
                      serving_zones, is_active, created_at, updated_at
               FROM hub_ops.hubs WHERE tenant_id = $1 AND is_active = true
               ORDER BY name"#
        ).bind(tenant_id.inner()).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(row_to_hub).collect())
    }
    async fn save(&self, hub: &crate::domain::entities::Hub) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO hub_ops.hubs (
                id, tenant_id, name, address, lat, lng, capacity, current_load,
                serving_zones, is_active, created_at, updated_at
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
            ON CONFLICT (id) DO UPDATE SET
                current_load = EXCLUDED.current_load,
                serving_zones = EXCLUDED.serving_zones,
                is_active = EXCLUDED.is_active,
                updated_at = EXCLUDED.updated_at
            "#
        )
        .bind(hub.id.inner()).bind(hub.tenant_id.inner()).bind(&hub.name).bind(&hub.address)
        .bind(hub.lat).bind(hub.lng).bind(hub.capacity as i32).bind(hub.current_load as i32)
        .bind(&hub.serving_zones).bind(hub.is_active).bind(hub.created_at).bind(hub.updated_at)
        .execute(&self.pool).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl InductionRepository for PgInductionRepository {
    async fn find_by_id(&self, id: &crate::domain::entities::InductionId) -> anyhow::Result<Option<crate::domain::entities::ParcelInduction>> {
        let row = sqlx::query_as::<_, InductionRow>(
            r#"SELECT id, hub_id, tenant_id, shipment_id, tracking_number, status,
                      zone, bay, inducted_by, inducted_at, sorted_at, dispatched_at
               FROM hub_ops.parcel_inductions WHERE id = $1"#
        ).bind(id.inner()).fetch_optional(&self.pool).await?;
        Ok(row.map(row_to_induction))
    }
    async fn find_by_shipment(&self, shipment_id: Uuid) -> anyhow::Result<Option<crate::domain::entities::ParcelInduction>> {
        let row = sqlx::query_as::<_, InductionRow>(
            r#"SELECT id, hub_id, tenant_id, shipment_id, tracking_number, status,
                      zone, bay, inducted_by, inducted_at, sorted_at, dispatched_at
               FROM hub_ops.parcel_inductions WHERE shipment_id = $1
               LIMIT 1"#
        ).bind(shipment_id).fetch_optional(&self.pool).await?;
        Ok(row.map(row_to_induction))
    }
    async fn list_active(&self, hub_id: &crate::domain::entities::HubId) -> anyhow::Result<Vec<crate::domain::entities::ParcelInduction>> {
        let rows = sqlx::query_as::<_, InductionRow>(
            r#"SELECT id, hub_id, tenant_id, shipment_id, tracking_number, status,
                      zone, bay, inducted_by, inducted_at, sorted_at, dispatched_at
               FROM hub_ops.parcel_inductions
               WHERE hub_id = $1 AND status IN ('inducted', 'sorted')
               ORDER BY inducted_at DESC"#
        ).bind(hub_id.inner()).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(row_to_induction).collect())
    }
    async fn save(&self, i: &crate::domain::entities::ParcelInduction) -> anyhow::Result<()> {
        let status = serde_json::to_value(&i.status)?.as_str().unwrap_or("inducted").to_owned();
        sqlx::query(
            r#"
            INSERT INTO hub_ops.parcel_inductions (
                id, hub_id, tenant_id, shipment_id, tracking_number, status,
                zone, bay, inducted_by, inducted_at, sorted_at, dispatched_at
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
            ON CONFLICT (id) DO UPDATE SET
                status = EXCLUDED.status, zone = EXCLUDED.zone, bay = EXCLUDED.bay,
                sorted_at = EXCLUDED.sorted_at, dispatched_at = EXCLUDED.dispatched_at
            "#
        )
        .bind(i.id.inner()).bind(i.hub_id.inner()).bind(i.tenant_id.inner()).bind(i.shipment_id)
        .bind(&i.tracking_number).bind(status).bind(&i.zone).bind(&i.bay).bind(i.inducted_by)
        .bind(i.inducted_at).bind(i.sorted_at).bind(i.dispatched_at)
        .execute(&self.pool).await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Container repository (minimal — find by ID + save departure timestamp)
// ---------------------------------------------------------------------------

struct PgContainerRepository { pool: sqlx::PgPool }

#[derive(sqlx::FromRow)]
struct ContainerRow {
    id:               Uuid,
    tenant_id:        Uuid,
    status:           String,
    departed_at:      Option<chrono::DateTime<chrono::Utc>>,
    estimated_arrival: Option<chrono::DateTime<chrono::Utc>>,
    // master_awbs TEXT[] — we use these to look up shipment IDs via inductions.
    master_awbs:      Vec<String>,
}

impl PgContainerRepository {
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<ContainerRow>> {
        let row = sqlx::query_as::<_, ContainerRow>(
            "SELECT id, tenant_id, status, departed_at, estimated_arrival, master_awbs
               FROM hub_ops.containers WHERE id = $1"
        ).bind(id).fetch_optional(&self.pool).await?;
        Ok(row)
    }

    /// Returns all shipment IDs for parcels in this container by joining
    /// through `hub_ops.parcel_inductions` on `tracking_number = master_awb`.
    async fn shipment_ids_for_container(&self, id: Uuid) -> anyhow::Result<Vec<Uuid>> {
        let rows: Vec<(Uuid,)> = sqlx::query_as(
            r#"SELECT DISTINCT i.shipment_id
               FROM hub_ops.parcel_inductions i
              WHERE i.tracking_number = ANY(
                  SELECT unnest(master_awbs) FROM hub_ops.containers WHERE id = $1
              )"#
        ).bind(id).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(|(sid,)| sid).collect())
    }

    async fn mark_departed(
        &self,
        id:  Uuid,
        eta: Option<chrono::DateTime<chrono::Utc>>,
    ) -> anyhow::Result<()> {
        let now = chrono::Utc::now();
        let affected = sqlx::query(
            "UPDATE hub_ops.containers
                SET status = 'in_transit', departed_at = $1, estimated_arrival = $2, updated_at = $3
              WHERE id = $4 AND status IN ('manifested', 'sealed')"
        )
        .bind(now)
        .bind(eta)
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await?
        .rows_affected();

        if affected == 0 {
            anyhow::bail!(
                "Container {} cannot depart: not found or not in manifested/sealed status", id
            );
        }
        Ok(())
    }

    async fn create(
        &self,
        id:                 Uuid,
        tenant_id:          Uuid,
        transport_mode:     &str,
        origin_hub_id:      Uuid,
        destination_hub_id: Uuid,
        carrier_ref:        Option<&str>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"INSERT INTO hub_ops.containers
               (id, tenant_id, transport_mode, carrier_ref, origin_hub_id, destination_hub_id,
                status, master_awbs, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, 'planning', '{}', NOW(), NOW())"#
        )
        .bind(id).bind(tenant_id).bind(transport_mode).bind(carrier_ref)
        .bind(origin_hub_id).bind(destination_hub_id)
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn resolve_awbs_to_shipment_ids(&self, master_awbs: &[String]) -> anyhow::Result<Vec<Uuid>> {
        if master_awbs.is_empty() { return Ok(vec![]); }
        let rows: Vec<(Uuid,)> = sqlx::query_as(
            "SELECT DISTINCT shipment_id FROM hub_ops.parcel_inductions WHERE tracking_number = ANY($1)"
        ).bind(master_awbs).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    async fn add_pallet(
        &self,
        container_id: Uuid,
        pallet_id:    Uuid,
        tenant_id:    Uuid,
        master_awbs:  &[String],
    ) -> anyhow::Result<()> {
        let status: Option<(String,)> = sqlx::query_as(
            "SELECT status FROM hub_ops.containers WHERE id = $1"
        ).bind(container_id).fetch_optional(&self.pool).await?;

        match status {
            None => anyhow::bail!("Container {} not found", container_id),
            Some((s,)) if matches!(s.as_str(), "in_transit" | "delivered" | "customs" | "released") => {
                anyhow::bail!("Container {} is '{}' and cannot be modified", container_id, s);
            }
            _ => {}
        }

        let already_loaded: Option<(bool,)> = sqlx::query_as(
            "SELECT true FROM hub_ops.container_pallets WHERE container_id = $1 AND pallet_id = $2"
        ).bind(container_id).bind(pallet_id).fetch_optional(&self.pool).await?;
        if already_loaded.is_some() {
            anyhow::bail!("Pallet {} is already loaded in container {}", pallet_id, container_id);
        }

        sqlx::query(
            "INSERT INTO hub_ops.container_pallets (container_id, pallet_id, tenant_id) VALUES ($1, $2, $3)"
        ).bind(container_id).bind(pallet_id).bind(tenant_id).execute(&self.pool).await?;

        if !master_awbs.is_empty() {
            sqlx::query(
                r#"UPDATE hub_ops.containers
                   SET master_awbs = ARRAY(SELECT DISTINCT unnest(array_cat(master_awbs, $1::text[]))),
                       updated_at  = NOW()
                   WHERE id = $2"#
            ).bind(master_awbs).bind(container_id).execute(&self.pool).await?;
        }
        Ok(())
    }

    async fn add_loose_piece(
        &self,
        container_id: Uuid,
        tenant_id:    Uuid,
        piece_awb:    &str,
        master_awb:   &str,
    ) -> anyhow::Result<()> {
        let status: Option<(String,)> = sqlx::query_as(
            "SELECT status FROM hub_ops.containers WHERE id = $1"
        ).bind(container_id).fetch_optional(&self.pool).await?;

        match status {
            None => anyhow::bail!("Container {} not found", container_id),
            Some((s,)) if matches!(s.as_str(), "in_transit" | "delivered" | "customs" | "released") => {
                anyhow::bail!("Container {} is '{}' and cannot be modified", container_id, s);
            }
            _ => {}
        }

        sqlx::query(
            "INSERT INTO hub_ops.container_loose_pieces (container_id, tenant_id, piece_awb) VALUES ($1, $2, $3)"
        ).bind(container_id).bind(tenant_id).bind(piece_awb).execute(&self.pool).await?;

        sqlx::query(
            r#"UPDATE hub_ops.containers
               SET master_awbs = CASE WHEN NOT ($1 = ANY(master_awbs))
                                      THEN array_append(master_awbs, $1)
                                      ELSE master_awbs END,
                   updated_at  = NOW()
               WHERE id = $2"#
        ).bind(master_awb).bind(container_id).execute(&self.pool).await?;

        Ok(())
    }

    async fn finalise(&self, container_id: Uuid) -> anyhow::Result<()> {
        let has_pallet: Option<(bool,)> = sqlx::query_as(
            "SELECT true FROM hub_ops.container_pallets WHERE container_id = $1 LIMIT 1"
        ).bind(container_id).fetch_optional(&self.pool).await?;

        let has_piece: Option<(bool,)> = sqlx::query_as(
            "SELECT true FROM hub_ops.container_loose_pieces WHERE container_id = $1 LIMIT 1"
        ).bind(container_id).fetch_optional(&self.pool).await?;

        if has_pallet.is_none() && has_piece.is_none() {
            anyhow::bail!("Container {} is empty — load at least one pallet or piece before finalising", container_id);
        }

        let affected = sqlx::query(
            "UPDATE hub_ops.containers SET status = 'manifested', updated_at = NOW() WHERE id = $1 AND status = 'planning'"
        ).bind(container_id).execute(&self.pool).await?.rows_affected();

        if affected == 0 {
            anyhow::bail!("Container {} cannot be finalised: not found or not in 'planning' status", container_id);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Truck spec repository
// ---------------------------------------------------------------------------

struct PgTruckSpecRepository { pool: sqlx::PgPool }

#[derive(sqlx::FromRow)]
struct TruckSpecRow {
    id: Uuid, tenant_id: Uuid, name: String,
    transport_mode: String, size_class: String,
    interior_length_cm: i32, interior_width_cm: i32, interior_height_cm: i32,
    max_payload_kg: f64,
    is_active: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

fn row_to_spec(r: TruckSpecRow) -> TruckSpec {
    TruckSpec {
        id: r.id, tenant_id: r.tenant_id, name: r.name,
        transport_mode: r.transport_mode, size_class: r.size_class,
        interior_length_cm: r.interior_length_cm,
        interior_width_cm:  r.interior_width_cm,
        interior_height_cm: r.interior_height_cm,
        max_payload_kg: r.max_payload_kg,
        is_active: r.is_active,
        created_at: r.created_at, updated_at: r.updated_at,
    }
}

#[async_trait::async_trait]
impl TruckSpecRepository for PgTruckSpecRepository {
    async fn list(&self, tenant_id: Uuid) -> anyhow::Result<Vec<TruckSpec>> {
        let rows = sqlx::query_as::<_, TruckSpecRow>(
            r#"SELECT id, tenant_id, name, transport_mode, size_class,
                      interior_length_cm, interior_width_cm, interior_height_cm,
                      max_payload_kg::float8, is_active, created_at, updated_at
               FROM hub_ops.truck_specs
               WHERE tenant_id = $1 AND is_active = true
               ORDER BY name"#
        ).bind(tenant_id).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(row_to_spec).collect())
    }

    async fn find(&self, id: Uuid, tenant_id: Uuid) -> anyhow::Result<Option<TruckSpec>> {
        let row = sqlx::query_as::<_, TruckSpecRow>(
            r#"SELECT id, tenant_id, name, transport_mode, size_class,
                      interior_length_cm, interior_width_cm, interior_height_cm,
                      max_payload_kg::float8, is_active, created_at, updated_at
               FROM hub_ops.truck_specs
               WHERE id = $1 AND tenant_id = $2"#
        ).bind(id).bind(tenant_id).fetch_optional(&self.pool).await?;
        Ok(row.map(row_to_spec))
    }

    async fn create(&self, spec: &TruckSpec) -> anyhow::Result<TruckSpec> {
        sqlx::query(
            r#"INSERT INTO hub_ops.truck_specs (
                id, tenant_id, name, transport_mode, size_class,
                interior_length_cm, interior_width_cm, interior_height_cm,
                max_payload_kg, is_active, created_at, updated_at
               ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)"#
        )
        .bind(spec.id).bind(spec.tenant_id).bind(&spec.name)
        .bind(&spec.transport_mode).bind(&spec.size_class)
        .bind(spec.interior_length_cm).bind(spec.interior_width_cm).bind(spec.interior_height_cm)
        .bind(spec.max_payload_kg).bind(spec.is_active)
        .bind(spec.created_at).bind(spec.updated_at)
        .execute(&self.pool).await?;
        Ok(spec.clone())
    }

    async fn update(&self, spec: &TruckSpec) -> anyhow::Result<TruckSpec> {
        sqlx::query(
            r#"UPDATE hub_ops.truck_specs SET
                name = $1, interior_length_cm = $2, interior_width_cm = $3,
                interior_height_cm = $4, max_payload_kg = $5, is_active = $6, updated_at = $7
               WHERE id = $8 AND tenant_id = $9"#
        )
        .bind(&spec.name).bind(spec.interior_length_cm).bind(spec.interior_width_cm)
        .bind(spec.interior_height_cm).bind(spec.max_payload_kg)
        .bind(spec.is_active).bind(spec.updated_at)
        .bind(spec.id).bind(spec.tenant_id)
        .execute(&self.pool).await?;
        Ok(spec.clone())
    }
}

// ---------------------------------------------------------------------------
// Consolidation plan repository
// ---------------------------------------------------------------------------

struct PgConsolidationPlanRepository { pool: sqlx::PgPool }

#[derive(sqlx::FromRow)]
struct PlanRow {
    id: Uuid, tenant_id: Uuid, hub_id: Uuid, truck_spec_id: Uuid,
    container_id: Option<Uuid>,
    items: serde_json::Value, placements: serde_json::Value, unplaced: serde_json::Value,
    total_weight_kg: f64,
    volume_used_cm3: i64, volume_total_cm3: i64,
    piece_count: i32,
    computed_at: chrono::DateTime<chrono::Utc>,
    created_at:  chrono::DateTime<chrono::Utc>,
    updated_at:  chrono::DateTime<chrono::Utc>,
}

fn row_to_plan(r: PlanRow) -> ConsolidationPlan {
    ConsolidationPlan {
        id: r.id, tenant_id: r.tenant_id, hub_id: r.hub_id,
        truck_spec_id: r.truck_spec_id, container_id: r.container_id,
        items: r.items, placements: r.placements, unplaced: r.unplaced,
        total_weight_kg: r.total_weight_kg,
        volume_used_cm3:  r.volume_used_cm3,
        volume_total_cm3: r.volume_total_cm3,
        piece_count: r.piece_count, computed_at: r.computed_at,
        created_at: r.created_at, updated_at: r.updated_at,
    }
}

#[async_trait::async_trait]
impl ConsolidationPlanRepository for PgConsolidationPlanRepository {
    async fn list(&self, hub_id: Uuid, tenant_id: Uuid) -> anyhow::Result<Vec<ConsolidationPlan>> {
        let rows = sqlx::query_as::<_, PlanRow>(
            r#"SELECT id, tenant_id, hub_id, truck_spec_id, container_id,
                      items, placements, unplaced,
                      total_weight_kg::float8, volume_used_cm3, volume_total_cm3,
                      piece_count, computed_at, created_at, updated_at
               FROM hub_ops.consolidation_plans
               WHERE hub_id = $1 AND tenant_id = $2
               ORDER BY created_at DESC
               LIMIT 20"#
        ).bind(hub_id).bind(tenant_id).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(row_to_plan).collect())
    }

    async fn find(&self, id: Uuid, tenant_id: Uuid) -> anyhow::Result<Option<ConsolidationPlan>> {
        let row = sqlx::query_as::<_, PlanRow>(
            r#"SELECT id, tenant_id, hub_id, truck_spec_id, container_id,
                      items, placements, unplaced,
                      total_weight_kg::float8, volume_used_cm3, volume_total_cm3,
                      piece_count, computed_at, created_at, updated_at
               FROM hub_ops.consolidation_plans
               WHERE id = $1 AND tenant_id = $2"#
        ).bind(id).bind(tenant_id).fetch_optional(&self.pool).await?;
        Ok(row.map(row_to_plan))
    }

    async fn upsert(&self, plan: &ConsolidationPlan) -> anyhow::Result<ConsolidationPlan> {
        sqlx::query(
            r#"INSERT INTO hub_ops.consolidation_plans (
                id, tenant_id, hub_id, truck_spec_id, container_id,
                items, placements, unplaced,
                total_weight_kg, volume_used_cm3, volume_total_cm3,
                piece_count, computed_at, created_at, updated_at
               ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
               ON CONFLICT (id) DO UPDATE SET
                placements       = EXCLUDED.placements,
                unplaced         = EXCLUDED.unplaced,
                total_weight_kg  = EXCLUDED.total_weight_kg,
                volume_used_cm3  = EXCLUDED.volume_used_cm3,
                volume_total_cm3 = EXCLUDED.volume_total_cm3,
                piece_count      = EXCLUDED.piece_count,
                computed_at      = EXCLUDED.computed_at,
                updated_at       = EXCLUDED.updated_at"#
        )
        .bind(plan.id).bind(plan.tenant_id).bind(plan.hub_id).bind(plan.truck_spec_id)
        .bind(plan.container_id)
        .bind(&plan.items).bind(&plan.placements).bind(&plan.unplaced)
        .bind(plan.total_weight_kg).bind(plan.volume_used_cm3).bind(plan.volume_total_cm3)
        .bind(plan.piece_count).bind(plan.computed_at).bind(plan.created_at).bind(plan.updated_at)
        .execute(&self.pool).await?;
        Ok(plan.clone())
    }

    async fn update_placements(
        &self,
        id: Uuid,
        tenant_id: Uuid,
        placements: serde_json::Value,
    ) -> anyhow::Result<ConsolidationPlan> {
        let now = chrono::Utc::now();
        sqlx::query(
            "UPDATE hub_ops.consolidation_plans
                SET placements = $1, updated_at = $2
              WHERE id = $3 AND tenant_id = $4"
        )
        .bind(&placements).bind(now).bind(id).bind(tenant_id)
        .execute(&self.pool).await?;
        self.find(id, tenant_id).await?
            .ok_or_else(|| anyhow::anyhow!("plan not found after placements update"))
    }
}

// ---------------------------------------------------------------------------
// Billing clearance HTTP client
// ---------------------------------------------------------------------------

/// Calls payments service `GET /v1/internal/billing-clearance?shipment_ids=...`
/// Returns (all_cleared, blocked_shipment_ids, unpaid_invoice_ids).
async fn check_billing_clearance(
    payments_url: &str,
    shipment_ids: &[Uuid],
) -> anyhow::Result<(bool, Vec<Uuid>, Vec<Uuid>)> {
    if payments_url.is_empty() || shipment_ids.is_empty() {
        return Ok((true, vec![], vec![]));
    }

    let client = reqwest::Client::new();

    // Build query string: ?shipment_ids=uuid&shipment_ids=uuid...
    let qs: String = shipment_ids.iter()
        .map(|id| format!("shipment_ids={id}"))
        .collect::<Vec<_>>()
        .join("&");

    let url = format!("{payments_url}/v1/internal/billing-clearance?{qs}");

    let resp = client.get(&url).send().await
        .map_err(|e| anyhow::anyhow!("Failed to call payments billing-clearance: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        anyhow::bail!("Payments billing-clearance returned HTTP {status}");
    }

    let body: serde_json::Value = resp.json().await
        .map_err(|e| anyhow::anyhow!("Failed to parse billing-clearance response: {e}"))?;

    let all_cleared = body["all_cleared"].as_bool().unwrap_or(true);

    let mut blocked_shipments = vec![];
    let mut unpaid_invoices   = vec![];
    if let Some(clearances) = body["clearances"].as_array() {
        for c in clearances {
            if !c["is_cleared"].as_bool().unwrap_or(true) {
                if let Some(sid) = c["shipment_id"].as_str().and_then(|s| s.parse().ok()) {
                    blocked_shipments.push(sid);
                }
                if let Some(invoice_ids) = c["unpaid_invoice_ids"].as_array() {
                    for iid in invoice_ids {
                        if let Some(iid_str) = iid.as_str().and_then(|s| s.parse().ok()) {
                            unpaid_invoices.push(iid_str);
                        }
                    }
                }
            }
        }
    }

    Ok((all_cleared, blocked_shipments, unpaid_invoices))
}

// ---------------------------------------------------------------------------
// POP status HTTP client
// ---------------------------------------------------------------------------

/// Calls pod service `GET /v1/internal/pop-status?shipment_ids=...`
/// Returns (all_completed, shipment_ids_missing_pop).
///
/// If the pod service returns 404 the `/v1/internal/pop-status` route does not
/// exist on that image (pre-POP deployment). In that case we emit a warning and
/// return `(true, [])` — allowing the load to proceed rather than blocking all
/// container operations with a 500. A proper POP gate is enforced once the pod
/// image is redeployed with the POP endpoints.
async fn check_pop_status(
    pod_url:      &str,
    shipment_ids: &[Uuid],
) -> anyhow::Result<(bool, Vec<Uuid>)> {
    if pod_url.is_empty() || shipment_ids.is_empty() {
        return Ok((true, vec![]));
    }

    let client = reqwest::Client::new();
    let qs: String = shipment_ids.iter()
        .map(|id| format!("shipment_ids={id}"))
        .collect::<Vec<_>>()
        .join("&");

    let url = format!("{pod_url}/v1/internal/pop-status?{qs}");

    let resp = client.get(&url).send().await
        .map_err(|e| anyhow::anyhow!("Failed to call pod pop-status: {e}"))?;

    let status = resp.status();

    // 404 means the pod image pre-dates the POP routing fix.
    // Degrade gracefully: allow the load but emit a prominent warning.
    if status == reqwest::StatusCode::NOT_FOUND {
        tracing::warn!(
            pod_url = %pod_url,
            "Pod service returned 404 on /v1/internal/pop-status — \
             image is outdated and does not have POP endpoints. \
             POP gate bypassed. Redeploy the pod service to enforce POP checks."
        );
        return Ok((true, vec![]));
    }

    if !status.is_success() {
        anyhow::bail!("Pod service pop-status returned HTTP {status}");
    }

    let body: serde_json::Value = resp.json().await
        .map_err(|e| anyhow::anyhow!("Failed to parse pop-status response: {e}"))?;

    let all_completed = body["all_completed"].as_bool().unwrap_or(true);
    let mut missing_pop = vec![];

    if let Some(statuses) = body["statuses"].as_array() {
        for s in statuses {
            if !s["has_completed_pop"].as_bool().unwrap_or(true) {
                if let Some(sid) = s["shipment_id"].as_str().and_then(|s| s.parse().ok()) {
                    missing_pop.push(sid);
                }
            }
        }
    }

    Ok((all_completed, missing_pop))
}

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct AppState {
    svc:               Arc<HubService>,
    containers:        Arc<PgContainerRepository>,
    consolidation_svc: Arc<ConsolidationService>,
    pallet_svc:        Arc<PalletService>,
    hub_transfer_svc:  Arc<HubTransferService>,
    hub_broadcaster:   HubBroadcaster,
    payments_url:      String,
    pod_url:           String,
    /// Raw pool kept for ad-hoc analytical queries (e.g. throughput buckets)
    /// that don't fit neatly behind a domain repository trait.
    pool:              sqlx::PgPool,
}

// ---------------------------------------------------------------------------
// HTTP handlers
// ---------------------------------------------------------------------------

async fn create_hub(State(s): State<AppState>, claims: AuthClaims, Json(cmd): Json<CreateHubCommand>) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::FLEET_MANAGE)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let hub = s.svc.create_hub(&tenant_id, cmd).await?;
    Ok::<_, AppError>((StatusCode::CREATED, Json(hub)))
}

// ---------------------------------------------------------------------------
// Container departure handler
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct DepartRequest {
    #[serde(default)]
    eta: Option<chrono::DateTime<chrono::Utc>>,
}

/// `PUT /v1/containers/:id/depart`
///
/// Hard billing clearance gate: if any shipment in the container has an
/// outstanding (draft or issued) per-shipment invoice in the payments service,
/// the departure is blocked and HTTP 402 is returned with the unpaid invoice IDs.
///
/// If `PAYMENTS__URL` is not configured, the clearance check is skipped and
/// departure is always allowed (graceful fallback for environments without
/// payments service).
async fn depart_container(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(container_id): Path<Uuid>,
    Json(req): Json<DepartRequest>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_UPDATE)?;

    let container = s.containers.find_by_id(container_id).await
        .map_err(|e| AppError::Internal(e))?
        .ok_or_else(|| AppError::NotFound { resource: "container", id: container_id.to_string() })?;

    // Billing clearance guard — only applies to sea/air containers where Balikbayan
    // boxes are loaded (Track A billing). Road containers for last-mile skip this.
    if !s.payments_url.is_empty() {
        let shipment_ids = s.containers.shipment_ids_for_container(container_id).await
            .map_err(|e| AppError::Internal(e))?;

        if !shipment_ids.is_empty() {
            let (all_cleared, blocked_shipments, unpaid_invoice_ids) =
                check_billing_clearance(&s.payments_url, &shipment_ids).await
                    .map_err(|e| AppError::Internal(e))?;

            if !all_cleared {
                tracing::warn!(
                    container_id    = %container_id,
                    blocked_count   = %blocked_shipments.len(),
                    "Container departure blocked — unpaid invoices outstanding"
                );
                return Ok::<_, AppError>((
                    StatusCode::PAYMENT_REQUIRED,
                    Json(serde_json::json!({
                        "error": "BILLING_CLEARANCE_REQUIRED",
                        "message": "Container cannot depart: some shipments have outstanding invoices.",
                        "blocked_shipment_ids": blocked_shipments,
                        "unpaid_invoice_ids":   unpaid_invoice_ids,
                    })),
                ));
            }
        }
    }

    // All clear — mark the container as departed.
    s.containers.mark_departed(container_id, req.eta).await
        .map_err(|e| AppError::BusinessRule(e.to_string()))?;

    tracing::info!(
        container_id = %container_id,
        tenant_id    = %container.tenant_id,
        "Container departed"
    );

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "container_id": container_id,
            "status":       "in_transit",
            "departed_at":  chrono::Utc::now(),
        })),
    ))
}

// ---------------------------------------------------------------------------
// Container management handlers (create / load / finalise)
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct CreateContainerRequest {
    /// "road" | "sea" | "air"
    transport_mode:     String,
    origin_hub_id:      Uuid,
    destination_hub_id: Uuid,
    carrier_ref:        Option<String>,
}

/// `POST /v1/containers` — create a new container in Planning state.
async fn create_container_handler(
    State(s): State<AppState>,
    claims: AuthClaims,
    Json(req): Json<CreateContainerRequest>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_UPDATE)?;

    if !["road", "sea", "air"].contains(&req.transport_mode.as_str()) {
        return Err(AppError::Validation(
            "transport_mode must be 'road', 'sea', or 'air'".into(),
        ));
    }

    let id = Uuid::new_v4();
    s.containers.create(
        id, claims.tenant_id, &req.transport_mode,
        req.origin_hub_id, req.destination_hub_id,
        req.carrier_ref.as_deref(),
    ).await.map_err(AppError::internal)?;

    tracing::info!(container_id = %id, transport = %req.transport_mode, "Container created");

    Ok::<_, AppError>((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "container_id":       id,
            "status":             "planning",
            "transport_mode":     req.transport_mode,
            "origin_hub_id":      req.origin_hub_id,
            "destination_hub_id": req.destination_hub_id,
            "carrier_ref":        req.carrier_ref,
        })),
    ))
}

#[derive(serde::Deserialize)]
struct LoadPalletRequest {
    pallet_id:   Uuid,
    /// Master AWBs carried by this pallet — used to resolve shipment IDs for guards.
    master_awbs: Vec<String>,
}

/// `POST /v1/containers/:id/load-pallet`
///
/// Billing clearance gate: blocks if any pallet AWB has an outstanding invoice.
/// POP status gate: blocks if any shipment on the pallet is missing a completed POP.
async fn load_pallet_handler(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(container_id): Path<Uuid>,
    Json(req): Json<LoadPalletRequest>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_UPDATE)?;

    let shipment_ids = s.containers
        .resolve_awbs_to_shipment_ids(&req.master_awbs)
        .await
        .map_err(AppError::internal)?;

    if !s.payments_url.is_empty() && !shipment_ids.is_empty() {
        let (all_cleared, blocked_shipments, unpaid_invoice_ids) =
            check_billing_clearance(&s.payments_url, &shipment_ids).await
                .map_err(AppError::internal)?;

        if !all_cleared {
            tracing::warn!(
                container_id  = %container_id,
                pallet_id     = %req.pallet_id,
                blocked_count = %blocked_shipments.len(),
                "Pallet load blocked — unpaid invoices outstanding"
            );
            return Ok::<_, AppError>((
                StatusCode::PAYMENT_REQUIRED,
                Json(serde_json::json!({
                    "error":                "BILLING_CLEARANCE_REQUIRED",
                    "message":              "Pallet cannot be loaded: some shipments have outstanding invoices.",
                    "blocked_shipment_ids": blocked_shipments,
                    "unpaid_invoice_ids":   unpaid_invoice_ids,
                })),
            ));
        }
    }

    if !s.pod_url.is_empty() && !shipment_ids.is_empty() {
        let (all_completed, missing_pop) =
            check_pop_status(&s.pod_url, &shipment_ids).await
                .map_err(AppError::internal)?;

        if !all_completed {
            tracing::warn!(
                container_id = %container_id,
                pallet_id    = %req.pallet_id,
                missing_count = %missing_pop.len(),
                "Pallet load blocked — POP not completed for some shipments"
            );
            return Ok::<_, AppError>((
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({
                    "error":                    "POP_REQUIRED",
                    "message":                  "Pallet cannot be loaded: some shipments do not have a completed Proof of Pickup.",
                    "shipment_ids_missing_pop": missing_pop,
                })),
            ));
        }
    }

    s.containers
        .add_pallet(container_id, req.pallet_id, claims.tenant_id, &req.master_awbs)
        .await
        .map_err(|e| AppError::BusinessRule(e.to_string()))?;

    tracing::info!(
        container_id = %container_id,
        pallet_id    = %req.pallet_id,
        awb_count    = req.master_awbs.len(),
        "Pallet loaded into container"
    );

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "container_id": container_id,
            "pallet_id":    req.pallet_id,
            "awbs_loaded":  req.master_awbs,
        })),
    ))
}

#[derive(serde::Deserialize)]
struct LoadLoosePieceRequest {
    piece_awb:  String,
    master_awb: String,
}

/// `POST /v1/containers/:id/load-loose-piece`
///
/// Same billing clearance and POP guards as load-pallet, applied per-piece.
async fn load_loose_piece_handler(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(container_id): Path<Uuid>,
    Json(req): Json<LoadLoosePieceRequest>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_UPDATE)?;

    let shipment_ids = s.containers
        .resolve_awbs_to_shipment_ids(&[req.master_awb.clone()])
        .await
        .map_err(AppError::internal)?;

    if !s.payments_url.is_empty() && !shipment_ids.is_empty() {
        let (all_cleared, blocked_shipments, unpaid_invoice_ids) =
            check_billing_clearance(&s.payments_url, &shipment_ids).await
                .map_err(AppError::internal)?;

        if !all_cleared {
            return Ok::<_, AppError>((
                StatusCode::PAYMENT_REQUIRED,
                Json(serde_json::json!({
                    "error":                "BILLING_CLEARANCE_REQUIRED",
                    "message":              "Piece cannot be loaded: shipment has outstanding invoices.",
                    "blocked_shipment_ids": blocked_shipments,
                    "unpaid_invoice_ids":   unpaid_invoice_ids,
                })),
            ));
        }
    }

    if !s.pod_url.is_empty() && !shipment_ids.is_empty() {
        let (all_completed, missing_pop) =
            check_pop_status(&s.pod_url, &shipment_ids).await
                .map_err(AppError::internal)?;

        if !all_completed {
            return Ok::<_, AppError>((
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({
                    "error":                    "POP_REQUIRED",
                    "message":                  "Piece cannot be loaded: shipment does not have a completed Proof of Pickup.",
                    "shipment_ids_missing_pop": missing_pop,
                })),
            ));
        }
    }

    s.containers
        .add_loose_piece(container_id, claims.tenant_id, &req.piece_awb, &req.master_awb)
        .await
        .map_err(|e| AppError::BusinessRule(e.to_string()))?;

    tracing::info!(
        container_id = %container_id,
        piece_awb    = %req.piece_awb,
        "Loose piece loaded into container"
    );

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "container_id": container_id,
            "piece_awb":    req.piece_awb,
            "master_awb":   req.master_awb,
        })),
    ))
}

/// `POST /v1/containers/:id/finalise` — seal the manifest; no more loading after this.
async fn finalise_manifest_handler(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(container_id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_UPDATE)?;

    s.containers.finalise(container_id).await
        .map_err(|e| AppError::BusinessRule(e.to_string()))?;

    tracing::info!(container_id = %container_id, "Container manifest finalised");

    Ok::<_, AppError>((
        StatusCode::OK,
        Json(serde_json::json!({
            "container_id": container_id,
            "status":       "manifested",
        })),
    ))
}

async fn list_hubs(State(s): State<AppState>, claims: AuthClaims) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::FLEET_READ)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let hubs = s.svc.list_hubs(&tenant_id).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"hubs": hubs}))))
}

async fn get_hub_by_id(State(s): State<AppState>, claims: AuthClaims, Path(id): Path<Uuid>) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::FLEET_READ)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let hub = s.svc.get_hub(&tenant_id, id).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(hub)))
}

async fn induct(State(s): State<AppState>, claims: AuthClaims, Json(cmd): Json<InductParcelCommand>) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_UPDATE)?;
    let induction = s.svc.induct_parcel(cmd).await?;
    Ok::<_, AppError>((StatusCode::CREATED, Json(induction)))
}

async fn sort(State(s): State<AppState>, claims: AuthClaims, Json(cmd): Json<SortParcelCommand>) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_UPDATE)?;
    let induction = s.svc.sort_parcel(cmd).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(induction)))
}

async fn dispatch(State(s): State<AppState>, claims: AuthClaims, Path(id): Path<Uuid>) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_UPDATE)?;
    let induction = s.svc.dispatch_parcel(id).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(induction)))
}

async fn manifest(State(s): State<AppState>, claims: AuthClaims, Path(hub_id): Path<Uuid>) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::SHIPMENT_READ)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let parcels = s.svc.hub_manifest(hub_id, &tenant_id).await?;
    let count = parcels.len();
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({
        "hub_id":  hub_id,
        "parcels": parcels,
        "count":   count,
    }))))
}

// ---------------------------------------------------------------------------
// Throughput analytics handler
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct ThroughputRow {
    h:        i32,
    inducted: i64,
    sorted:   i64,
}

/// `GET /v1/hubs/throughput/today`
///
/// Returns per-hour induction + sort counts for the current UTC calendar day,
/// scoped to the caller's tenant. Buckets are only emitted for hours that have
/// at least one event — the frontend fills in the rest with zeros.
async fn throughput_today(State(s): State<AppState>, claims: AuthClaims) -> impl IntoResponse {
    use logisticos_types::TenantId;
    claims.require_permission(permissions::FLEET_READ)?;
    let tenant_id = TenantId::from_uuid(claims.tenant_id);

    let rows = sqlx::query_as::<_, ThroughputRow>(
        r#"
        SELECT
            EXTRACT(HOUR FROM inducted_at)::int AS h,
            COUNT(*)          AS inducted,
            COUNT(sorted_at)  AS sorted
        FROM hub_ops.parcel_inductions
        WHERE tenant_id  = $1
          AND inducted_at >= DATE_TRUNC('day', NOW() AT TIME ZONE 'UTC')
          AND inducted_at <  DATE_TRUNC('day', NOW() AT TIME ZONE 'UTC') + INTERVAL '1 day'
        GROUP BY 1
        ORDER BY 1
        "#,
    )
    .bind(tenant_id.inner())
    .fetch_all(&s.pool)
    .await
    .map_err(|e| AppError::internal(e.into()))?;

    let buckets: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            let label = match r.h {
                0       => "12AM".to_owned(),
                1..=11  => format!("{}AM", r.h),
                12      => "12PM".to_owned(),
                _       => format!("{}PM", r.h - 12),
            };
            serde_json::json!({ "hour": label, "inducted": r.inducted, "sorted": r.sorted })
        })
        .collect();

    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({ "buckets": buckets }))))
}

// ---------------------------------------------------------------------------
// Pallet lifecycle handlers
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct ScanPieceBody {
    piece_awb:          String,
    master_awb:         String,
    declared_weight_g:  u32,
    actual_weight_g:    Option<u32>,
    destination_hub_id: Option<Uuid>,
}

/// `POST /v1/hubs/:hub_id/pieces` — scan a piece onto the open pallet for this hub.
async fn scan_piece_handler(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(hub_id): Path<Uuid>,
    Json(body): Json<ScanPieceBody>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_UPDATE)?;

    let piece_awb  = logisticos_types::awb::ChildAwb::parse(&body.piece_awb)
        .map_err(|e| AppError::Validation(e.to_string()))?;
    let master_awb = logisticos_types::awb::Awb::parse(&body.master_awb)
        .map_err(|e| AppError::Validation(e.to_string()))?;

    let pallet = s.pallet_svc.load_piece_onto_pallet(LoadPieceCommand {
        tenant_id:           logisticos_types::TenantId::from_uuid(claims.tenant_id),
        hub_id:              logisticos_types::HubId::from_uuid(hub_id),
        destination_hub_id:  body.destination_hub_id.map(logisticos_types::HubId::from_uuid),
        piece_awb,
        master_awb,
        declared_weight_g:   body.declared_weight_g,
        actual_weight_g:     body.actual_weight_g,
        scanned_by:          claims.user_id,
    }).await.map_err(|e| AppError::BusinessRule(e.to_string()))?;

    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({
        "pallet_id":    pallet.id,
        "piece_count":  pallet.piece_count(),
        "weight_kg":    pallet.total_weight_kg(),
        "status":       format!("{:?}", pallet.status),
    }))))
}

/// `GET /v1/pallets/:id` — fetch pallet state.
async fn get_pallet_handler(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_READ)?;
    let pallet = s.pallet_svc
        .find_pallet(logisticos_types::PalletId::from_uuid(id))
        .await?
        .ok_or_else(|| AppError::NotFound { resource: "pallet", id: id.to_string() })?;
    Ok::<_, AppError>((StatusCode::OK, Json(pallet)))
}

/// `POST /v1/pallets/:id/seal` — seal a pallet (no more pieces).
async fn seal_pallet_handler(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_UPDATE)?;
    let pallet = s.pallet_svc.seal_pallet(SealPalletCommand {
        pallet_id: logisticos_types::PalletId::from_uuid(id),
        sealed_by: claims.user_id,
    }).await.map_err(|e| AppError::BusinessRule(e.to_string()))?;
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({
        "pallet_id":   pallet.id,
        "piece_count": pallet.piece_count(),
        "weight_kg":   pallet.total_weight_kg(),
        "sealed_at":   pallet.sealed_at,
        "status":      format!("{:?}", pallet.status),
    }))))
}

#[derive(serde::Deserialize)]
struct LoadPalletIntoContainerBody {
    container_id: Uuid,
    /// Master AWBs in this pallet — used to populate container.master_awbs.
    /// If omitted the container's AWB denorm won't be updated at load time.
    #[serde(default)]
    pallet_awbs: Vec<String>,
}

/// `POST /v1/pallets/:id/load` — load a sealed pallet into a container.
async fn load_pallet_into_container_handler(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(pallet_id): Path<Uuid>,
    Json(body): Json<LoadPalletIntoContainerBody>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_UPDATE)?;

    let pallet_awbs: Vec<logisticos_types::awb::Awb> = body.pallet_awbs
        .iter()
        .filter_map(|s| logisticos_types::awb::Awb::parse(s).ok())
        .collect();

    s.pallet_svc.load_pallet_into_container(LoadPalletIntoContainerCommand {
        container_id: logisticos_types::ContainerId::from_uuid(body.container_id),
        pallet_id:    logisticos_types::PalletId::from_uuid(pallet_id),
        pallet_awbs,
    }).await.map_err(|e| AppError::BusinessRule(e.to_string()))?;

    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({
        "container_id": body.container_id,
        "pallet_id":    pallet_id,
    }))))
}

/// `POST /v1/containers/:id/arrive` — record container arrival at destination hub.
async fn arrive_container_handler(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(container_id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_UPDATE)?;
    s.pallet_svc.arrive_container(ArriveContainerCommand {
        container_id: logisticos_types::ContainerId::from_uuid(container_id),
    }).await.map_err(|e| AppError::BusinessRule(e.to_string()))?;
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({
        "container_id": container_id,
        "status":       "delivered",
    }))))
}

// ---------------------------------------------------------------------------
// Consolidation handlers
// ---------------------------------------------------------------------------

async fn list_truck_specs(
    State(s): State<AppState>,
    claims: AuthClaims,
) -> impl IntoResponse {
    claims.require_permission(permissions::FLEET_READ)?;
    let specs = s.consolidation_svc.list_specs(claims.tenant_id).await
        .map_err(AppError::internal)?;
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({ "specs": specs }))))
}

async fn create_truck_spec(
    State(s): State<AppState>,
    claims: AuthClaims,
    Json(cmd): Json<CreateSpecCommand>,
) -> impl IntoResponse {
    claims.require_permission(permissions::FLEET_MANAGE)?;
    let spec = s.consolidation_svc.create_spec(claims.tenant_id, cmd).await
        .map_err(AppError::internal)?;
    Ok::<_, AppError>((StatusCode::CREATED, Json(spec)))
}

async fn update_truck_spec(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(cmd): Json<UpdateSpecCommand>,
) -> impl IntoResponse {
    claims.require_permission(permissions::FLEET_MANAGE)?;
    let spec = s.consolidation_svc.update_spec(id, claims.tenant_id, cmd).await
        .map_err(|e| AppError::BusinessRule(e.to_string()))?;
    Ok::<_, AppError>((StatusCode::OK, Json(spec)))
}

async fn compute_plan(
    State(s): State<AppState>,
    claims: AuthClaims,
    Json(cmd): Json<ComputePlanCommand>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_UPDATE)?;
    let plan = s.consolidation_svc.compute_plan(claims.tenant_id, cmd).await
        .map_err(|e| AppError::BusinessRule(e.to_string()))?;
    Ok::<_, AppError>((StatusCode::CREATED, Json(plan)))
}

async fn get_plan(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_READ)?;
    let plan = s.consolidation_svc.get_plan(id, claims.tenant_id).await
        .map_err(AppError::internal)?
        .ok_or_else(|| AppError::NotFound { resource: "plan", id: id.to_string() })?;
    Ok::<_, AppError>((StatusCode::OK, Json(plan)))
}

async fn list_hub_plans(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(hub_id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_READ)?;
    let plans = s.consolidation_svc.list_plans(hub_id, claims.tenant_id).await
        .map_err(AppError::internal)?;
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({ "plans": plans }))))
}

async fn update_placements_handler(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdatePlacementsBody>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_UPDATE)?;
    let plan = s.consolidation_svc.update_placements(id, claims.tenant_id, body).await
        .map_err(|e| AppError::BusinessRule(e.to_string()))?;
    Ok::<_, AppError>((StatusCode::OK, Json(plan)))
}

// ---------------------------------------------------------------------------
// WebSocket handler — hub event bus
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct WsQuery {
    token: String,
}

async fn ws_hub_events(
    State(s): State<AppState>,
    Path(hub_id): Path<Uuid>,
    Query(q): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    // Manual JWT validation — WS upgrade cannot carry an Authorization header.
    let jwt_secret = std::env::var("AUTH__JWT_SECRET").unwrap_or_default();
    let jwt = JwtService::new(&jwt_secret, 3600, 86400);
    if jwt.validate_access_token(&q.token).is_err() {
        return (StatusCode::UNAUTHORIZED, "invalid or expired token").into_response();
    }
    let broadcaster = s.hub_broadcaster.clone();
    ws.on_upgrade(move |socket| drive_ws(socket, hub_id, broadcaster))
}

async fn drive_ws(mut socket: WebSocket, hub_id: Uuid, broadcaster: HubBroadcaster) {
    let mut rx = broadcaster.subscribe(hub_id).await;
    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Ok(text) => {
                        if socket.send(Message::Text(text)).await.is_err() { break; }
                    }
                    Err(_) => break,
                }
            }
            frame = socket.recv() => {
                match frame {
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Bootstrap
// ---------------------------------------------------------------------------

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load()?;
    let otlp = std::env::var("OTLP_ENDPOINT").ok();
    logisticos_tracing::init(logisticos_tracing::TracingConfig {
        service_name: "hub-ops",
        env: &cfg.app.env,
        otlp_endpoint: otlp.as_deref(),
        log_level: None,
    })?;

    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .after_connect(|conn, _meta| Box::pin(async move {
            sqlx::query("SET search_path TO hub_ops, public")
                .execute(&mut *conn)
                .await?;
            Ok(())
        }))
        .connect(&cfg.database.url)
        .await?;

    logisticos_common::migrations::run(&pool, "hub_ops", &sqlx::migrate!("./migrations")).await?;

    let hub_repo       = Arc::new(PgHubRepository { pool: pool.clone() });
    let induction_repo = Arc::new(PgInductionRepository { pool: pool.clone() });
    let container_repo = Arc::new(PgContainerRepository { pool: pool.clone() });
    let spec_repo      = Arc::new(PgTruckSpecRepository { pool: pool.clone() });
    let plan_repo      = Arc::new(PgConsolidationPlanRepository { pool: pool.clone() });
    let hub_broadcaster = HubBroadcaster::new();
    let svc = Arc::new(HubService::new(hub_repo, induction_repo));
    let consolidation_svc = Arc::new(ConsolidationService::new(
        spec_repo, plan_repo, hub_broadcaster.clone(),
    ));

    let kafka = Arc::new(
        KafkaProducer::new(&cfg.kafka.brokers)
            .context("failed to create Kafka producer for hub-ops")?,
    );
    let pallet_repo    = Arc::new(PgPalletRepository::new(pool.clone()));
    let pallet_ctr_repo = Arc::new(PgPalletContainerRepository::new(pool.clone()));
    let pallet_svc     = Arc::new(PalletService::new(pallet_repo, pallet_ctr_repo.clone(), kafka.clone()));

    // Cross-border hub transfer service — reuses the typed container repo + kafka.
    let hub_transfer_svc = Arc::new(HubTransferService::new(
        pallet_ctr_repo.clone(),
        Arc::new(PgHubScanRepository::new(pool.clone())),
        Arc::new(PgHubLocationRepository::new(pool.clone())),
        Arc::new(PgHubInventoryRepository::new(pool.clone())),
        Arc::new(PgHubTransferManifestRepository::new(pool.clone())),
        Arc::new(PgHubRoutingConfigRepository::new(pool.clone())),
        kafka.clone(),
    ));

    let payments_url = cfg.payments.url.clone();
    let pod_url      = cfg.pod.url.clone();
    let state = AppState {
        svc, containers: container_repo,
        consolidation_svc, pallet_svc,
        hub_transfer_svc,
        hub_broadcaster,
        payments_url, pod_url,
        pool: pool.clone(),
    };

    let jwt_secret = std::env::var("AUTH__JWT_SECRET")
        .context("AUTH__JWT_SECRET env var not set")?;
    let jwt = Arc::new(JwtService::new(&jwt_secret, 3600, 86400));

    // Protected business routes — JWT required on all /v1/* endpoints.
    let protected = Router::new()
        // Hub management
        .route("/v1/hubs",                    get(list_hubs).post(create_hub))
        .route("/v1/hubs/throughput/today",   get(throughput_today))
        .route("/v1/hubs/:id",                get(get_hub_by_id))
        .route("/v1/hubs/:id/manifest",       get(manifest))
        .route("/v1/hubs/induct",             post(induct))
        .route("/v1/hubs/sort",               post(sort))
        .route("/v1/hubs/dispatch/:id",       post(dispatch))
        // Container CRUD — create, load, finalise, depart (billing clearance + POP guard)
        .route("/v1/containers",                      post(create_container_handler))
        .route("/v1/containers/:id/load-pallet",      post(load_pallet_handler))
        .route("/v1/containers/:id/load-loose-piece", post(load_loose_piece_handler))
        .route("/v1/containers/:id/finalise",         post(finalise_manifest_handler))
        .route("/v1/containers/:id/depart",           axum::routing::put(depart_container))
        // Pallet lifecycle — piece scanning, sealing, container loading
        .route("/v1/hubs/:id/pieces",          post(scan_piece_handler))
        .route("/v1/pallets/:id",              get(get_pallet_handler))
        .route("/v1/pallets/:id/seal",         post(seal_pallet_handler))
        .route("/v1/pallets/:id/load",         post(load_pallet_into_container_handler))
        .route("/v1/containers/:id/arrive",    post(arrive_container_handler))
        // Cross-border hub transfer — scans, container customs lifecycle, fan-out
        .route("/v1/hub-transfer/scans",                    post(record_scan_handler))
        .route("/v1/hub-transfer/scans/:shipment_id",       get(list_scans_handler))
        .route("/v1/containers/:id/arrive-at-port",         post(arrive_at_port_handler))
        .route("/v1/containers/:id/enter-customs",          post(enter_customs_handler))
        .route("/v1/containers/:id/clear-customs",          post(clear_customs_handler))
        .route("/v1/containers/:id/release-domestic",       post(release_domestic_handler))
        .route("/v1/containers/:id/deconsolidate",          post(deconsolidate_handler))
        .route("/v1/containers/:id/manifest",               get(get_manifest_handler))
        .route("/v1/hubs/:id/customs-queue",                get(customs_queue_handler))
        // Hub transfer admin — locations, inventory, routing config
        .route("/v1/hub-transfer/locations",                get(list_locations_handler).post(create_location_handler))
        .route("/v1/hub-transfer/inventory",                get(list_inventory_handler).post(check_in_inventory_handler))
        .route("/v1/hub-transfer/inventory/move",           post(move_inventory_handler))
        .route("/v1/hub-transfer/routing-config",           get(list_routing_handler).post(upsert_routing_handler))
        // Consolidation — truck specs
        .route("/v1/consolidation/specs",     get(list_truck_specs).post(create_truck_spec))
        .route("/v1/consolidation/specs/:id", axum::routing::put(update_truck_spec))
        // Consolidation — plans
        .route("/v1/consolidation/plans",                       post(compute_plan))
        .route("/v1/consolidation/plans/:id",                   get(get_plan))
        .route("/v1/consolidation/plans/:id/placements",        axum::routing::put(update_placements_handler))
        .route("/v1/hubs/:hub_id/consolidation/plans",          get(list_hub_plans))
        .layer(axum::middleware::from_fn_with_state(jwt, logisticos_auth::middleware::require_auth));

    // WebSocket — JWT validated manually inside the handler from ?token= query param.
    let ws_router = Router::new()
        .route("/v1/hubs/:id/ws", get(ws_hub_events))
        .with_state(state.clone());

    // Observability endpoints — no auth required (scraped by infra, not user-facing).
    let observability = Router::new()
        .route("/health",  get(|| async { axum::Json(serde_json::json!({"status":"ok","service":"hub-ops"})) }))
        .route("/ready",   get(|| async { axum::Json(serde_json::json!({"status":"ready"})) }))
        .route("/metrics", get(|| async { "" }));

    let app = protected
        .merge(ws_router)
        .merge(observability)
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", cfg.app.host, cfg.app.port).parse()?;
    tracing::info!(addr = %addr, "hub-ops service listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).with_graceful_shutdown(shutdown_signal()).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Cross-border hub transfer handlers
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct RecordScanRequest {
    hub_id:           Uuid,
    piece_awb:        String,
    master_awb:       String,
    shipment_id:      Uuid,
    scan_type:        ScanType,
    /// Hardware clock at the physical scan moment (ISO-8601).
    device_timestamp: chrono::DateTime<chrono::Utc>,
    #[serde(default)] pallet_id:    Option<Uuid>,
    #[serde(default)] container_id: Option<Uuid>,
    #[serde(default)] exception:    Option<ScanException>,
}

/// `POST /v1/hub-transfer/scans` — record an immutable hub scan.
/// `InboundReceive` requires a completed POP (chain-of-custody open bookend).
async fn record_scan_handler(
    State(s): State<AppState>,
    claims: AuthClaims,
    Json(req): Json<RecordScanRequest>,
) -> impl IntoResponse {
    use logisticos_types::{awb::{Awb, ChildAwb}, ContainerId, HubId, PalletId, ShipmentId, TenantId};
    claims.require_permission(permissions::SHIPMENT_UPDATE)?;

    // POP-before-InboundReceive gate.
    if matches!(req.scan_type, ScanType::InboundReceive) && !s.pod_url.is_empty() {
        let (all_completed, missing_pop) =
            check_pop_status(&s.pod_url, &[req.shipment_id]).await.map_err(AppError::internal)?;
        if !all_completed {
            return Ok::<_, AppError>((
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({
                    "error":   "POP_REQUIRED",
                    "message": "Inbound receive blocked: shipment has no completed Proof of Pickup.",
                    "shipment_ids_missing_pop": missing_pop,
                })),
            ));
        }
    }

    let piece  = ChildAwb::parse(&req.piece_awb).map_err(|e| AppError::Validation(e.to_string()))?;
    let master = Awb::parse(&req.master_awb).map_err(|e| AppError::Validation(e.to_string()))?;

    let mut scan = HubScan::record(
        TenantId::from_uuid(claims.tenant_id),
        HubId::from_uuid(req.hub_id),
        piece,
        master,
        ShipmentId::from_uuid(req.shipment_id),
        req.scan_type,
        claims.user_id,
        req.device_timestamp,
    );
    if let Some(p) = req.pallet_id    { scan = scan.with_pallet(PalletId::from_uuid(p)); }
    if let Some(c) = req.container_id { scan = scan.with_container(ContainerId::from_uuid(c)); }
    if let Some(e) = req.exception    { scan = scan.with_exception(e); }

    let scan = s.hub_transfer_svc.record_scan(scan).await?;
    let body = serde_json::to_value(&scan)
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    Ok((StatusCode::CREATED, Json(body)))
}

/// `GET /v1/hub-transfer/scans/:shipment_id` — scan history for a shipment.
async fn list_scans_handler(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(shipment_id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_READ)?;
    let scans = s.hub_transfer_svc
        .list_scans(shipment_id, claims.tenant_id)
        .await?;
    Ok::<_, AppError>((StatusCode::OK, Json(scans)))
}

/// Optional per-shipment recipient detail for milestone customer notifications.
#[derive(serde::Deserialize, Default)]
struct MilestoneDetailsRequest {
    #[serde(default)]
    details: Vec<logisticos_events::payloads::ShipmentMilestoneDetail>,
}

async fn arrive_at_port_handler(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(container_id): Path<Uuid>,
    Json(req): Json<MilestoneDetailsRequest>,
) -> impl IntoResponse {
    use logisticos_types::ContainerId;
    claims.require_permission(permissions::SHIPMENT_UPDATE)?;
    let c = s.hub_transfer_svc
        .arrive_at_port(ContainerId::from_uuid(container_id), req.details)
        .await?;
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({
        "container_id": container_id, "status": format!("{:?}", c.status),
    }))))
}

async fn enter_customs_handler(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(container_id): Path<Uuid>,
    Json(req): Json<MilestoneDetailsRequest>,
) -> impl IntoResponse {
    use logisticos_types::ContainerId;
    claims.require_permission(permissions::SHIPMENT_UPDATE)?;
    let c = s.hub_transfer_svc
        .enter_customs(ContainerId::from_uuid(container_id), req.details)
        .await?;
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({
        "container_id": container_id, "status": format!("{:?}", c.status),
    }))))
}

#[derive(serde::Deserialize)]
struct ClearCustomsRequest {
    /// Defaults to the authenticated user when omitted.
    #[serde(default)] cleared_by:         Option<Uuid>,
    #[serde(default)] duties_total_cents: Option<i64>,
    #[serde(default)] customs_filing_ref: Option<String>,
    /// 3-char tenant code for duties invoice numbering (forwarded to payments).
    #[serde(default)] tenant_code:        String,
    /// Per-shipment recipients + duty payers (the "enrich from request" data).
    #[serde(default)] details:            Vec<logisticos_events::payloads::ShipmentMilestoneDetail>,
}

/// `POST /v1/containers/:id/clear-customs` — mandatory human clearance gate.
async fn clear_customs_handler(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(container_id): Path<Uuid>,
    Json(req): Json<ClearCustomsRequest>,
) -> impl IntoResponse {
    use logisticos_types::ContainerId;
    claims.require_permission(permissions::SHIPMENT_UPDATE)?;
    let c = s.hub_transfer_svc.clear_customs(ClearCustomsCommand {
        container_id:       ContainerId::from_uuid(container_id),
        cleared_by:         req.cleared_by.unwrap_or(claims.user_id),
        duties_total_cents: req.duties_total_cents,
        customs_filing_ref: req.customs_filing_ref,
        tenant_code:        req.tenant_code,
        details:            req.details,
    }).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({
        "container_id": container_id, "status": format!("{:?}", c.status),
    }))))
}

async fn release_domestic_handler(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(container_id): Path<Uuid>,
) -> impl IntoResponse {
    use logisticos_types::ContainerId;
    claims.require_permission(permissions::SHIPMENT_UPDATE)?;
    let c = s.hub_transfer_svc.release_domestic(ContainerId::from_uuid(container_id)).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({
        "container_id": container_id, "status": format!("{:?}", c.status),
    }))))
}

#[derive(serde::Deserialize)]
struct DeconsolidateRequest {
    destination_zone: String,
    /// Master AWBs in the container — resolved to shipment IDs for routing fan-out.
    #[serde(default)] master_awbs: Vec<String>,
    /// Last-mile service level forwarded to dispatch/carrier ("standard" default).
    #[serde(default)] service_level: String,
    /// SLA window in hours forwarded to the carrier booking (0 = consumer default).
    #[serde(default)] sla_hours: i64,
}

/// `POST /v1/containers/:id/deconsolidate` — break-bulk + last-mile routing fan-out.
async fn deconsolidate_handler(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(container_id): Path<Uuid>,
    Json(req): Json<DeconsolidateRequest>,
) -> impl IntoResponse {
    use logisticos_types::ContainerId;
    claims.require_permission(permissions::SHIPMENT_UPDATE)?;

    let shipment_ids = if req.master_awbs.is_empty() {
        Vec::new()
    } else {
        s.containers.resolve_awbs_to_shipment_ids(&req.master_awbs).await.map_err(AppError::internal)?
    };

    let c = s.hub_transfer_svc.deconsolidate(DeconsolidateCommand {
        container_id:     ContainerId::from_uuid(container_id),
        destination_zone: req.destination_zone,
        shipment_ids:     shipment_ids.clone(),
        service_level:    req.service_level,
        sla_hours:        req.sla_hours,
    }).await?;

    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({
        "container_id":   container_id,
        "status":         format!("{:?}", c.status),
        "fanned_out":     shipment_ids.len(),
    }))))
}

async fn get_manifest_handler(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(container_id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_READ)?;
    match s.hub_transfer_svc.get_manifest(container_id).await? {
        Some(m) => Ok::<_, AppError>((StatusCode::OK, Json(m))),
        None => Err(AppError::NotFound { resource: "manifest", id: container_id.to_string() }),
    }
}

async fn customs_queue_handler(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(hub_id): Path<Uuid>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_READ)?;
    let queue = s.hub_transfer_svc.list_customs_queue(hub_id, claims.tenant_id).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(queue)))
}

#[derive(serde::Deserialize)]
struct HubScopedQuery {
    hub_id:  Uuid,
    #[serde(default)] zone_id: Option<String>,
}

#[derive(serde::Deserialize)]
struct CreateLocationRequest {
    hub_id:       Uuid,
    zone_id:      String,
    location_tag: String,
    #[serde(default)] aisle:        Option<String>,
    #[serde(default)] rack:         Option<String>,
    #[serde(default)] shelf:        Option<String>,
    #[serde(default)] bin:          Option<String>,
    #[serde(default)] capacity_cbm: Option<f64>,
}

async fn create_location_handler(
    State(s): State<AppState>,
    claims: AuthClaims,
    Json(req): Json<CreateLocationRequest>,
) -> impl IntoResponse {
    use logisticos_types::{HubId, TenantId};
    claims.require_permission(permissions::FLEET_MANAGE)?;
    let location = HubLocation::new(
        TenantId::from_uuid(claims.tenant_id),
        HubId::from_uuid(req.hub_id),
        req.zone_id,
        req.location_tag,
    )
    .with_path(req.aisle, req.rack, req.shelf, req.bin);
    let mut location = location;
    location.capacity_cbm = req.capacity_cbm;
    s.hub_transfer_svc.create_location(&location).await?;
    Ok::<_, AppError>((StatusCode::CREATED, Json(location)))
}

async fn list_locations_handler(
    State(s): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<HubScopedQuery>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_READ)?;
    let locations = s.hub_transfer_svc.list_locations(q.hub_id, q.zone_id.as_deref()).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(locations)))
}

#[derive(serde::Deserialize)]
struct CheckInInventoryRequest {
    hub_id:      Uuid,
    location_id: Uuid,
    /// "piece" → unit_ref is a ChildAwb; "consolidated" → unit_ref is a PalletId uuid.
    unit_kind:   String,
    unit_ref:    String,
    #[serde(default)] batch_number: Option<String>,
}

async fn check_in_inventory_handler(
    State(s): State<AppState>,
    claims: AuthClaims,
    Json(req): Json<CheckInInventoryRequest>,
) -> impl IntoResponse {
    use logisticos_types::{awb::ChildAwb, HubId, PalletId, TenantId};
    claims.require_permission(permissions::SHIPMENT_UPDATE)?;

    let unit = match req.unit_kind.as_str() {
        "piece" => InventoryUnit::Piece(
            ChildAwb::parse(&req.unit_ref).map_err(|e| AppError::Validation(e.to_string()))?,
        ),
        "consolidated" => InventoryUnit::Consolidated(PalletId::from_uuid(
            Uuid::parse_str(&req.unit_ref).map_err(|e| AppError::Validation(e.to_string()))?,
        )),
        other => return Err(AppError::Validation(format!("unknown unit_kind '{other}'"))),
    };

    let mut inv = HubInventory::check_in(
        TenantId::from_uuid(claims.tenant_id),
        HubId::from_uuid(req.hub_id),
        req.location_id,
        unit,
        claims.user_id,
    );
    inv.batch_number = req.batch_number;
    s.hub_transfer_svc.check_in_inventory(&inv).await?;
    Ok::<_, AppError>((StatusCode::CREATED, Json(inv)))
}

#[derive(serde::Deserialize)]
struct InventoryLocationQuery { location_id: Uuid }

async fn list_inventory_handler(
    State(s): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<InventoryLocationQuery>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_READ)?;
    let items = s.hub_transfer_svc.list_inventory_at(q.location_id).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(items)))
}

#[derive(serde::Deserialize)]
struct MoveInventoryRequest {
    inventory_id:   Uuid,
    to_location_id: Uuid,
}

async fn move_inventory_handler(
    State(s): State<AppState>,
    claims: AuthClaims,
    Json(req): Json<MoveInventoryRequest>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_UPDATE)?;
    s.hub_transfer_svc.move_inventory(req.inventory_id, req.to_location_id).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({
        "inventory_id": req.inventory_id, "location_id": req.to_location_id,
    }))))
}

async fn list_routing_handler(
    State(s): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<HubScopedQuery>,
) -> impl IntoResponse {
    claims.require_permission(permissions::SHIPMENT_READ)?;
    let configs = s.hub_transfer_svc.list_routing_configs(q.hub_id, claims.tenant_id).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(configs)))
}

#[derive(serde::Deserialize)]
struct UpsertRoutingRequest {
    hub_id:           Uuid,
    destination_zone: String,
    /// "own_driver" | "carrier" | "auto"
    routing_type:     RoutingType,
    #[serde(default)] carrier_id:                Option<Uuid>,
    #[serde(default)] auto_fallback_window_mins: i32,
}

async fn upsert_routing_handler(
    State(s): State<AppState>,
    claims: AuthClaims,
    Json(req): Json<UpsertRoutingRequest>,
) -> impl IntoResponse {
    use logisticos_types::{HubId, TenantId};
    claims.require_permission(permissions::FLEET_MANAGE)?;
    let mut config = HubRoutingConfig::new(
        TenantId::from_uuid(claims.tenant_id),
        HubId::from_uuid(req.hub_id),
        req.destination_zone,
        req.routing_type,
    )
    .with_fallback_window(req.auto_fallback_window_mins);
    config.carrier_id = req.carrier_id;
    s.hub_transfer_svc.upsert_routing_config(&config).await?;
    Ok::<_, AppError>((StatusCode::OK, Json(config)))
}

async fn shutdown_signal() {
    use tokio::signal;
    let ctrl_c = async { signal::ctrl_c().await.expect("ctrl-c") };
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate()).expect("SIGTERM").recv().await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
}
