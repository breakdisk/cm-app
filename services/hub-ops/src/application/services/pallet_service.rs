//! PalletService — orchestrates piece → pallet → container consolidation.
//!
//! # Consolidation lifecycle
//!
//! ```text
//! scan piece at hub
//!     → load_piece_onto_pallet()   — adds ChildAwb to an Open pallet
//!     → seal_pallet()              — transitions Pallet Open→Sealed, emits PalletSealed
//!     → load_pallet_into_container() — attaches sealed pallet to a Planning/Loading container
//!     → depart_container()         — transitions Container→InTransit, emits ContainerDeparted
//! ```
//!
//! Weight discrepancy detection happens here: when `load_piece_onto_pallet` is
//! called with an `actual_weight_grams` that differs from the declared weight by
//! more than `WEIGHT_DISCREPANCY_THRESHOLD_G`, a `WeightDiscrepancyFound` Kafka
//! event is published so the payments service can create a surcharge invoice.

use std::sync::Arc;
use chrono::Utc;
use logisticos_errors::{AppError, AppResult};
use logisticos_types::{
    awb::{Awb, ChildAwb},
    ContainerId, HubId, PalletId, TenantId, TransportMode,
};
use logisticos_events::{
    envelope::Event,
    payloads::{
        ContainerArrived, ContainerDeparted, PalletSealed, WeightDiscrepancyFound,
    },
    producer::KafkaProducer,
    topics,
};

use crate::domain::entities::{
    container::{Container, ContainerError},
    pallet::{Pallet, PalletError},
};

/// Threshold (grams) above which a weight delta is considered a discrepancy.
const WEIGHT_DISCREPANCY_THRESHOLD_G: u32 = 100;

// ── Repository traits ─────────────────────────────────────────────────────────

#[async_trait::async_trait]
pub trait PalletRepository: Send + Sync {
    async fn find_by_id(&self, id: &PalletId) -> anyhow::Result<Option<Pallet>>;
    async fn find_open_for_hub(
        &self,
        hub_id: &HubId,
        destination_hub_id: Option<&HubId>,
    ) -> anyhow::Result<Option<Pallet>>;
    async fn save(&self, pallet: &Pallet) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
pub trait ContainerRepository: Send + Sync {
    async fn find_by_id(&self, id: &ContainerId) -> anyhow::Result<Option<Container>>;
    async fn save(&self, container: &Container) -> anyhow::Result<()>;
}

// ── Commands ──────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct LoadPieceCommand {
    pub tenant_id:           TenantId,
    pub hub_id:              HubId,
    pub destination_hub_id:  Option<HubId>,
    pub piece_awb:           ChildAwb,
    pub master_awb:          Awb,
    pub declared_weight_g:   u32,
    /// Hub re-weigh result. None = use declared weight (no discrepancy check).
    pub actual_weight_g:     Option<u32>,
    pub scanned_by:          uuid::Uuid,
}

#[derive(Debug)]
pub struct SealPalletCommand {
    pub pallet_id: PalletId,
    pub sealed_by: uuid::Uuid,
}

#[derive(Debug)]
pub struct CreateContainerCommand {
    pub tenant_id:         TenantId,
    pub transport_mode:    TransportMode,
    pub origin_hub_id:     HubId,
    pub destination_hub:   HubId,
    pub carrier_ref:       Option<String>,
}

#[derive(Debug)]
pub struct LoadPalletIntoContainerCommand {
    pub container_id: ContainerId,
    pub pallet_id:    PalletId,
    /// AWBs carried in this pallet — for container.master_awbs denormalisation.
    pub pallet_awbs:  Vec<Awb>,
}

#[derive(Debug)]
pub struct DepartContainerCommand {
    pub container_id:      ContainerId,
    pub eta:               Option<chrono::DateTime<Utc>>,
}

#[derive(Debug)]
pub struct ArriveContainerCommand {
    pub container_id: ContainerId,
}

// ── PalletService ─────────────────────────────────────────────────────────────

pub struct PalletService {
    pallet_repo:    Arc<dyn PalletRepository>,
    container_repo: Arc<dyn ContainerRepository>,
    kafka:          Arc<KafkaProducer>,
}

impl PalletService {
    pub fn new(
        pallet_repo:    Arc<dyn PalletRepository>,
        container_repo: Arc<dyn ContainerRepository>,
        kafka:          Arc<KafkaProducer>,
    ) -> Self {
        Self { pallet_repo, container_repo, kafka }
    }

    // ── Piece → Pallet ────────────────────────────────────────────────────────

    /// Load a scanned piece onto an open pallet at the hub.
    ///
    /// If no open pallet exists for this hub/destination, a new one is created
    /// automatically (auto-open policy).
    ///
    /// If `actual_weight_g` is provided and deviates from `declared_weight_g`
    /// by more than `WEIGHT_DISCREPANCY_THRESHOLD_G`, a `WeightDiscrepancyFound`
    /// Kafka event is emitted.
    pub async fn load_piece_onto_pallet(&self, cmd: LoadPieceCommand) -> AppResult<Pallet> {
        // ── Weight discrepancy check ─────────────────────────────────────────
        if let Some(actual) = cmd.actual_weight_g {
            let delta = (actual as i32 - cmd.declared_weight_g as i32).unsigned_abs();
            if delta >= WEIGHT_DISCREPANCY_THRESHOLD_G {
                self.emit_weight_discrepancy(
                    &cmd.piece_awb,
                    &cmd.master_awb,
                    cmd.declared_weight_g,
                    actual,
                    &cmd.tenant_id,
                    &cmd.hub_id,
                    cmd.scanned_by,
                )
                .await?;
            }
        }

        // ── Find or create open pallet ────────────────────────────────────────
        let mut pallet = match self.pallet_repo
            .find_open_for_hub(&cmd.hub_id, cmd.destination_hub_id.as_ref())
            .await
            .map_err(AppError::Internal)?
        {
            Some(p) => p,
            None => {
                let p = Pallet::new(
                    cmd.tenant_id.clone(),
                    cmd.hub_id.clone(),
                    cmd.destination_hub_id.clone(),
                );
                self.pallet_repo.save(&p).await.map_err(AppError::Internal)?;
                p
            }
        };

        let weight = cmd.actual_weight_g.unwrap_or(cmd.declared_weight_g);
        pallet.add_piece(cmd.piece_awb, weight)
            .map_err(|e| match e {
                PalletError::NotOpen(s) => AppError::BusinessRule(
                    format!("Pallet is not open: {s}"),
                ),
                PalletError::PieceLimitReached => AppError::BusinessRule(
                    "Pallet piece limit (999) reached".into(),
                ),
                PalletError::Empty => AppError::Internal(
                    anyhow::anyhow!("Unexpected Empty error from add_piece"),
                ),
            })?;

        self.pallet_repo.save(&pallet).await.map_err(AppError::Internal)?;
        Ok(pallet)
    }

    /// Seal a pallet — no more pieces can be added.
    /// Emits `PalletSealed` Kafka event.
    pub async fn seal_pallet(&self, cmd: SealPalletCommand) -> AppResult<Pallet> {
        let mut pallet = self.pallet_repo
            .find_by_id(&cmd.pallet_id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound {
                resource: "Pallet",
                id: cmd.pallet_id.inner().to_string(),
            })?;

        pallet.seal(cmd.sealed_by)
            .map_err(|e| match e {
                PalletError::Empty   => AppError::BusinessRule("Cannot seal an empty pallet".into()),
                PalletError::NotOpen(s) => AppError::BusinessRule(format!("Pallet not open: {s}")),
                PalletError::PieceLimitReached => AppError::Internal(
                    anyhow::anyhow!("Unexpected PieceLimitReached from seal"),
                ),
            })?;

        self.pallet_repo.save(&pallet).await.map_err(AppError::Internal)?;

        // ── Emit PalletSealed ─────────────────────────────────────────────────
        let event = Event::new(
            "hub-ops",
            "pallet.sealed",
            pallet.tenant_id.inner(),
            PalletSealed {
                pallet_id:       pallet.id.inner(),
                tenant_id:       pallet.tenant_id.inner(),
                hub_id:          pallet.origin_hub_id.inner(),
                destination_hub: pallet.destination_hub
                    .as_ref()
                    .map(|h| h.inner()),
                piece_count:     pallet.piece_count() as u16,
                total_weight_kg: pallet.total_weight_kg(),
                sealed_by:       cmd.sealed_by,
                sealed_at:       pallet.sealed_at.unwrap_or_else(Utc::now).to_rfc3339(),
            },
        );
        self.kafka
            .publish_event(topics::PALLET_SEALED, &event)
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(
            pallet_id   = %pallet.id.inner(),
            piece_count = pallet.piece_count(),
            weight_kg   = pallet.total_weight_kg(),
            "Pallet sealed"
        );
        Ok(pallet)
    }

    // ── Pallet → Container ────────────────────────────────────────────────────

    /// Create a new container in Planning state.
    pub async fn create_container(&self, cmd: CreateContainerCommand) -> AppResult<Container> {
        let mut container = Container::new(
            cmd.tenant_id,
            cmd.transport_mode,
            cmd.origin_hub_id,
            cmd.destination_hub,
        );
        container.carrier_ref = cmd.carrier_ref;
        self.container_repo.save(&container).await.map_err(AppError::Internal)?;
        tracing::info!(container_id = %container.id.inner(), "Container created");
        Ok(container)
    }

    /// Load a sealed pallet into a container.
    pub async fn load_pallet_into_container(
        &self,
        cmd: LoadPalletIntoContainerCommand,
    ) -> AppResult<Container> {
        let mut container = self.container_repo
            .find_by_id(&cmd.container_id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound {
                resource: "Container",
                id: cmd.container_id.inner().to_string(),
            })?;

        container.load_pallet(cmd.pallet_id, cmd.pallet_awbs)
            .map_err(|e| match e {
                ContainerError::AlreadyDeparted => AppError::BusinessRule(
                    "Container has already departed".into(),
                ),
                ContainerError::AlreadyLoaded => AppError::BusinessRule(
                    "Pallet is already loaded in this container".into(),
                ),
                ContainerError::Empty | ContainerError::InvalidTransition { .. } => {
                    AppError::Internal(anyhow::anyhow!("{e}"))
                }
            })?;

        self.container_repo.save(&container).await.map_err(AppError::Internal)?;
        Ok(container)
    }

    /// Finalise the manifest — no more loading after this.
    pub async fn finalise_manifest(&self, container_id: ContainerId) -> AppResult<Container> {
        let mut container = self.container_repo
            .find_by_id(&container_id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound {
                resource: "Container",
                id: container_id.inner().to_string(),
            })?;

        container.finalise_manifest()
            .map_err(|e| AppError::BusinessRule(e.to_string()))?;

        self.container_repo.save(&container).await.map_err(AppError::Internal)?;
        Ok(container)
    }

    /// Depart container — transitions to InTransit, emits `ContainerDeparted`.
    pub async fn depart_container(&self, cmd: DepartContainerCommand) -> AppResult<Container> {
        let mut container = self.container_repo
            .find_by_id(&cmd.container_id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound {
                resource: "Container",
                id: cmd.container_id.inner().to_string(),
            })?;

        container.depart(cmd.eta)
            .map_err(|e| AppError::BusinessRule(e.to_string()))?;

        self.container_repo.save(&container).await.map_err(AppError::Internal)?;

        // ── Emit ContainerDeparted ────────────────────────────────────────────
        let master_awbs: Vec<String> = container.master_awbs
            .iter()
            .map(|a| a.as_str().to_string())
            .collect();

        let event = Event::new(
            "hub-ops",
            "container.departed",
            container.tenant_id.inner(),
            ContainerDeparted {
                container_id:      container.id.inner(),
                tenant_id:         container.tenant_id.inner(),
                origin_hub_id:     container.origin_hub_id.inner(),
                destination_hub:   container.destination_hub.inner(),
                transport_mode:    format!("{:?}", container.transport_mode).to_lowercase(),
                pallet_count:      container.pallet_count() as u16,
                loose_piece_count: container.loose_piece_count() as u16,
                carrier_ref:       container.carrier_ref.clone(),
                departed_at:       container.departed_at.unwrap_or_else(Utc::now).to_rfc3339(),
                eta:               container.estimated_arrival.map(|t| t.to_rfc3339()),
                master_awbs:       master_awbs.clone(),
            },
        );
        self.kafka
            .publish_event(topics::CONTAINER_DEPARTED, &event)
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(
            container_id  = %container.id.inner(),
            awb_count     = master_awbs.len(),
            transport     = ?container.transport_mode,
            "Container departed"
        );
        Ok(container)
    }

    /// Record container arrival at destination hub. Emits `ContainerArrived`.
    pub async fn arrive_container(&self, cmd: ArriveContainerCommand) -> AppResult<Container> {
        let mut container = self.container_repo
            .find_by_id(&cmd.container_id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound {
                resource: "Container",
                id: cmd.container_id.inner().to_string(),
            })?;

        container.arrive()
            .map_err(|e| AppError::BusinessRule(e.to_string()))?;

        self.container_repo.save(&container).await.map_err(AppError::Internal)?;

        let master_awbs: Vec<String> = container.master_awbs
            .iter()
            .map(|a| a.as_str().to_string())
            .collect();

        let event = Event::new(
            "hub-ops",
            "container.arrived",
            container.tenant_id.inner(),
            ContainerArrived {
                container_id:    container.id.inner(),
                tenant_id:       container.tenant_id.inner(),
                destination_hub: container.destination_hub.inner(),
                arrived_at:      container.arrived_at.unwrap_or_else(Utc::now).to_rfc3339(),
                master_awbs:     master_awbs.clone(),
            },
        );
        self.kafka
            .publish_event(topics::CONTAINER_ARRIVED, &event)
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(
            container_id = %container.id.inner(),
            awb_count    = master_awbs.len(),
            "Container arrived"
        );
        Ok(container)
    }

    // ── Internal helpers ──────────────────────────────────────────────────────

    async fn emit_weight_discrepancy(
        &self,
        piece_awb:       &ChildAwb,
        master_awb:      &Awb,
        declared_grams:  u32,
        actual_grams:    u32,
        tenant_id:       &TenantId,
        hub_id:          &HubId,
        scanned_by:      uuid::Uuid,
    ) -> AppResult<()> {
        let delta = actual_grams as i32 - declared_grams as i32;
        let event = Event::new(
            "hub-ops",
            "piece.weight_discrepancy",
            tenant_id.inner(),
            WeightDiscrepancyFound {
                piece_awb:      piece_awb.as_str().to_string(),
                master_awb:     master_awb.as_str().to_string(),
                shipment_id:    uuid::Uuid::nil(), // populated by hub scan handler
                tenant_id:      tenant_id.inner(),
                merchant_id:    uuid::Uuid::nil(), // populated by hub scan handler
                hub_id:         hub_id.inner(),
                declared_grams,
                actual_grams,
                delta_grams:    delta,
                found_at:       Utc::now().to_rfc3339(),
                found_by:       scanned_by,
            },
        );
        self.kafka
            .publish_event(topics::WEIGHT_DISCREPANCY_FOUND, &event)
            .await
            .map_err(AppError::Internal)?;

        tracing::warn!(
            piece_awb       = piece_awb.as_str(),
            declared_grams,
            actual_grams,
            delta_grams     = delta,
            "Weight discrepancy detected"
        );
        Ok(())
    }
}
