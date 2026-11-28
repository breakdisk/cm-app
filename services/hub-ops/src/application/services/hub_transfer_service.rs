//! HubTransferService — orchestrates the cross-border hub transfer lifecycle.
//!
//! Event-first: every container/customs/scan transition persists state then emits
//! a Kafka event. Hub-ops never calls dispatch or carrier directly — it emits
//! `hub.shipment.dispatch_requested` / `hub.shipment.carrier_booking_requested`
//! and the downstream consumers react.
//!
//! ```text
//! scan (InboundReceive)        → emit hub.piece.scanned_inbound  → order-intake: AtHub
//! container.arrive_at_port()   → emit hub.container.arrived_at_port → engagement
//! container.enter_customs()    → manifest.hold()  → emit hub.container.customs_hold
//! container.clear_customs()    → manifest.clear() → emit hub.container.customs_cleared
//! container.release_domestic() → emit hub.container.released_domestic
//! container.deconsolidate()    → emit hub.container.deconsolidated + routing fan-out
//! ```

use std::sync::Arc;

use chrono::Utc;
use logisticos_errors::{AppError, AppResult};
use logisticos_events::{
    envelope::Event,
    payloads::{
        ContainerArrivedAtPort, ContainerCustomsCleared, ContainerCustomsHold,
        ContainerDeconsolidated, ContainerReleasedDomestic, HubCarrierBookingRequested,
        HubDispatchRequested, PieceScannedInbound, ShipmentMilestoneDetail,
    },
    producer::KafkaProducer,
    topics,
};
use logisticos_types::{ContainerId, TenantId};
use uuid::Uuid;

use crate::application::services::hub_transfer_repositories::{
    HubInventoryRepository, HubLocationRepository, HubRoutingConfigRepository, HubScanRepository,
    HubTransferManifestRepository,
};
use crate::application::services::pallet_service::{ContainerRepository, PalletRepository};
use crate::application::services::HubRepository;
use crate::domain::entities::{
    HubId,
    container::Container, HubInventory, HubLocation, HubRoutingConfig, HubScan, HubTransferManifest,
    RoutingType, ScanType,
};

// ── Commands ──────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct ClearCustomsCommand {
    /// From the validated JWT, never the request body.
    pub tenant_id:          TenantId,
    pub container_id:       ContainerId,
    /// Hub agent who approved clearance — mandatory human gate.
    pub cleared_by:         Uuid,
    pub duties_total_cents: Option<i64>,
    pub customs_filing_ref: Option<String>,
    /// 3-char tenant code for duties invoice numbering (forwarded to payments).
    pub tenant_code:        String,
    /// Per-shipment recipients + duty payers (the "enrich from request" data).
    pub details:            Vec<ShipmentMilestoneDetail>,
}

#[derive(Debug)]
pub struct DeconsolidateCommand {
    /// From the validated JWT, never the request body.
    pub tenant_id:        TenantId,
    pub container_id:     ContainerId,
    /// Last-mile zone for routing-rule lookup at the destination hub.
    pub destination_zone: String,
    /// Shipments to fan out for last-mile (resolved from master AWBs by the caller).
    pub shipment_ids:     Vec<Uuid>,
    /// Enrichment supplied by the deconsolidate request — forwarded to the
    /// dispatch/carrier events so downstream SLA records are accurate.
    pub service_level:    String,
    pub sla_hours:        i64,
}

// ── Service ─────────────────────────────────────────────────────────────────────

pub struct HubTransferService {
    hub_repo:       Arc<dyn HubRepository>,
    pallet_repo:    Arc<dyn PalletRepository>,
    container_repo: Arc<dyn ContainerRepository>,
    scan_repo:      Arc<dyn HubScanRepository>,
    location_repo:  Arc<dyn HubLocationRepository>,
    inventory_repo: Arc<dyn HubInventoryRepository>,
    manifest_repo:  Arc<dyn HubTransferManifestRepository>,
    routing_repo:   Arc<dyn HubRoutingConfigRepository>,
    kafka:          Arc<KafkaProducer>,
}

impl HubTransferService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        hub_repo:       Arc<dyn HubRepository>,
        pallet_repo:    Arc<dyn PalletRepository>,
        container_repo: Arc<dyn ContainerRepository>,
        scan_repo:      Arc<dyn HubScanRepository>,
        location_repo:  Arc<dyn HubLocationRepository>,
        inventory_repo: Arc<dyn HubInventoryRepository>,
        manifest_repo:  Arc<dyn HubTransferManifestRepository>,
        routing_repo:   Arc<dyn HubRoutingConfigRepository>,
        kafka:          Arc<KafkaProducer>,
    ) -> Self {
        Self {
            hub_repo,
            pallet_repo,
            container_repo,
            scan_repo,
            location_repo,
            inventory_repo,
            manifest_repo,
            routing_repo,
            kafka,
        }
    }

    // ── Scans ───────────────────────────────────────────────────────────────────

    /// Append an immutable scan. On `InboundReceive` emits `hub.piece.scanned_inbound`
    /// so order-intake marks the shipment `AtHub`.
    pub async fn record_scan(&self, scan: HubScan) -> AppResult<HubScan> {
        let tenant = scan.tenant_id.clone();
        self.require_hub(&tenant, scan.hub_id.inner()).await?;
        if let Some(p) = &scan.pallet_id {
            self.pallet_repo
                .find_by_id(p, &tenant)
                .await
                .map_err(AppError::Internal)?
                .ok_or_else(|| AppError::NotFound { resource: "Pallet", id: p.inner().to_string() })?;
        }
        if let Some(c) = &scan.container_id {
            self.load_container(&tenant, c).await?;
        }
        self.scan_repo.append(&scan).await.map_err(AppError::Internal)?;

        if scan.scan_type == ScanType::InboundReceive {
            let event = Event::new(
                "hub-ops",
                "hub.piece.scanned_inbound",
                scan.tenant_id.inner(),
                PieceScannedInbound {
                    piece_awb:        scan.piece_awb.as_str().to_string(),
                    master_awb:       scan.master_awb.as_str().to_string(),
                    shipment_id:      scan.shipment_id.inner(),
                    tenant_id:        scan.tenant_id.inner(),
                    hub_id:           scan.hub_id.inner(),
                    scanned_by:       scan.scanned_by,
                    device_timestamp: Some(scan.device_timestamp.to_rfc3339()),
                    server_timestamp: scan.server_timestamp.to_rfc3339(),
                },
            );
            self.kafka
                .publish_event(topics::HUB_PIECE_SCANNED_INBOUND, &event)
                .await
                .map_err(AppError::Internal)?;
        }

        tracing::info!(scan_id = %scan.id, scan_type = ?scan.scan_type, "Hub scan recorded");
        Ok(scan)
    }

    pub async fn list_scans(
        &self,
        shipment_id: Uuid,
        tenant_id:   Uuid,
    ) -> AppResult<Vec<HubScan>> {
        self.scan_repo
            .list_for_shipment(shipment_id, tenant_id)
            .await
            .map_err(AppError::Internal)
    }

    // ── Container transitions ─────────────────────────────────────────────────────

    /// InTransit → ArrivedAtPort (sea/air). Ensures a manifest exists and emits
    /// `hub.container.arrived_at_port`.
    pub async fn arrive_at_port(
        &self,
        tenant_id:    &TenantId,
        container_id: ContainerId,
        details:      Vec<ShipmentMilestoneDetail>,
    ) -> AppResult<Container> {
        let mut container = self.load_container(tenant_id, &container_id).await?;
        container.arrive_at_port().map_err(business_rule)?;
        self.container_repo.save(&container).await.map_err(AppError::Internal)?;
        self.load_or_create_manifest(&container).await?;

        let event = Event::new(
            "hub-ops",
            "hub.container.arrived_at_port",
            container.tenant_id.inner(),
            ContainerArrivedAtPort {
                container_id: container.id.inner(),
                tenant_id:    container.tenant_id.inner(),
                port_hub_id:  Some(container.destination_hub.inner()),
                master_awbs:  master_awb_strings(&container),
                details,
                arrived_at:   container.arrived_at.unwrap_or_else(Utc::now).to_rfc3339(),
            },
        );
        self.kafka
            .publish_event(topics::CONTAINER_ARRIVED_AT_PORT, &event)
            .await
            .map_err(AppError::Internal)?;
        Ok(container)
    }

    /// ArrivedAtPort → Customs. Holds the manifest and emits `hub.container.customs_hold`.
    pub async fn enter_customs(
        &self,
        tenant_id:    &TenantId,
        container_id: ContainerId,
        details:      Vec<ShipmentMilestoneDetail>,
    ) -> AppResult<Container> {
        let mut container = self.load_container(tenant_id, &container_id).await?;
        container.enter_customs().map_err(business_rule)?;
        self.container_repo.save(&container).await.map_err(AppError::Internal)?;

        let mut manifest = self.load_or_create_manifest(&container).await?;
        manifest.hold().map_err(business_rule)?;
        self.manifest_repo.save(&manifest).await.map_err(AppError::Internal)?;

        let event = Event::new(
            "hub-ops",
            "hub.container.customs_hold",
            container.tenant_id.inner(),
            ContainerCustomsHold {
                container_id: container.id.inner(),
                tenant_id:    container.tenant_id.inner(),
                master_awbs:  master_awb_strings(&container),
                details,
                held_at:      Utc::now().to_rfc3339(),
            },
        );
        self.kafka
            .publish_event(topics::CONTAINER_CUSTOMS_HOLD, &event)
            .await
            .map_err(AppError::Internal)?;
        Ok(container)
    }

    /// Customs → Released (human gate). Records duties/filing + `cleared_by` on the
    /// manifest and emits `hub.container.customs_cleared`.
    pub async fn clear_customs(&self, cmd: ClearCustomsCommand) -> AppResult<Container> {
        let mut container = self.load_container(&cmd.tenant_id, &cmd.container_id).await?;
        container.clear_customs().map_err(business_rule)?;
        self.container_repo.save(&container).await.map_err(AppError::Internal)?;

        let mut manifest = self.load_or_create_manifest(&container).await?;
        if cmd.duties_total_cents.is_some() {
            manifest.duties_total_cents = cmd.duties_total_cents;
        }
        if cmd.customs_filing_ref.is_some() {
            manifest.customs_filing_ref = cmd.customs_filing_ref.clone();
        }
        manifest.clear(cmd.cleared_by).map_err(business_rule)?;
        self.manifest_repo.save(&manifest).await.map_err(AppError::Internal)?;

        let event = Event::new(
            "hub-ops",
            "hub.container.customs_cleared",
            container.tenant_id.inner(),
            ContainerCustomsCleared {
                container_id:       container.id.inner(),
                tenant_id:          container.tenant_id.inner(),
                master_awbs:        master_awb_strings(&container),
                cleared_by:         cmd.cleared_by,
                duties_total_cents: manifest.duties_total_cents,
                customs_filing_ref: manifest.customs_filing_ref.clone(),
                tenant_code:        cmd.tenant_code.clone(),
                details:            cmd.details.clone(),
                cleared_at:         manifest.cleared_at.unwrap_or_else(Utc::now).to_rfc3339(),
            },
        );
        self.kafka
            .publish_event(topics::CONTAINER_CUSTOMS_CLEARED, &event)
            .await
            .map_err(AppError::Internal)?;
        Ok(container)
    }

    /// InTransit → Released (road, skips port/customs). Emits `hub.container.released_domestic`.
    pub async fn release_domestic(
        &self,
        tenant_id:    &TenantId,
        container_id: ContainerId,
    ) -> AppResult<Container> {
        let mut container = self.load_container(tenant_id, &container_id).await?;
        container.release_domestic().map_err(business_rule)?;
        self.container_repo.save(&container).await.map_err(AppError::Internal)?;

        let event = Event::new(
            "hub-ops",
            "hub.container.released_domestic",
            container.tenant_id.inner(),
            ContainerReleasedDomestic {
                container_id: container.id.inner(),
                tenant_id:    container.tenant_id.inner(),
                master_awbs:  master_awb_strings(&container),
                released_at:  Utc::now().to_rfc3339(),
            },
        );
        self.kafka
            .publish_event(topics::CONTAINER_RELEASED_DOMESTIC, &event)
            .await
            .map_err(AppError::Internal)?;
        Ok(container)
    }

    /// Released → Deconsolidated. Emits `hub.container.deconsolidated` then fans out a
    /// last-mile routing request per shipment based on the destination-zone routing rule.
    pub async fn deconsolidate(&self, cmd: DeconsolidateCommand) -> AppResult<Container> {
        let mut container = self.load_container(&cmd.tenant_id, &cmd.container_id).await?;
        let children = container.deconsolidate().map_err(business_rule)?;
        self.container_repo.save(&container).await.map_err(AppError::Internal)?;

        // Auto-resolve shipment IDs from the container's master AWBs when the
        // caller (e.g. the MCP tool) did not supply them explicitly. This ensures
        // fan_out_routing emits dispatch/carrier events for every parcel.
        let resolved_ids: Vec<Uuid>;
        let cmd = if cmd.shipment_ids.is_empty() {
            let awbs = master_awb_strings(&container);
            resolved_ids = self
                .container_repo
                .resolve_awbs_to_shipment_ids(&awbs, &cmd.tenant_id)
                .await
                .map_err(AppError::Internal)?;
            tracing::info!(
                container_id = %container.id.inner(),
                resolved     = resolved_ids.len(),
                "deconsolidate: auto-resolved shipment IDs from master AWBs"
            );
            DeconsolidateCommand { shipment_ids: resolved_ids, ..cmd }
        } else {
            cmd
        };

        let child_awbs: Vec<String> = children.iter().map(|c| c.as_str().to_string()).collect();
        let event = Event::new(
            "hub-ops",
            "hub.container.deconsolidated",
            container.tenant_id.inner(),
            ContainerDeconsolidated {
                container_id:       container.id.inner(),
                tenant_id:          container.tenant_id.inner(),
                destination_hub_id: container.destination_hub.inner(),
                master_awbs:        master_awb_strings(&container),
                child_awbs,
                deconsolidated_at:  Utc::now().to_rfc3339(),
            },
        );
        self.kafka
            .publish_event(topics::CONTAINER_DECONSOLIDATED, &event)
            .await
            .map_err(AppError::Internal)?;

        self.fan_out_routing(&container, &cmd).await?;
        Ok(container)
    }

    /// Resolve the destination-zone routing rule and emit one dispatch/carrier
    /// request per shipment. Defaults to own-driver dispatch when no rule exists.
    async fn fan_out_routing(
        &self,
        container: &Container,
        cmd:       &DeconsolidateCommand,
    ) -> AppResult<()> {
        let destination_zone = cmd.destination_zone.as_str();
        let shipment_ids     = cmd.shipment_ids.as_slice();
        let service_level    = cmd.service_level.clone();
        let sla_hours        = cmd.sla_hours;
        let tenant_id = container.tenant_id.inner();
        let hub_id    = container.destination_hub.inner();
        let config = self
            .routing_repo
            .find(tenant_id, hub_id, destination_zone)
            .await
            .map_err(AppError::Internal)?;

        let routing_type = config.as_ref().map(|c| c.routing_type).unwrap_or(RoutingType::OwnDriver);
        let carrier_id   = config.as_ref().and_then(|c| c.carrier_id);
        let window_mins  = config.as_ref().map(|c| c.auto_fallback_window_mins).unwrap_or(0);
        let now = Utc::now().to_rfc3339();

        for &shipment_id in shipment_ids {
            match routing_type {
                RoutingType::Carrier => {
                    let event = Event::new(
                        "hub-ops",
                        "hub.shipment.carrier_booking_requested",
                        tenant_id,
                        HubCarrierBookingRequested {
                            shipment_id,
                            tenant_id,
                            hub_id,
                            destination_zone: destination_zone.to_string(),
                            carrier_id,
                            service_level: service_level.clone(),
                            sla_hours,
                            total_cost_cents: 0,
                            requested_at: now.clone(),
                        },
                    );
                    self.kafka
                        .publish_event(topics::HUB_CARRIER_BOOKING_REQUESTED, &event)
                        .await
                        .map_err(AppError::Internal)?;
                }
                RoutingType::OwnDriver | RoutingType::Auto => {
                    let fallback = if matches!(routing_type, RoutingType::Auto) { window_mins } else { 0 };
                    let event = Event::new(
                        "hub-ops",
                        "hub.shipment.dispatch_requested",
                        tenant_id,
                        HubDispatchRequested {
                            shipment_id,
                            tenant_id,
                            hub_id,
                            destination_zone: destination_zone.to_string(),
                            auto_fallback_window_mins: fallback,
                            service_level: service_level.clone(),
                            sla_hours,
                            total_cost_cents: 0,
                            fallback_carrier_id: if matches!(routing_type, RoutingType::Auto) { carrier_id } else { None },
                            requested_at: now.clone(),
                        },
                    );
                    self.kafka
                        .publish_event(topics::HUB_DISPATCH_REQUESTED, &event)
                        .await
                        .map_err(AppError::Internal)?;
                }
            }
        }

        tracing::info!(
            container_id = %container.id.inner(),
            zone         = destination_zone,
            routing      = ?routing_type,
            shipments    = shipment_ids.len(),
            "Deconsolidation routing fan-out complete"
        );
        Ok(())
    }

    // ── Manifests / routing / locations / inventory (read + admin) ───────────────

    /// Transfer manifest for one of the caller's containers. `None` when the
    /// container has no manifest yet, or is not the caller's.
    pub async fn get_manifest(
        &self,
        tenant_id:    &TenantId,
        container_id: Uuid,
    ) -> AppResult<Option<HubTransferManifest>> {
        self.manifest_repo
            .find_by_container(container_id, tenant_id.inner())
            .await
            .map_err(AppError::Internal)
    }

    /// Container + its transfer manifest — backs the `get_container_status` MCP tool.
    pub async fn get_container_status(
        &self,
        tenant_id:    &TenantId,
        container_id: ContainerId,
    ) -> AppResult<(Container, Option<HubTransferManifest>)> {
        let container = self.load_container(tenant_id, &container_id).await?;
        let manifest  = self
            .manifest_repo
            .find_by_container(container_id.inner(), tenant_id.inner())
            .await
            .map_err(AppError::Internal)?;
        Ok((container, manifest))
    }

    pub async fn list_customs_queue(
        &self,
        hub_id:    Uuid,
        tenant_id: Uuid,
    ) -> AppResult<Vec<HubTransferManifest>> {
        self.require_hub(&TenantId::from_uuid(tenant_id), hub_id).await?;
        self.manifest_repo.list_in_customs(hub_id, tenant_id).await.map_err(AppError::Internal)
    }

    pub async fn upsert_routing_config(&self, config: &HubRoutingConfig) -> AppResult<()> {
        self.require_hub(&config.tenant_id, config.hub_id.inner()).await?;
        self.routing_repo.save(config).await.map_err(AppError::Internal)
    }

    pub async fn list_routing_configs(
        &self,
        hub_id:    Uuid,
        tenant_id: Uuid,
    ) -> AppResult<Vec<HubRoutingConfig>> {
        self.require_hub(&TenantId::from_uuid(tenant_id), hub_id).await?;
        self.routing_repo.list_for_hub(hub_id, tenant_id).await.map_err(AppError::Internal)
    }

    pub async fn create_location(&self, location: &HubLocation) -> AppResult<()> {
        self.require_hub(&location.tenant_id, location.hub_id.inner()).await?;
        self.location_repo.save(location).await.map_err(AppError::Internal)
    }

    pub async fn list_locations(
        &self,
        tenant_id: &TenantId,
        hub_id:    Uuid,
        zone_id:   Option<&str>,
    ) -> AppResult<Vec<HubLocation>> {
        self.require_hub(tenant_id, hub_id).await?;
        self.location_repo
            .list_for_hub(hub_id, tenant_id.inner(), zone_id)
            .await
            .map_err(AppError::Internal)
    }

    pub async fn check_in_inventory(&self, inventory: &HubInventory) -> AppResult<()> {
        self.require_hub(&inventory.tenant_id, inventory.hub_id.inner()).await?;
        // The location is a request-body id: it must be the caller's.
        self.location_repo
            .find_by_id(inventory.location_id, inventory.tenant_id.inner())
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound {
                resource: "HubLocation",
                id: inventory.location_id.to_string(),
            })?;
        self.inventory_repo.upsert(inventory).await.map_err(AppError::Internal)
    }

    pub async fn list_inventory_at(
        &self,
        tenant_id:   &TenantId,
        location_id: Uuid,
    ) -> AppResult<Vec<HubInventory>> {
        self.location_repo
            .find_by_id(location_id, tenant_id.inner())
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound {
                resource: "HubLocation",
                id: location_id.to_string(),
            })?;
        self.inventory_repo
            .list_at_location(location_id, tenant_id.inner())
            .await
            .map_err(AppError::Internal)
    }

    /// Move a unit to a new location in place (single-row update). Both the
    /// inventory record and the destination location must be the caller's.
    pub async fn move_inventory(
        &self,
        tenant_id:      &TenantId,
        inventory_id:   Uuid,
        to_location_id: Uuid,
    ) -> AppResult<()> {
        let rows = self
            .inventory_repo
            .move_to(inventory_id, to_location_id, tenant_id.inner())
            .await
            .map_err(AppError::Internal)?;
        if rows == 0 {
            return Err(AppError::NotFound {
                resource: "HubInventory",
                id: inventory_id.to_string(),
            });
        }
        Ok(())
    }

    // ── Internal helpers ──────────────────────────────────────────────────────────

    /// 404 unless `hub_id` is one of `tenant_id`'s hubs. Every hub id that
    /// arrives in a path, query or body goes through here.
    pub async fn require_hub(&self, tenant_id: &TenantId, hub_id: Uuid) -> AppResult<()> {
        self.hub_repo
            .find_by_id(&HubId::from_uuid(hub_id), tenant_id)
            .await
            .map_err(AppError::Internal)?
            .map(|_| ())
            .ok_or_else(|| AppError::NotFound { resource: "Hub", id: hub_id.to_string() })
    }

    async fn load_container(&self, tenant_id: &TenantId, id: &ContainerId) -> AppResult<Container> {
        self.container_repo
            .find_by_id(id, tenant_id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound {
                resource: "Container",
                id: id.inner().to_string(),
            })
    }

    async fn load_or_create_manifest(&self, container: &Container) -> AppResult<HubTransferManifest> {
        if let Some(m) = self
            .manifest_repo
            .find_by_container(container.id.inner(), container.tenant_id.inner())
            .await
            .map_err(AppError::Internal)?
        {
            return Ok(m);
        }
        let manifest = HubTransferManifest::new(
            container.tenant_id.clone(),
            container.id.clone(),
            container.origin_hub_id.clone(),
            container.destination_hub.clone(),
            container.transport_mode,
        );
        self.manifest_repo.save(&manifest).await.map_err(AppError::Internal)?;
        Ok(manifest)
    }
}

fn master_awb_strings(container: &Container) -> Vec<String> {
    container.master_awbs.iter().map(|a| a.as_str().to_string()).collect()
}

fn business_rule<E: std::fmt::Display>(e: E) -> AppError {
    AppError::BusinessRule(e.to_string())
}
