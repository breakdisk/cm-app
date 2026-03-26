use async_trait::async_trait;
use uuid::Uuid;

use logisticos_types::TenantId;

use crate::domain::entities::{Vehicle, VehicleId};

#[async_trait]
pub trait VehicleRepository: Send + Sync {
    async fn find_by_id(&self, id: &VehicleId) -> anyhow::Result<Option<Vehicle>>;
    async fn find_by_driver(&self, tenant_id: &TenantId, driver_id: Uuid) -> anyhow::Result<Option<Vehicle>>;
    async fn list(&self, tenant_id: &TenantId, limit: i64, offset: i64) -> anyhow::Result<Vec<Vehicle>>;
    async fn list_maintenance_due(&self, tenant_id: &TenantId, within_days: i64) -> anyhow::Result<Vec<Vehicle>>;
    async fn save(&self, vehicle: &Vehicle) -> anyhow::Result<()>;
    async fn count(&self, tenant_id: &TenantId) -> anyhow::Result<i64>;
}
