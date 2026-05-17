use std::sync::Arc;
use chrono::{DateTime, Utc};
use tokio::sync::watch;
use logisticos_errors::AppResult;
use uuid::Uuid;

use crate::domain::repositories::MarketplaceRepository;
use crate::domain::entities::{VehicleListing, ListingStatus};

/// Dead-Man's Switch (DMS) background task
/// Runs every 60 seconds to validate and suspend listings that no longer meet
/// operational preconditions (driver online, vehicle operational, window not expired, etc.)
pub struct DeadMansSwitch {
    repo: Arc<dyn MarketplaceRepository>,
    http_client: reqwest::Client,
    fleet_service_url: String,
    driver_service_url: String,
    interval_secs: u64,
}

impl DeadMansSwitch {
    pub fn new(
        repo: Arc<dyn MarketplaceRepository>,
        fleet_service_url: String,
        driver_service_url: String,
    ) -> Self {
        Self {
            repo,
            http_client: reqwest::Client::new(),
            fleet_service_url,
            driver_service_url,
            interval_secs: 60,
        }
    }

    /// Start the DMS background task (runs until shutdown signal)
    pub async fn start(self, mut shutdown_rx: watch::Receiver<bool>) -> anyhow::Result<()> {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(self.interval_secs));

        loop {
            tokio::select! {
                _ = shutdown_rx.changed() => {
                    tracing::info!("DMS background task shutting down");
                    break;
                }
                _ = interval.tick() => {
                    if let Err(e) = self.tick().await {
                        tracing::error!("DMS tick failed: {e}");
                    }
                }
            }
        }
        Ok(())
    }

    /// Single DMS evaluation tick — validates all active listings
    async fn tick(&self) -> AppResult<()> {
        // Fetch all active listings
        let listings = self.repo.list_all_active_listings().await?;

        for listing in listings {
            if let Err(e) = self.validate_and_suspend_if_needed(&listing).await {
                tracing::warn!(
                    listing_id = %listing.id,
                    "Failed to validate listing: {e}"
                );
            }
        }

        Ok(())
    }

    /// Validate a single listing against all DMS conditions
    /// Returns Ok(()) if valid, or suspends and returns the suspension reason
    async fn validate_and_suspend_if_needed(&self, listing: &VehicleListing) -> AppResult<()> {
        if listing.status != ListingStatus::Active {
            return Ok(()); // Skip already suspended or withdrawn listings
        }

        let now = Utc::now();

        // 1. Check: availability window expired
        if now > listing.idle_until {
            return self.suspend_listing(listing.id, "window_expired").await;
        }

        // 2. Check: vehicle operational (fleet API call)
        if !self.is_vehicle_operational(listing.id).await? {
            return self.suspend_listing(listing.id, "vehicle_not_operational").await;
        }

        // 3. Check: partner has available drivers (driver-ops API call)
        if !self.partner_has_available_drivers(listing.carrier_id).await? {
            return self.suspend_listing(listing.id, "no_online_driver").await;
        }

        // 4. TODO: Check carrier status (would call carrier repo directly)
        // if !self.is_carrier_active(listing.carrier_id).await? {
        //     return self.suspend_listing(listing.id, "carrier_suspended").await;
        // }

        Ok(())
    }

    /// Call fleet API to check if vehicle is operational
    async fn is_vehicle_operational(&self, vehicle_id: Uuid) -> AppResult<bool> {
        let url = format!("{}/v1/internal/vehicles/{}/operational", self.fleet_service_url, vehicle_id);

        match self.http_client.get(&url).send().await {
            Ok(response) if response.status() == 200 => {
                match response.json::<serde_json::Value>().await {
                    Ok(body) => {
                        let is_operational = body
                            .get("data")
                            .and_then(|v| v.get("is_operational"))
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        Ok(is_operational)
                    }
                    Err(_) => Ok(false),
                }
            }
            _ => Ok(false), // Assume non-operational if fleet service is down
        }
    }

    /// Call driver-ops API to check if partner has available drivers
    async fn partner_has_available_drivers(&self, partner_id: Uuid) -> AppResult<bool> {
        let url = format!(
            "{}/v1/internal/partners/{}/available-drivers",
            self.driver_service_url,
            partner_id
        );

        match self.http_client.get(&url).send().await {
            Ok(response) if response.status() == 200 => {
                match response.json::<serde_json::Value>().await {
                    Ok(body) => {
                        let has_available = body
                            .get("data")
                            .and_then(|v| v.get("has_available_drivers"))
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        Ok(has_available)
                    }
                    Err(_) => Ok(false),
                }
            }
            _ => Ok(false), // Assume no drivers if driver-ops service is down
        }
    }

    /// Suspend a listing with a given reason
    async fn suspend_listing(&self, listing_id: Uuid, reason: &str) -> AppResult<()> {
        let mut listing = self
            .repo
            .find_listing_by_id(listing_id)
            .await?
            .ok_or_else(|| logisticos_errors::AppError::NotFound {
                resource: "VehicleListing",
                id: listing_id.to_string(),
            })?;

        if listing.status == ListingStatus::Active {
            listing.status = ListingStatus::Suspended;
            listing.updated_at = Utc::now();
            self.repo.update_listing(&listing).await?;

            tracing::info!(
                listing_id = %listing.id,
                reason = reason,
                "Listing suspended by DMS"
            );
        }

        Ok(())
    }
}
