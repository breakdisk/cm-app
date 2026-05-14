use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use logisticos_errors::{AppError, AppResult};

use crate::domain::{
    entities::{
        BookingStatus, ListingStatus, MarketplaceBooking, SizeClass, VehicleListing,
    },
    repositories::MarketplaceRepository,
};

// ── Input types ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateListingCommand {
    pub vehicle_plate:                String,
    pub size_class:                   String,
    pub max_weight_kg:                f32,
    pub max_volume_m3:                Option<f32>,
    pub base_price_cents:             i64,
    pub per_km_cents:                 i64,
    pub per_kg_cents:                 Option<i64>,
    pub service_area_label:           String,
    pub idle_from:                    DateTime<Utc>,
    pub idle_until:                   DateTime<Utc>,
    #[serde(default = "default_response_window")]
    pub carrier_response_window_mins: i32,
}

fn default_response_window() -> i32 { 15 }

#[derive(Debug, Deserialize)]
pub struct UpdateListingPatch {
    pub status:                       Option<String>,
    pub base_price_cents:             Option<i64>,
    pub per_km_cents:                 Option<i64>,
    pub per_kg_cents:                 Option<i64>,
    pub idle_until:                   Option<DateTime<Utc>>,
    pub service_area_label:           Option<String>,
    pub max_weight_kg:                Option<f32>,
    pub max_volume_m3:                Option<f32>,
    pub carrier_response_window_mins: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct RecordPickupInput {
    pub picked_up_by:  Option<String>,
    pub pickup_notes:  Option<String>,
}

// ── Service ───────────────────────────────────────────────────────────────────

pub struct MarketplaceService {
    repo: Arc<dyn MarketplaceRepository>,
}

impl MarketplaceService {
    pub fn new(repo: Arc<dyn MarketplaceRepository>) -> Self { Self { repo } }

    pub async fn create_listing(
        &self,
        tenant_id: Uuid,
        carrier_id: Uuid,
        cmd: CreateListingCommand,
    ) -> AppResult<VehicleListing> {
        let size_class = SizeClass::from_str(&cmd.size_class)
            .map_err(|e| AppError::BusinessRule(e.to_string()))?;

        if cmd.idle_until <= cmd.idle_from {
            return Err(AppError::BusinessRule("idle_until must be after idle_from".into()));
        }

        let listing = VehicleListing::new(
            tenant_id,
            carrier_id,
            cmd.vehicle_plate,
            size_class,
            cmd.max_weight_kg,
            cmd.max_volume_m3,
            cmd.base_price_cents,
            cmd.per_km_cents,
            cmd.per_kg_cents,
            cmd.service_area_label,
            cmd.idle_from,
            cmd.idle_until,
            cmd.carrier_response_window_mins,
        );
        self.repo.create_listing(&listing).await.map_err(AppError::internal)?;
        Ok(listing)
    }

    pub async fn list_listings(
        &self,
        carrier_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<VehicleListing>> {
        self.repo
            .list_listings_by_carrier(carrier_id, limit.clamp(1, 100), offset.max(0))
            .await
            .map_err(AppError::internal)
    }

    pub async fn update_listing(
        &self,
        listing_id: Uuid,
        carrier_id: Uuid,
        patch: UpdateListingPatch,
    ) -> AppResult<VehicleListing> {
        let mut listing = self
            .repo
            .find_listing_by_id(listing_id)
            .await
            .map_err(AppError::internal)?
            .ok_or_else(|| AppError::NotFound { resource: "VehicleListing", id: listing_id.to_string() })?;

        if listing.carrier_id != carrier_id {
            return Err(AppError::NotFound { resource: "VehicleListing", id: listing_id.to_string() });
        }

        if let Some(s) = patch.status {
            listing.status = ListingStatus::from_str(&s)
                .map_err(|e| AppError::BusinessRule(e.to_string()))?;
        }
        if let Some(v) = patch.base_price_cents             { listing.base_price_cents = v; }
        if let Some(v) = patch.per_km_cents                 { listing.per_km_cents = v; }
        if patch.per_kg_cents.is_some()                     { listing.per_kg_cents = patch.per_kg_cents; }
        if let Some(v) = patch.idle_until                   { listing.idle_until = v; }
        if let Some(v) = patch.service_area_label           { listing.service_area_label = v; }
        if let Some(v) = patch.max_weight_kg                { listing.max_weight_kg = v; }
        if patch.max_volume_m3.is_some()                    { listing.max_volume_m3 = patch.max_volume_m3; }
        if let Some(v) = patch.carrier_response_window_mins { listing.carrier_response_window_mins = v; }
        listing.updated_at = Utc::now();

        self.repo.update_listing(&listing).await.map_err(AppError::internal)?;
        Ok(listing)
    }

    pub async fn delete_listing(&self, listing_id: Uuid, carrier_id: Uuid) -> AppResult<()> {
        let listing = self
            .repo
            .find_listing_by_id(listing_id)
            .await
            .map_err(AppError::internal)?
            .ok_or_else(|| AppError::NotFound { resource: "VehicleListing", id: listing_id.to_string() })?;

        if listing.carrier_id != carrier_id {
            return Err(AppError::NotFound { resource: "VehicleListing", id: listing_id.to_string() });
        }
        self.repo.delete_listing(listing_id).await.map_err(AppError::internal)?;
        Ok(())
    }

    pub async fn list_bookings(
        &self,
        carrier_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<MarketplaceBooking>> {
        self.repo
            .list_bookings_by_carrier(carrier_id, limit.clamp(1, 100), offset.max(0))
            .await
            .map_err(AppError::internal)
    }

    pub async fn accept_booking(
        &self,
        booking_id: Uuid,
        carrier_id: Uuid,
    ) -> AppResult<MarketplaceBooking> {
        let mut booking = self.load_owned_booking(booking_id, carrier_id).await?;
        booking.accept().map_err(|e| AppError::BusinessRule(e.to_string()))?;
        self.repo.save_booking(&booking).await.map_err(AppError::internal)?;
        Ok(booking)
    }

    pub async fn reject_booking(
        &self,
        booking_id: Uuid,
        carrier_id: Uuid,
    ) -> AppResult<MarketplaceBooking> {
        let mut booking = self.load_owned_booking(booking_id, carrier_id).await?;
        booking.reject().map_err(|e| AppError::BusinessRule(e.to_string()))?;
        self.repo.save_booking(&booking).await.map_err(AppError::internal)?;
        Ok(booking)
    }

    pub async fn record_pickup(
        &self,
        booking_id: Uuid,
        carrier_id: Uuid,
        input: RecordPickupInput,
    ) -> AppResult<MarketplaceBooking> {
        let mut booking = self.load_owned_booking(booking_id, carrier_id).await?;
        booking
            .record_pickup(input.picked_up_by, input.pickup_notes)
            .map_err(|e| AppError::BusinessRule(e.to_string()))?;
        self.repo.save_booking(&booking).await.map_err(AppError::internal)?;
        Ok(booking)
    }

    /// Mask consumer PII for bookings that haven't been accepted yet.
    /// Pre-accept: consumer name shown as initials only; phone not revealed.
    pub fn apply_pii_mask(mut booking: MarketplaceBooking) -> MarketplaceBooking {
        if booking.status == BookingStatus::Pending || booking.status == BookingStatus::Rejected {
            // Reduce to first-initial + last-name initial e.g. "María Reyes" → "M. R."
            booking.consumer_name = initials(&booking.consumer_name);
            booking.consumer_phone = None;
        }
        booking
    }

    async fn load_owned_booking(
        &self,
        booking_id: Uuid,
        carrier_id: Uuid,
    ) -> AppResult<MarketplaceBooking> {
        let booking = self
            .repo
            .find_booking_by_id(booking_id)
            .await
            .map_err(AppError::internal)?
            .ok_or_else(|| AppError::NotFound { resource: "MarketplaceBooking", id: booking_id.to_string() })?;

        if booking.carrier_id != carrier_id {
            return Err(AppError::NotFound { resource: "MarketplaceBooking", id: booking_id.to_string() });
        }
        Ok(booking)
    }
}

fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|w| w.chars().next())
        .map(|c| format!("{c}."))
        .collect::<Vec<_>>()
        .join(" ")
}
