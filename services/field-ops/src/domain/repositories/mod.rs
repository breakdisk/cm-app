//! Repository contracts.
//!
//! TENANCY: every method takes `tenant_id` as its first argument, by design.
//! There is no database-level policy enforcing isolation in this schema (see
//! migration 0001), so the signature is the enforcement point — a method that
//! can be called without a tenant is a method that can leak across tenants.

use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::entities::Courier;

#[async_trait]
pub trait CourierRepository: Send + Sync {
    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<Option<Courier>>;
    async fn find_by_user(&self, tenant_id: Uuid, user_id: Uuid) -> anyhow::Result<Option<Courier>>;
    async fn save(&self, courier: &Courier) -> anyhow::Result<()>;

    /// Every courier in the tenant, newest first. The ops roster.
    ///
    /// Deliberately unfiltered by status: the surface that manages couriers has
    /// to show the suspended and the offline ones, which is precisely what a
    /// dispatch-shaped query hides.
    async fn list_for_tenant(
        &self,
        tenant_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<Courier>>;

    /// Record what the compliance service last said about a courier.
    ///
    /// Keyed on `user_id` rather than the courier id: the compliance profile's
    /// `entity_id` is the identity user, and while ADR-0015 collapses the two
    /// (`courier.id = user_id`) that is an invariant of *registration*, not
    /// something an inbound event from another service should be made to
    /// depend on.
    ///
    /// Returns whether a row was updated. A `false` is expected and not an
    /// error: compliance publishes for driver-ops drivers on the same topic,
    /// and most of them are not couriers.
    async fn update_compliance(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        status: &str,
        assignable: bool,
    ) -> anyhow::Result<bool>;

    /// Dispatchable couriers within `radius_km` of a point, nearest first.
    ///
    /// Whether the compliance term is applied is fixed at construction, not
    /// passed per call: it is a deployment rollout flag, and threading it
    /// through every call site invites one of them to pass the wrong value.
    async fn find_available_near(
        &self,
        tenant_id: Uuid,
        lat: f64,
        lng: f64,
        radius_km: f64,
        limit: i64,
    ) -> anyhow::Result<Vec<Courier>>;
}
