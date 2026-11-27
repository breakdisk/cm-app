use std::sync::Arc;
use chrono::Utc;
use logisticos_errors::{AppError, AppResult};
use logisticos_types::{Coordinates, DriverId, TenantId};
use logisticos_events::{producer::KafkaProducer, topics, envelope::Event, payloads::DriverAvailable};
use uuid::Uuid;

use crate::{
    application::commands::UpdateLocationCommand,
    domain::{
        entities::{hos_clock, Driver, DriverLocation, DriverStatus, DriverType, HosClock, HosPolicy},
        events::DriverLocationUpdated,
        repositories::{DriverRepository, DutySessionRepository, LocationRepository},
        value_objects::STALE_LOCATION_THRESHOLD_MINUTES,
    },
};

pub struct LocationService {
    driver_repo: Arc<dyn DriverRepository>,
    location_repo: Arc<dyn LocationRepository>,
    kafka: Arc<KafkaProducer>,
    duty_repo: Arc<dyn DutySessionRepository>,
    hos_policy: HosPolicy,
}

impl LocationService {
    pub fn new(
        driver_repo: Arc<dyn DriverRepository>,
        location_repo: Arc<dyn LocationRepository>,
        kafka: Arc<KafkaProducer>,
        duty_repo: Arc<dyn DutySessionRepository>,
        hos_policy: HosPolicy,
    ) -> Self {
        Self { driver_repo, location_repo, kafka, duty_repo, hos_policy }
    }

    pub async fn update_location(
        &self,
        driver_id: &DriverId,
        tenant_id: &TenantId,
        cmd: UpdateLocationCommand,
    ) -> AppResult<()> {
        let location = DriverLocation {
            driver_id: driver_id.inner(),
            tenant_id: tenant_id.inner(),
            lat: cmd.lat,
            lng: cmd.lng,
            accuracy_m: cmd.accuracy_m,
            speed_kmh: cmd.speed_kmh,
            heading: cmd.heading,
            battery_pct: cmd.battery_pct,
            recorded_at: cmd.recorded_at,
            received_at: chrono::Utc::now(),
        };

        // Business rule: reject stale GPS fixes (offline backfill older than 5 minutes)
        if location.is_stale() {
            tracing::warn!(driver_id = %driver_id, "Rejected stale GPS location");
            return Err(AppError::BusinessRule(format!(
                "Location fix is more than {} minutes old",
                STALE_LOCATION_THRESHOLD_MINUTES
            )));
        }

        // Business rule: reject implausible speeds (GPS multipath noise)
        if !location.is_plausible_speed() {
            tracing::warn!(
                driver_id = %driver_id,
                speed = ?cmd.speed_kmh,
                "Rejected implausible speed"
            );
            return Err(AppError::BusinessRule("Location rejected: implausible speed".into()));
        }

        // Persist to time-series table
        self.location_repo.record(&location).await.map_err(AppError::Internal)?;

        // Update driver's denormalised position in the drivers table (best-effort).
        let driver_row = self.driver_repo
            .find_by_user_id(driver_id.inner()).await.map_err(AppError::Internal)?;
        if let Some(mut driver) = driver_row {

            driver.update_location(Coordinates { lat: cmd.lat, lng: cmd.lng });
            self.driver_repo.save(&driver).await.map_err(AppError::Internal)?;
        }

        // Publish event — dispatch service caches this for proximity scoring,
        // merchant portal map subscribes via WebSocket relay.
        let event = Event::new("driver-ops", "driver.location_updated", tenant_id.inner(), DriverLocationUpdated {
            driver_id: driver_id.inner(),
            tenant_id: tenant_id.inner(),
            lat: cmd.lat,
            lng: cmd.lng,
            speed_kmh: cmd.speed_kmh,
            heading: cmd.heading,
            recorded_at: cmd.recorded_at,
        });
        self.kafka.publish_event(topics::DRIVER_LOCATION_UPDATED, &event).await
            .map_err(AppError::Internal)?;

        Ok(())
    }

    pub async fn go_online(&self, driver_id: &DriverId, tenant_id: &TenantId) -> AppResult<()> {
        let mut driver = self.find_or_create_driver(driver_id, tenant_id).await?;
        // Always run go_online() — never short-circuit on `status == Available`.
        // A driver row may legitimately be Available but have `is_active=false`
        // (e.g. an ops override that bulk-deactivated stale drivers). The short
        // circuit caused those drivers to stay invisible to dispatch forever
        // because `is_active` never got reconciled on subsequent logins.
        // go_online() is idempotent; the only cost is one extra UPDATE.
        driver.go_online();
        self.driver_repo.save(&driver).await.map_err(AppError::Internal)?;
        tracing::info!(driver_id = %driver_id, is_active = driver.is_active, "Driver went online");

        // Hours of service: open a duty session unless one is already open.
        self.open_duty(tenant_id.inner(), driver_id.inner()).await;

        // Notify dispatch to retry any pending queue items for this tenant.
        let event = Event::new("driver-ops", "driver.available", tenant_id.inner(), DriverAvailable {
            driver_id:    driver_id.inner(),
            tenant_id:    tenant_id.inner(),
            available_at: chrono::Utc::now().to_rfc3339(),
        });
        if let Err(e) = self.kafka.publish_event(topics::DRIVER_AVAILABLE, &event).await {
            tracing::warn!(driver_id = %driver_id, err = %e, "Failed to publish DRIVER_AVAILABLE (non-fatal)");
        }

        Ok(())
    }

    pub async fn go_offline(&self, driver_id: &DriverId) -> AppResult<()> {
        // If driver row doesn't exist there's nothing to go offline from — return Ok.
        let Some(mut driver) = self.driver_repo
            .find_by_user_id(driver_id.inner()).await.map_err(AppError::Internal)?
        else {
            return Ok(());
        };
        if driver.active_route_id.is_some() {
            return Err(AppError::BusinessRule(
                "Cannot go offline while assigned to an active route".into()
            ));
        }
        driver.go_offline();
        self.driver_repo.save(&driver).await.map_err(AppError::Internal)?;
        tracing::info!(driver_id = %driver_id, "Driver went offline");

        self.close_duty(driver.tenant_id.inner(), driver_id.inner()).await;
        Ok(())
    }

    /// The driver's hours-of-service clock over the policy's rolling window.
    pub async fn hours_of_service(&self, driver_id: &DriverId, tenant_id: &TenantId) -> AppResult<HosClock> {
        let now = Utc::now();
        let sessions = self.duty_repo
            .list_overlapping(tenant_id.inner(), driver_id.inner(), now - self.hos_policy.window())
            .await
            .map_err(AppError::Internal)?;
        Ok(hos_clock(&sessions, self.hos_policy, now))
    }

    // The clock is display-only, so a failed duty write is logged and never
    // stops a driver going on or off duty.
    async fn open_duty(&self, tenant_id: Uuid, driver_id: Uuid) {
        if let Err(e) = self.duty_repo.open(tenant_id, driver_id, Utc::now()).await {
            tracing::warn!(driver_id = %driver_id, err = %e, "Failed to open duty session (non-fatal)");
        }
    }

    async fn close_duty(&self, tenant_id: Uuid, driver_id: Uuid) {
        if let Err(e) = self.duty_repo.close_open(tenant_id, driver_id, Utc::now()).await {
            tracing::warn!(driver_id = %driver_id, err = %e, "Failed to close duty session (non-fatal)");
        }
    }

    /// Returns the driver row, creating a minimal stub on first login if one doesn't exist yet.
    async fn find_or_create_driver(&self, user_id: &DriverId, tenant_id: &TenantId) -> AppResult<Driver> {
        if let Some(d) = self.driver_repo.find_by_user_id(user_id.inner()).await.map_err(AppError::Internal)? {
            return Ok(d);
        }
        let now = chrono::Utc::now();
        let driver = Driver {
            id:                      user_id.clone(),
            tenant_id:               tenant_id.clone(),
            user_id:                 user_id.inner(),
            first_name:              "Driver".into(),
            last_name:               String::new(),
            phone:                   String::new(),
            status:                  DriverStatus::Offline,
            current_location:        None,
            last_location_at:        None,
            vehicle_id:              None,
            active_route_id:         None,
            is_active:               true,
            driver_type:             DriverType::FullTime,
            per_delivery_rate_cents: 0,
            cod_commission_rate_bps: 0,
            zone:                    None,
            vehicle_type:            None,
            carrier_id:              None,
            hub_id:                  None,
            created_at:              now,
            updated_at:              now,
        };
        self.driver_repo.save(&driver).await.map_err(AppError::Internal)?;
        tracing::info!(driver_id = %user_id, "Auto-created driver profile on first login");
        Ok(driver)
    }

}
