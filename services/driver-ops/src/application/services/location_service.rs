use std::sync::Arc;
use logisticos_errors::{AppError, AppResult};
use logisticos_types::{Coordinates, DriverId, TenantId};
use logisticos_events::{producer::KafkaProducer, topics, envelope::Event};

use crate::{
    application::commands::UpdateLocationCommand,
    domain::{
        entities::{Driver, DriverLocation, DriverStatus},
        events::DriverLocationUpdated,
        repositories::{DriverRepository, LocationRepository},
        value_objects::STALE_LOCATION_THRESHOLD_MINUTES,
    },
};

pub struct LocationService {
    driver_repo: Arc<dyn DriverRepository>,
    location_repo: Arc<dyn LocationRepository>,
    kafka: Arc<KafkaProducer>,
}

impl LocationService {
    pub fn new(
        driver_repo: Arc<dyn DriverRepository>,
        location_repo: Arc<dyn LocationRepository>,
        kafka: Arc<KafkaProducer>,
    ) -> Self {
        Self { driver_repo, location_repo, kafka }
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

        // Update driver's live position in the main drivers table
        let mut driver = self.load_driver(driver_id).await?;

        driver.update_location(Coordinates { lat: cmd.lat, lng: cmd.lng });
        self.driver_repo.save(&driver).await.map_err(AppError::Internal)?;

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

    pub async fn go_online(&self, driver_id: &DriverId) -> AppResult<()> {
        let mut driver = self.load_driver(driver_id).await?;
        if driver.status == DriverStatus::Available {
            return Ok(()); // Already online — idempotent
        }
        driver.go_online();
        self.driver_repo.save(&driver).await.map_err(AppError::Internal)?;
        tracing::info!(driver_id = %driver_id, "Driver went online");
        Ok(())
    }

    pub async fn go_offline(&self, driver_id: &DriverId) -> AppResult<()> {
        let mut driver = self.load_driver(driver_id).await?;
        // Business rule: cannot go offline with an active route
        if driver.active_route_id.is_some() {
            return Err(AppError::BusinessRule(
                "Cannot go offline while assigned to an active route".into()
            ));
        }
        driver.go_offline();
        self.driver_repo.save(&driver).await.map_err(AppError::Internal)?;
        tracing::info!(driver_id = %driver_id, "Driver went offline");
        Ok(())
    }

    async fn load_driver(&self, user_id: &DriverId) -> AppResult<Driver> {
        // HTTP handlers always pass claims.user_id as driver_id; use find_by_user_id so
        // this works even when drivers.id was generated differently (e.g. via POST /v1/drivers).
        self.driver_repo.find_by_user_id(user_id.inner()).await.map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound { resource: "Driver", id: user_id.inner().to_string() })
    }
}
