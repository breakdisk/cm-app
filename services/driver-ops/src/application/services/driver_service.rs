use std::sync::Arc;
use logisticos_errors::{AppError, AppResult};
use logisticos_types::{DriverId, TenantId};
use uuid::Uuid;

use crate::{
    application::commands::{RegisterDriverCommand, UpdateDriverCommand},
    domain::{
        entities::{Driver, DriverStatus, DriverType},
        repositories::DriverRepository,
    },
};

pub struct DriverService {
    driver_repo: Arc<dyn DriverRepository>,
}

impl DriverService {
    pub fn new(driver_repo: Arc<dyn DriverRepository>) -> Self {
        Self { driver_repo }
    }

    /// Register a new driver profile linked to an identity service user.
    /// The user must already exist in identity.users — driver-ops trusts the caller to verify this.
    pub async fn register(&self, tenant_id: TenantId, cmd: RegisterDriverCommand) -> AppResult<Driver> {
        // Idempotency: if a driver profile already exists for this user, return it
        if let Some(existing) = self.driver_repo.find_by_user_id(cmd.user_id).await.map_err(AppError::Internal)? {
            return Ok(existing);
        }

        let now = chrono::Utc::now();
        let driver = Driver {
            id: DriverId::from_uuid(cmd.user_id),
            tenant_id,
            user_id: cmd.user_id,
            first_name: cmd.first_name,
            last_name: cmd.last_name,
            phone: cmd.phone,
            status: DriverStatus::Offline,
            current_location: None,
            last_location_at: None,
            vehicle_id: cmd.vehicle_id,
            active_route_id: None,
            is_active: true,
            driver_type: DriverType::FullTime,
            per_delivery_rate_cents: 0,
            cod_commission_rate_bps: 0,
            zone: None,
            vehicle_type: None,
            carrier_id: cmd.carrier_id,
            hub_id:     None,
            created_at: now,
            updated_at: now,
        };

        self.driver_repo.save(&driver).await.map_err(AppError::Internal)?;
        tracing::info!(driver_id = %driver.id, user_id = %driver.user_id, "Driver registered");
        Ok(driver)
    }

    /// Partner-portal profile update: commission/rate/zone/vehicle/contact fields.
    pub async fn update(&self, tenant_id: &TenantId, driver_id: &DriverId, cmd: UpdateDriverCommand) -> AppResult<Driver> {
        let mut driver = self.get(driver_id).await?;
        if driver.tenant_id.inner() != tenant_id.inner() {
            return Err(AppError::NotFound { resource: "Driver", id: driver_id.inner().to_string() });
        }
        if let Some(v) = cmd.first_name              { driver.first_name = v; }
        if let Some(v) = cmd.last_name               { driver.last_name  = v; }
        if let Some(v) = cmd.phone                   { driver.phone      = v; }
        if let Some(v) = cmd.driver_type             { driver.driver_type = v; }
        if let Some(v) = cmd.per_delivery_rate_cents { driver.per_delivery_rate_cents = v; }
        if let Some(v) = cmd.cod_commission_rate_bps { driver.cod_commission_rate_bps = v; }
        if let Some(v) = cmd.zone                    { driver.zone = Some(v); }
        if let Some(v) = cmd.vehicle_type            { driver.vehicle_type = Some(v); }
        if let Some(v) = cmd.is_active               { driver.is_active = v; }
        if cmd.carrier_id.is_some()                  { driver.carrier_id = cmd.carrier_id; }
        driver.updated_at = chrono::Utc::now();
        self.driver_repo.save(&driver).await.map_err(AppError::Internal)?;
        Ok(driver)
    }

    pub async fn list_by_tenant(&self, tenant_id: &TenantId) -> AppResult<Vec<Driver>> {
        self.driver_repo.list_by_tenant(tenant_id).await.map_err(AppError::Internal)
    }

    pub async fn get(&self, driver_id: &DriverId) -> AppResult<Driver> {
        self.driver_repo.find_by_id(driver_id).await.map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound { resource: "Driver", id: driver_id.inner().to_string() })
    }

    /// Admin hard-delete — permanently removes the driver profile.
    ///
    /// Guards:
    /// - Driver must belong to the calling tenant (tenant isolation).
    /// - Driver must have no active route (prevents mid-trip orphan).
    /// - Driver must be Offline or inactive (blocks deleting an en-route courier).
    ///
    /// This does NOT delete the corresponding identity.users row — that lives
    /// in the identity service and must be deactivated there separately if needed.
    /// Task history is preserved via cascade-SET NULL in the DB foreign key.
    pub async fn delete_driver(
        &self,
        tenant_id: &TenantId,
        driver_id: &DriverId,
    ) -> AppResult<()> {
        let driver = self.get(driver_id).await?;

        if driver.tenant_id.inner() != tenant_id.inner() {
            return Err(AppError::NotFound { resource: "Driver", id: driver_id.inner().to_string() });
        }
        if driver.active_route_id.is_some() {
            return Err(AppError::BusinessRule(
                "Cannot delete a driver with an active route — cancel their route first".into(),
            ));
        }
        if !matches!(driver.status, DriverStatus::Offline) {
            return Err(AppError::BusinessRule(
                "Cannot delete an online driver — set them offline first".into(),
            ));
        }

        let deleted = self.driver_repo.delete(driver_id).await.map_err(AppError::Internal)?;
        if !deleted {
            return Err(AppError::NotFound { resource: "Driver", id: driver_id.inner().to_string() });
        }
        tracing::info!(driver_id = %driver_id, tenant_id = %tenant_id, "Driver hard-deleted by admin");
        Ok(())
    }

    /// Called by Kafka consumer when dispatch assigns a route to a driver.
    pub async fn assign_route(&self, driver_id: &DriverId, route_id: Uuid) -> AppResult<()> {
        let mut driver = self.get(driver_id).await?;
        if !driver.can_accept_route() {
            return Err(AppError::BusinessRule(
                "Driver is not available for route assignment".into()
            ));
        }
        driver.active_route_id = Some(route_id);
        driver.status = DriverStatus::EnRoute;
        driver.updated_at = chrono::Utc::now();
        self.driver_repo.save(&driver).await.map_err(AppError::Internal)?;
        Ok(())
    }

    /// Called when all tasks on a route are completed.
    pub async fn clear_route(&self, driver_id: &DriverId) -> AppResult<()> {
        let mut driver = self.get(driver_id).await?;
        driver.active_route_id = None;
        driver.status = DriverStatus::Available;
        driver.updated_at = chrono::Utc::now();
        self.driver_repo.save(&driver).await.map_err(AppError::Internal)?;
        Ok(())
    }

    /// Admin override — set a driver's status directly. Used by the Dispatch
    /// Console (and ops scripts) to take a driver out of the auto-dispatch
    /// pool without requiring the driver to toggle in their own app.
    ///
    /// Reachable only with FLEET_MANAGE permission (admin role). We guard
    /// the en-route case explicitly: forcing offline a driver who's mid-route
    /// would orphan an in-progress assignment, so the admin must cancel the
    /// route first. For idle drivers (Available/Offline) the transition is
    /// immediate.
    pub async fn set_status(
        &self,
        tenant_id: &TenantId,
        driver_id: &DriverId,
        new_status: DriverStatus,
        actor_id: Uuid,
    ) -> AppResult<Driver> {
        let mut driver = self.get(driver_id).await?;
        if driver.tenant_id.inner() != tenant_id.inner() {
            return Err(AppError::NotFound { resource: "Driver", id: driver_id.inner().to_string() });
        }

        // Refuse to flip a driver mid-route — the active route would be
        // stranded with no driver. Admin must cancel the route first
        // (separate dispatch endpoint).
        if driver.active_route_id.is_some() && new_status != DriverStatus::EnRoute {
            return Err(AppError::BusinessRule(
                "Driver has an active route — cancel the route before changing status".into()
            ));
        }

        let prior = driver.status;
        driver.status = new_status;
        driver.updated_at = chrono::Utc::now();
        self.driver_repo.save(&driver).await.map_err(AppError::Internal)?;
        tracing::info!(
            driver_id = %driver_id,
            actor_id  = %actor_id,
            from      = ?prior,
            to        = ?new_status,
            "Admin set driver status"
        );
        Ok(driver)
    }
}
