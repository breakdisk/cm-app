use async_trait::async_trait;
use logisticos_types::{DriverId, RouteId, TenantId};
use crate::domain::entities::{Route, DriverAssignment};
use uuid::Uuid;

#[async_trait]
pub trait RouteRepository: Send + Sync {
    async fn find_by_id(&self, id: &RouteId) -> anyhow::Result<Option<Route>>;
    async fn find_active_by_driver(&self, driver_id: &DriverId) -> anyhow::Result<Option<Route>>;
    async fn list_by_tenant(&self, tenant_id: &TenantId) -> anyhow::Result<Vec<Route>>;
    async fn save(&self, route: &Route) -> anyhow::Result<()>;
}

#[async_trait]
pub trait DriverAssignmentRepository: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<DriverAssignment>>;
    async fn find_active_by_driver(&self, driver_id: &DriverId) -> anyhow::Result<Option<DriverAssignment>>;
    async fn save(&self, assignment: &DriverAssignment) -> anyhow::Result<()>;
}

/// Read-only view of driver availability — sourced from the driver-ops service DB
/// or a replicated read view. Dispatch does not own driver state.
#[async_trait]
pub trait DriverAvailabilityRepository: Send + Sync {
    /// Returns drivers who are online, not currently assigned, within `radius_km` of `coords`.
    async fn find_available_near(
        &self,
        tenant_id: &TenantId,
        coords: logisticos_types::Coordinates,
        radius_km: f64,
    ) -> anyhow::Result<Vec<AvailableDriver>>;
}

#[derive(Debug, Clone)]
pub struct AvailableDriver {
    pub driver_id: DriverId,
    pub name: String,
    pub distance_km: f64,
    pub location: logisticos_types::Coordinates,
    pub active_stop_count: u32,  // how many stops already assigned today
    pub vehicle_type: Option<String>,
}
