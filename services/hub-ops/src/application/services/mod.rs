pub mod pallet_service;
pub use pallet_service::PalletService;
pub mod consolidation_service;
pub use consolidation_service::ConsolidationService;
pub mod hub_transfer_repositories;
pub use hub_transfer_repositories::{
    HubInventoryRepository, HubLocationRepository, HubRoutingConfigRepository, HubScanRepository,
    HubTransferManifestRepository,
};
pub mod hub_transfer_service;
pub use hub_transfer_service::{ClearCustomsCommand, DeconsolidateCommand, HubTransferService};

use std::sync::Arc;
use serde::Deserialize;
use uuid::Uuid;

use logisticos_errors::{AppError, AppResult};
use logisticos_types::TenantId;
use logisticos_events::{
    envelope::Event, payloads::HubLoadChanged, producer::KafkaProducer, topics,
};

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
    /// Publishes `HUB_LOAD_CHANGED`. `None` in tests that do not exercise Kafka.
    kafka:          Option<Arc<KafkaProducer>>,
}

/// Builds the `HUB_LOAD_CHANGED` event for a hub whose `current_load` was
/// `previous_load` before the mutation just applied to `hub`. Returns `None`
/// when the load did not change (e.g. dispatch on an empty hub), so no
/// spurious event is published.
pub(crate) fn hub_load_changed_event(hub: &Hub, previous_load: u32) -> Option<Event<HubLoadChanged>> {
    if hub.current_load == previous_load {
        return None;
    }
    Some(Event::new(
        "hub-ops",
        "hub.load.changed",
        hub.tenant_id.inner(),
        HubLoadChanged {
            hub_id:        hub.id.inner(),
            capacity:      hub.capacity as i32,
            previous_load: previous_load as i32,
            current_load:  hub.current_load as i32,
        },
    ))
}

impl HubService {
    pub fn new(
        hub_repo: Arc<dyn HubRepository>,
        induction_repo: Arc<dyn InductionRepository>,
    ) -> Self {
        Self { hub_repo, induction_repo, kafka: None }
    }

    /// Attach the producer used to publish hub load changes.
    pub fn with_events(mut self, kafka: Arc<KafkaProducer>) -> Self {
        self.kafka = Some(kafka);
        self
    }

    /// Publish after the hub row is saved. A failure is logged and never
    /// fails the request — the load change is already committed.
    async fn publish_load_changed(&self, hub: &Hub, previous_load: u32) {
        let (Some(kafka), Some(event)) = (&self.kafka, hub_load_changed_event(hub, previous_load)) else {
            return;
        };
        if let Err(e) = kafka.publish_event(topics::HUB_LOAD_CHANGED, &event).await {
            tracing::error!(
                hub_id = %hub.id.inner(),
                error = %e,
                "failed to publish hub load change"
            );
        }
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

    pub async fn get_hub(&self, tenant_id: &TenantId, hub_id: Uuid) -> AppResult<Hub> {
        let id = HubId::from_uuid(hub_id);
        self.hub_repo
            .find_by_id(&id)
            .await
            .map_err(AppError::internal)?
            .filter(|h| h.tenant_id == *tenant_id)
            .ok_or_else(|| AppError::NotFound { resource: "Hub", id: hub_id.to_string() })
    }

    pub async fn induct_parcel(&self, tenant_id: &TenantId, cmd: InductParcelCommand) -> AppResult<ParcelInduction> {
        let hub_id = HubId::from_uuid(cmd.hub_id);
        let mut hub = self.hub_repo
            .find_by_id(&hub_id)
            .await
            .map_err(AppError::internal)?
            .filter(|h| h.tenant_id == *tenant_id)
            .ok_or_else(|| AppError::NotFound { resource: "Hub", id: cmd.hub_id.to_string() })?;

        if let Some(existing) = self.induction_repo.find_by_shipment(cmd.shipment_id).await.map_err(AppError::internal)? {
            if existing.hub_id == hub_id && existing.tenant_id == *tenant_id {
                return Ok(existing);
            }
        }

        let previous_load = hub.current_load;
        hub.induct_parcel().map_err(|e| AppError::BusinessRule(e.to_string()))?;
        self.hub_repo.save(&hub).await.map_err(AppError::internal)?;
        self.publish_load_changed(&hub, previous_load).await;

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

    pub async fn sort_parcel(&self, tenant_id: &TenantId, cmd: SortParcelCommand) -> AppResult<ParcelInduction> {
        let mut induction = self.induction_repo
            .find_by_id(&InductionId::from_uuid(cmd.induction_id))
            .await
            .map_err(AppError::internal)?
            .filter(|i| i.tenant_id == *tenant_id)
            .ok_or_else(|| AppError::NotFound { resource: "Induction", id: cmd.induction_id.to_string() })?;

        induction.sort_to(cmd.zone, cmd.bay);
        self.induction_repo.save(&induction).await.map_err(AppError::internal)?;
        Ok(induction)
    }

    pub async fn dispatch_parcel(&self, tenant_id: &TenantId, induction_id: Uuid) -> AppResult<ParcelInduction> {
        let mut induction = self.induction_repo
            .find_by_id(&InductionId::from_uuid(induction_id))
            .await
            .map_err(AppError::internal)?
            .filter(|i| i.tenant_id == *tenant_id)
            .ok_or_else(|| AppError::NotFound { resource: "Induction", id: induction_id.to_string() })?;

        let mut hub = self.hub_repo
            .find_by_id(&induction.hub_id)
            .await
            .map_err(AppError::internal)?
            .filter(|h| h.tenant_id == *tenant_id)
            .ok_or_else(|| AppError::NotFound { resource: "Hub", id: induction.hub_id.inner().to_string() })?;

        induction.dispatch();
        let previous_load = hub.current_load;
        hub.dispatch_parcel();
        self.induction_repo.save(&induction).await.map_err(AppError::internal)?;
        self.hub_repo.save(&hub).await.map_err(AppError::internal)?;
        self.publish_load_changed(&hub, previous_load).await;
        Ok(induction)
    }

    pub async fn hub_manifest(
        &self,
        hub_id:    Uuid,
        tenant_id: &TenantId,
    ) -> AppResult<Vec<ParcelInduction>> {
        let all = self.induction_repo
            .list_active(&HubId::from_uuid(hub_id))
            .await
            .map_err(AppError::internal)?;
        // Filter by tenant to prevent cross-tenant data leakage.
        // list_active queries by hub_id only; the tenant guard is applied here
        // until the InductionRepository trait grows a tenant-scoped variant.
        Ok(all.into_iter().filter(|p| p.tenant_id == *tenant_id).collect())
    }
}

#[cfg(test)]
mod load_event_tests {
    use super::*;

    fn hub(capacity: u32) -> Hub {
        Hub::new(TenantId::new(), "H".into(), "addr".into(), 0.0, 0.0, capacity)
    }

    #[test]
    fn induction_event_carries_previous_and_current() {
        let mut h = hub(10);
        h.current_load = 4;
        let prev = h.current_load;
        h.induct_parcel().unwrap();
        let ev = hub_load_changed_event(&h, prev).expect("load changed");
        assert_eq!(ev.tenant_id, h.tenant_id.inner());
        assert_eq!(ev.data.hub_id, h.id.inner());
        assert_eq!(ev.data.capacity, 10);
        assert_eq!(ev.data.previous_load, 4);
        assert_eq!(ev.data.current_load, 5);
    }

    #[test]
    fn dispatch_event_carries_previous_and_current() {
        let mut h = hub(10);
        h.current_load = 7;
        let prev = h.current_load;
        h.dispatch_parcel();
        let ev = hub_load_changed_event(&h, prev).expect("load changed");
        assert_eq!(ev.data.previous_load, 7);
        assert_eq!(ev.data.current_load, 6);
    }

    #[test]
    fn no_event_when_load_unchanged() {
        let mut h = hub(10);
        let prev = h.current_load;
        h.dispatch_parcel(); // empty hub: no-op
        assert!(hub_load_changed_event(&h, prev).is_none());
    }
}

#[cfg(test)]
mod tenant_isolation_tests {
    use super::*;
    use std::sync::Mutex;

    struct FakeHubs { hubs: Mutex<Vec<Hub>>, saves: Mutex<u32> }
    #[async_trait::async_trait]
    impl HubRepository for FakeHubs {
        async fn find_by_id(&self, id: &HubId) -> anyhow::Result<Option<Hub>> {
            Ok(self.hubs.lock().unwrap().iter().find(|h| h.id.inner() == id.inner()).cloned())
        }
        async fn list(&self, _t: &TenantId) -> anyhow::Result<Vec<Hub>> { Ok(vec![]) }
        async fn save(&self, hub: &Hub) -> anyhow::Result<()> {
            *self.saves.lock().unwrap() += 1;
            let mut v = self.hubs.lock().unwrap();
            v.retain(|h| h.id.inner() != hub.id.inner());
            v.push(hub.clone());
            Ok(())
        }
    }

    struct FakeInductions { items: Mutex<Vec<ParcelInduction>>, saves: Mutex<u32> }
    #[async_trait::async_trait]
    impl InductionRepository for FakeInductions {
        async fn find_by_id(&self, id: &InductionId) -> anyhow::Result<Option<ParcelInduction>> {
            Ok(self.items.lock().unwrap().iter().find(|i| i.id.inner() == id.inner()).cloned())
        }
        async fn find_by_shipment(&self, sid: Uuid) -> anyhow::Result<Option<ParcelInduction>> {
            Ok(self.items.lock().unwrap().iter().find(|i| i.shipment_id == sid).cloned())
        }
        async fn list_active(&self, _h: &HubId) -> anyhow::Result<Vec<ParcelInduction>> { Ok(vec![]) }
        async fn save(&self, i: &ParcelInduction) -> anyhow::Result<()> {
            *self.saves.lock().unwrap() += 1;
            self.items.lock().unwrap().push(i.clone());
            Ok(())
        }
    }

    fn setup(hub: Hub, inductions: Vec<ParcelInduction>)
        -> (HubService, Arc<FakeHubs>, Arc<FakeInductions>)
    {
        let h = Arc::new(FakeHubs { hubs: Mutex::new(vec![hub]), saves: Mutex::new(0) });
        let i = Arc::new(FakeInductions { items: Mutex::new(inductions), saves: Mutex::new(0) });
        (HubService::new(h.clone(), i.clone()), h, i)
    }

    // The publish happens only after `hub_repo.save`, so "no hub save and
    // unchanged load" is the observable proof that nothing was published.

    #[tokio::test]
    async fn induct_into_foreign_tenant_hub_is_not_found_and_publishes_nothing() {
        let owner = TenantId::new();
        let hub = Hub::new(owner.clone(), "H".into(), "a".into(), 0.0, 0.0, 10);
        let hub_id = hub.id.inner();
        let (svc, hubs, inds) = setup(hub, vec![]);
        let err = svc.induct_parcel(&TenantId::new(), InductParcelCommand {
            hub_id, shipment_id: Uuid::new_v4(), tracking_number: "T".into(), inducted_by: None,
        }).await.unwrap_err();
        assert!(matches!(err, AppError::NotFound { .. }));
        assert_eq!(*hubs.saves.lock().unwrap(), 0);
        assert_eq!(*inds.saves.lock().unwrap(), 0);
        assert_eq!(hubs.hubs.lock().unwrap()[0].current_load, 0);
    }

    #[tokio::test]
    async fn dispatch_foreign_tenant_induction_is_not_found_and_publishes_nothing() {
        let owner = TenantId::new();
        let mut hub = Hub::new(owner.clone(), "H".into(), "a".into(), 0.0, 0.0, 10);
        hub.current_load = 3;
        let ind = ParcelInduction::new(hub.id.clone(), owner, Uuid::new_v4(), "T".into(), None);
        let ind_id = ind.id.inner();
        let (svc, hubs, inds) = setup(hub, vec![ind]);
        let err = svc.dispatch_parcel(&TenantId::new(), ind_id).await.unwrap_err();
        assert!(matches!(err, AppError::NotFound { .. }));
        assert_eq!(*hubs.saves.lock().unwrap(), 0);
        assert_eq!(*inds.saves.lock().unwrap(), 0);
        assert_eq!(hubs.hubs.lock().unwrap()[0].current_load, 3);
    }

    #[tokio::test]
    async fn sort_foreign_tenant_induction_is_not_found() {
        let owner = TenantId::new();
        let hub = Hub::new(owner.clone(), "H".into(), "a".into(), 0.0, 0.0, 10);
        let ind = ParcelInduction::new(hub.id.clone(), owner, Uuid::new_v4(), "T".into(), None);
        let ind_id = ind.id.inner();
        let (svc, _hubs, inds) = setup(hub, vec![ind]);
        let err = svc.sort_parcel(&TenantId::new(), SortParcelCommand {
            induction_id: ind_id, zone: "Z".into(), bay: "B".into(),
        }).await.unwrap_err();
        assert!(matches!(err, AppError::NotFound { .. }));
        assert_eq!(*inds.saves.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn same_tenant_induct_and_dispatch_still_work() {
        let owner = TenantId::new();
        let hub = Hub::new(owner.clone(), "H".into(), "a".into(), 0.0, 0.0, 10);
        let hub_id = hub.id.inner();
        let (svc, hubs, _) = setup(hub, vec![]);
        let ind = svc.induct_parcel(&owner, InductParcelCommand {
            hub_id, shipment_id: Uuid::new_v4(), tracking_number: "T".into(), inducted_by: None,
        }).await.unwrap();
        assert_eq!(hubs.hubs.lock().unwrap()[0].current_load, 1);
        svc.dispatch_parcel(&owner, ind.id.inner()).await.unwrap();
        assert_eq!(hubs.hubs.lock().unwrap()[0].current_load, 0);
    }
}
