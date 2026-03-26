use async_trait::async_trait;
use logisticos_types::{DriverId, TenantId};
use uuid::Uuid;
use crate::domain::entities::{Driver, DriverTask, DriverLocation};

#[async_trait]
pub trait DriverRepository: Send + Sync {
    async fn find_by_id(&self, id: &DriverId) -> anyhow::Result<Option<Driver>>;
    async fn find_by_user_id(&self, user_id: Uuid) -> anyhow::Result<Option<Driver>>;
    async fn list_by_tenant(&self, tenant_id: &TenantId) -> anyhow::Result<Vec<Driver>>;
    async fn save(&self, driver: &Driver) -> anyhow::Result<()>;
}

#[async_trait]
pub trait TaskRepository: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<DriverTask>>;
    async fn list_by_driver(&self, driver_id: &DriverId) -> anyhow::Result<Vec<DriverTask>>;
    async fn list_by_route(&self, route_id: Uuid) -> anyhow::Result<Vec<DriverTask>>;
    async fn save(&self, task: &DriverTask) -> anyhow::Result<()>;
    async fn bulk_save(&self, tasks: &[DriverTask]) -> anyhow::Result<()>;
}

#[async_trait]
pub trait LocationRepository: Send + Sync {
    /// Append a location record to the time-series table.
    async fn record(&self, location: &DriverLocation) -> anyhow::Result<()>;
    /// Get the most recent location for a driver.
    async fn latest(&self, driver_id: &DriverId) -> anyhow::Result<Option<DriverLocation>>;
    /// Get location trail over a time range (for replay/audit).
    async fn history(
        &self,
        driver_id: &DriverId,
        from: chrono::DateTime<chrono::Utc>,
        to: chrono::DateTime<chrono::Utc>,
    ) -> anyhow::Result<Vec<DriverLocation>>;
}
