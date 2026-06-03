//! Repository traits for the cross-border hub transfer entities.
//!
//! Persistence contracts only — Postgres implementations live in
//! `crate::infrastructure::db::hub_transfer`. The service layer (Slice C) wires
//! these to the HTTP handlers and Kafka emitters.

use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::entities::{
    HubInventory, HubLocation, HubRoutingConfig, HubScan, HubTransferManifest, InventoryUnit,
};

/// Append-only chain-of-custody scan log. There is intentionally no update or
/// delete method — scans are immutable.
#[async_trait]
pub trait HubScanRepository: Send + Sync {
    async fn append(&self, scan: &HubScan) -> anyhow::Result<()>;
    async fn list_for_shipment(
        &self,
        shipment_id: Uuid,
        tenant_id:   Uuid,
    ) -> anyhow::Result<Vec<HubScan>>;
}

#[async_trait]
pub trait HubLocationRepository: Send + Sync {
    async fn save(&self, location: &HubLocation) -> anyhow::Result<()>;
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<HubLocation>>;
    async fn list_for_hub(
        &self,
        hub_id:  Uuid,
        zone_id: Option<&str>,
    ) -> anyhow::Result<Vec<HubLocation>>;
}

#[async_trait]
pub trait HubInventoryRepository: Send + Sync {
    /// Insert or update in place (location moves are a single row update).
    async fn upsert(&self, inventory: &HubInventory) -> anyhow::Result<()>;
    /// Re-point an existing inventory record to a new location. Returns rows affected
    /// (0 = unknown id).
    async fn move_to(&self, inventory_id: Uuid, to_location_id: Uuid) -> anyhow::Result<u64>;
    async fn list_at_location(&self, location_id: Uuid) -> anyhow::Result<Vec<HubInventory>>;
    async fn find_by_unit(
        &self,
        tenant_id: Uuid,
        unit:      &InventoryUnit,
    ) -> anyhow::Result<Option<HubInventory>>;
}

#[async_trait]
pub trait HubTransferManifestRepository: Send + Sync {
    async fn save(&self, manifest: &HubTransferManifest) -> anyhow::Result<()>;
    async fn find_by_container(&self, container_id: Uuid)
        -> anyhow::Result<Option<HubTransferManifest>>;
    /// Containers currently in customs hold at a hub (the Customs Queue).
    async fn list_in_customs(
        &self,
        hub_id:    Uuid,
        tenant_id: Uuid,
    ) -> anyhow::Result<Vec<HubTransferManifest>>;
}

#[async_trait]
pub trait HubRoutingConfigRepository: Send + Sync {
    async fn save(&self, config: &HubRoutingConfig) -> anyhow::Result<()>;
    /// Resolve the rule for a destination zone. Falls back to None when no rule
    /// is configured (caller decides the default routing behaviour).
    async fn find(
        &self,
        tenant_id:        Uuid,
        hub_id:           Uuid,
        destination_zone: &str,
    ) -> anyhow::Result<Option<HubRoutingConfig>>;
    async fn list_for_hub(
        &self,
        hub_id:    Uuid,
        tenant_id: Uuid,
    ) -> anyhow::Result<Vec<HubRoutingConfig>>;
}
