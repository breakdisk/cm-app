pub mod pallet_service;
pub use pallet_service::PalletService;

use std::sync::Arc;
use serde::Deserialize;
use uuid::Uuid;

use logisticos_errors::{AppError, AppResult};
use logisticos_types::TenantId;

use crate::domain::entities::{Hub, HubId, InductionId, ParcelInduction};

#[async_trait::async_trait]
pub trait HubRepository: Send + Sync {
    async fn find_by_id(&self, id: &HubId) -> anyhow::Result<Option<Hub>>;
    async fn list(&self, tenant_id: &TenantId) -> anyhow::Result<Vec<Hub>>;
    async fn save(&self, hub: &Hub) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
pub trait InductionRepository: Send + Sync {
    async fn find_by_id(&self, id: &InductionId) -> anyhow::Result<Option<ParcelInduction>>;
    async fn find_by_shipment(&self, shipment_id: Uuid) -> anyhow::Result<Option<ParcelInduction>>;
    async fn list_active(&self, hub_id: &HubId) -> anyhow::Result<Vec<ParcelInduction>>;
    async fn save(&self, induction: &ParcelInduction) -> anyhow::Result<()>;
}

#[derive(Debug, Deserialize)]
pub struct CreateHubCommand {
    pub name:          String,
    pub address:       String,
    pub lat:           f64,
    pub lng:           f64,
    pub capacity:      u32,
    pub serving_zones: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct InductParcelCommand {
    pub hub_id:          Uuid,
    pub shipment_id:     Uuid,
    pub tracking_number: String,
    pub inducted_by:     Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct SortParcelCommand {
    pub induction_id: Uuid,
    pub zone:         String,
    pub bay:          String,
}

pub struct HubService {
    hub_repo:       Arc<dyn HubRepository>,
    induction_repo: Arc<dyn InductionRepository>,
}

impl HubService {
    pub fn new(
        hub_repo: Arc<dyn HubRepository>,
        induction_repo: Arc<dyn InductionRepository>,
    ) -> Self {
        Self { hub_repo, induction_repo }
    }

    pub async fn create_hub(&self, tenant_id: &TenantId, cmd: CreateHubCommand) -> AppResult<Hub> {
        let mut hub = Hub::new(
            tenant_id.clone(),
            cmd.name,
            cmd.address,
            cmd.lat,
            cmd.lng,
            cmd.capacity,
        );
        hub.serving_zones = cmd.serving_zones;
        self.hub_repo.save(&hub).await.map_err(AppError::internal)?;
        Ok(hub)
    }

    pub async fn list_hubs(&self, tenant_id: &TenantId) -> AppResult<Vec<Hub>> {
        self.hub_repo.list(tenant_id).await.map_err(AppError::internal)
    }

    pub async fn induct_parcel(&self, cmd: InductParcelCommand) -> AppResult<ParcelInduction> {
        let hub_id = HubId::from_uuid(cmd.hub_id);
        let mut hub = self.hub_repo
            .find_by_id(&hub_id)
            .await
            .map_err(AppError::internal)?
            .ok_or_else(|| AppError::NotFound { resource: "Hub", id: cmd.hub_id.to_string() })?;

        if let Some(existing) = self.induction_repo.find_by_shipment(cmd.shipment_id).await.map_err(AppError::internal)? {
            if existing.hub_id == hub_id {
                return Ok(existing);
            }
        }

        hub.induct_parcel().map_err(|e| AppError::BusinessRule(e.to_string()))?;
        self.hub_repo.save(&hub).await.map_err(AppError::internal)?;

        let induction = ParcelInduction::new(
            hub_id,
            hub.tenant_id.clone(),
            cmd.shipment_id,
            cmd.tracking_number,
            cmd.inducted_by,
        );
        self.induction_repo.save(&induction).await.map_err(AppError::internal)?;
        Ok(induction)
    }

    pub async fn sort_parcel(&self, cmd: SortParcelCommand) -> AppResult<ParcelInduction> {
        let mut induction = self.induction_repo
            .find_by_id(&InductionId::from_uuid(cmd.induction_id))
            .await
            .map_err(AppError::internal)?
            .ok_or_else(|| AppError::NotFound { resource: "Induction", id: cmd.induction_id.to_string() })?;

        induction.sort_to(cmd.zone, cmd.bay);
        self.induction_repo.save(&induction).await.map_err(AppError::internal)?;
        Ok(induction)
    }

    pub async fn dispatch_parcel(&self, induction_id: Uuid) -> AppResult<ParcelInduction> {
        let mut induction = self.induction_repo
            .find_by_id(&InductionId::from_uuid(induction_id))
            .await
            .map_err(AppError::internal)?
            .ok_or_else(|| AppError::NotFound { resource: "Induction", id: induction_id.to_string() })?;

        let mut hub = self.hub_repo
            .find_by_id(&induction.hub_id)
            .await
            .map_err(AppError::internal)?
            .ok_or_else(|| AppError::NotFound { resource: "Hub", id: induction.hub_id.inner().to_string() })?;

        induction.dispatch();
        hub.dispatch_parcel();
        self.induction_repo.save(&induction).await.map_err(AppError::internal)?;
        self.hub_repo.save(&hub).await.map_err(AppError::internal)?;
        Ok(induction)
    }

    pub async fn hub_manifest(&self, hub_id: Uuid) -> AppResult<Vec<ParcelInduction>> {
        self.induction_repo
            .list_active(&HubId::from_uuid(hub_id))
            .await
            .map_err(AppError::internal)
    }
}
