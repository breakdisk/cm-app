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
    /// Cancel a planned or in-progress route by ID.
    /// Returns `true` if the route was found and cancelled, `false` if already
    /// completed/cancelled or not found.
    async fn cancel(&self, id: &RouteId, tenant_id: &TenantId) -> anyhow::Result<bool>;
}

#[async_trait]
pub trait DriverAssignmentRepository: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<DriverAssignment>>;
    async fn find_active_by_driver(&self, driver_id: &DriverId) -> anyhow::Result<Option<DriverAssignment>>;
    async fn save(&self, assignment: &DriverAssignment) -> anyhow::Result<()>;
    /// Admin operation: cancel any `pending` or `accepted` assignment for
    /// this driver so they re-enter the auto-dispatch pool.
    /// Returns `true` if a row was cancelled, `false` if the driver had no
    /// active assignment.
    async fn cancel_active_for_driver(&self, driver_id: &DriverId) -> anyhow::Result<bool>;

    /// Closes the active (`pending`/`accepted`) assignment of a route that has
    /// been cancelled, as `cancelled`. Does nothing unless the route's status
    /// is already `cancelled`, so a retry after a partial failure is safe.
    /// Returns `true` if a row was closed.
    async fn cancel_active_for_route(&self, tenant_id: Uuid, route_id: Uuid) -> anyhow::Result<bool>;

    /// The route a task outcome belongs to: by `driver_ops.tasks.id` first,
    /// then by shipment (route stop or task). Never by driver id (ADR-0015).
    async fn find_route_for_outcome(
        &self,
        tenant_id: Uuid,
        task_id: Option<Uuid>,
        shipment_id: Option<Uuid>,
    ) -> anyhow::Result<Option<Uuid>>;

    /// Closes the route's active assignment as `completed` and completes the
    /// route, but only when the route has tasks, none are open, and every
    /// route stop has a task. One atomic statement; replay-safe.
    /// Returns `true` if anything changed.
    async fn close_if_finished(&self, tenant_id: Uuid, route_id: Uuid) -> anyhow::Result<bool>;

    /// Repair pass: closes up to `limit` accepted assignments whose work has
    /// ended but whose outcome event was missed. Returns the number closed.
    async fn close_finished_assignments(&self, limit: i64) -> anyhow::Result<u64>;

    /// Inserts the assignment together with the number of `TASK_ASSIGNED` legs
    /// dispatch will emit for it, in one statement: the assignment never exists
    /// without its leg count, so the close predicate cannot fall back to the
    /// stop-count rule that closes a quick-dispatch route after its pickup.
    /// Use this wherever legs are emitted for the assignment.
    async fn save_with_expected_tasks(&self, assignment: &DriverAssignment, expected: i32) -> anyhow::Result<()>;
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

    /// Gig payout snapshot source: Some(per_delivery_rate_cents) when the
    /// driver is part_time, None for full_time — the driver app must never
    /// show full-time drivers a price.
    async fn gig_rate_cents(&self, driver_id: &DriverId) -> anyhow::Result<Option<i64>>;

    /// The carrier (partner) a driver belongs to, or None for the platform's
    /// own drivers. Stamped on `TaskAssigned` so order-intake can link the
    /// shipment to the partner that handles it.
    ///
    /// Defaults to None so an implementor that has no carrier data (a test
    /// double) behaves as "platform driver", which links nothing.
    async fn carrier_id(&self, _driver_id: &DriverId) -> anyhow::Result<Option<uuid::Uuid>> {
        Ok(None)
    }
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
