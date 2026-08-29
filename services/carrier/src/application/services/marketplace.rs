use std::sync::Arc;

use chrono::{DateTime, Utc};
use logisticos_events::{envelope::Event, producer::KafkaProducer, topics};
use serde::Deserialize;
use uuid::Uuid;

use logisticos_errors::{AppError, AppResult};

use crate::domain::{
    entities::{
        BookingPaymentMethod, BookingPaymentStatus, BookingStatus, ListingStatus,
        MarketplaceBooking, SizeClass, VehicleListing,
    },
    repositories::MarketplaceRepository,
};

/// A hold opened at `services/payments`, and where to send the merchant to
/// complete it.
#[derive(Debug, Clone)]
pub struct AuthorizedIntent {
    pub intent_id:    Uuid,
    pub checkout_url: String,
}

/// The money side of a marketplace booking.
///
/// A trait rather than a concrete client so the service is testable without a
/// payments service, and so a deployment with no gateway configured can wire
/// `None` and lose only the `Online` method -- see `MarketplaceService::new`.
#[async_trait::async_trait]
pub trait BookingPayments: Send + Sync {
    /// Ring-fence the quote. Does not take the money: a booking is `Pending`
    /// until a carrier answers, and a merchant must not be charged for a truck
    /// that gets rejected.
    async fn authorize(
        &self,
        tenant_id: Uuid,
        booking_id: Uuid,
        amount_cents: i64,
        currency: &str,
        return_url: &str,
    ) -> anyhow::Result<AuthorizedIntent>;

    /// The carrier accepted -- take what was held.
    async fn capture(&self, intent_id: Uuid) -> anyhow::Result<()>;

    /// Rejected, or the response window ran out -- release it.
    async fn void(&self, intent_id: Uuid) -> anyhow::Result<()>;
}

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

/// What a merchant states about the job. Deliberately no price field: the
/// quote is computed server-side from the listing's own rate card, and a
/// client-supplied price is a client-supplied discount.
#[derive(Debug, Deserialize)]
pub struct CreateBookingCommand {
    pub listing_id:      Uuid,
    /// The shipment this vehicle is being booked for.
    pub shipment_id:     Uuid,
    pub awb:             String,
    #[serde(default)]
    pub consumer_name:   String,
    #[serde(default)]
    pub consumer_phone:  Option<String>,
    #[serde(default)]
    pub pickup_label:    String,
    #[serde(default)]
    pub dropoff_label:   String,
    pub cargo_weight_kg: f32,
    #[serde(default)]
    pub cargo_volume_m3: Option<f32>,
    /// Priced against the listing's `per_km_cents`.
    #[serde(default)]
    pub distance_km:     f32,
    pub pickup_at:       DateTime<Utc>,
    /// `"invoice"` (default -- bill it on the merchant's ordinary invoice run,
    /// which is what the carrier-side handlers always implicitly assumed) or
    /// `"online"`.
    #[serde(default)]
    pub payment_method:  BookingPaymentMethod,
}

/// A placed booking, plus the card page when one is needed.
#[derive(Debug)]
pub struct PlacedBooking {
    pub booking:      MarketplaceBooking,
    /// `Some` only for `Online`. The merchant must complete this before the
    /// carrier ever sees the booking.
    pub checkout_url: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum BookingError {
    #[error("listing {0} is not available to book")]
    Unavailable(Uuid),
    #[error("this vehicle cannot carry {requested}kg (max {max}kg)")]
    TooHeavy { requested: f32, max: f32 },
    #[error("online payment is not configured for this deployment")]
    OnlineUnavailable,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<BookingError> for AppError {
    fn from(e: BookingError) -> Self {
        match e {
            BookingError::Unavailable(id) =>
                AppError::NotFound { resource: "VehicleListing", id: id.to_string() },
            BookingError::TooHeavy { .. } | BookingError::OnlineUnavailable =>
                AppError::BusinessRule(e.to_string()),
            BookingError::Other(inner) => AppError::internal(inner),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct RecordPickupInput {
    pub picked_up_by:  Option<String>,
    pub pickup_notes:  Option<String>,
}

// ── Service ───────────────────────────────────────────────────────────────────

pub struct MarketplaceService {
    repo:  Arc<dyn MarketplaceRepository>,
    kafka: Arc<KafkaProducer>,
    /// `None` when no payments URL is configured. Online booking is then
    /// refused with a clear business-rule error rather than the service
    /// failing to start -- payment is an optional capability here, exactly as
    /// it is in OmniDeliv and order-intake.
    payments: Option<Arc<dyn BookingPayments>>,
    currency: String,
    payment_return_url_base: String,
}

impl MarketplaceService {
    pub fn new(repo: Arc<dyn MarketplaceRepository>, kafka: Arc<KafkaProducer>) -> Self {
        Self {
            repo,
            kafka,
            payments: None,
            currency: "AED".into(),
            payment_return_url_base: String::new(),
        }
    }

    /// Enables the `Online` payment method. Without this every booking is
    /// `Invoice`, which is the behaviour that predates this feature.
    pub fn with_payments(
        mut self,
        payments: Arc<dyn BookingPayments>,
        currency: String,
        payment_return_url_base: String,
    ) -> Self {
        self.payments = Some(payments);
        self.currency = currency;
        self.payment_return_url_base = payment_return_url_base;
        self
    }

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

    /// Listings a merchant can book right now. The one marketplace read that
    /// deliberately crosses carriers -- choosing between them is the product.
    pub async fn find_available_listings(
        &self,
        tenant_id:     Uuid,
        min_weight_kg: f32,
        size_class:    Option<&str>,
        limit:         i64,
    ) -> AppResult<Vec<VehicleListing>> {
        if let Some(sc) = size_class {
            // Validated rather than passed straight to SQL, so an unknown class
            // is a 400 and not an empty result that reads as "nothing available".
            SizeClass::from_str(sc).map_err(|e| AppError::BusinessRule(e.to_string()))?;
        }
        self.repo
            .find_available_listings(tenant_id, min_weight_kg.max(0.0), size_class, Utc::now(), limit.clamp(1, 100))
            .await
            .map_err(AppError::internal)
    }

    /// Place a booking against a listing.
    ///
    /// Two shapes, and the difference is when the carrier learns about it:
    ///
    ///  - `Invoice` -- the booking is `Pending` and immediately visible to the
    ///    carrier, exactly as the carrier-side handlers have always assumed.
    ///  - `Online` -- an authorization hold is opened and a card page returned.
    ///    The carrier is shown nothing until `payment.intent.authorized` lands
    ///    (see `infrastructure::booking_payment_consumer`), because a booking
    ///    the merchant has not funded is a truck held for a job that may never
    ///    be paid for.
    pub async fn create_booking(
        &self,
        tenant_id: Uuid,
        booked_by_user_id: Uuid,
        cmd: CreateBookingCommand,
    ) -> Result<PlacedBooking, BookingError> {
        let listing = self
            .repo
            .find_listing_by_id(cmd.listing_id)
            .await?
            .ok_or(BookingError::Unavailable(cmd.listing_id))?;

        // Cross-tenant listings read as absent rather than forbidden, so ids
        // cannot be enumerated across tenants.
        if listing.tenant_id != tenant_id || listing.status != ListingStatus::Active {
            return Err(BookingError::Unavailable(cmd.listing_id));
        }

        let now = Utc::now();
        if listing.idle_from > now || listing.idle_until <= now {
            return Err(BookingError::Unavailable(cmd.listing_id));
        }

        if cmd.cargo_weight_kg > listing.max_weight_kg {
            return Err(BookingError::TooHeavy {
                requested: cmd.cargo_weight_kg,
                max: listing.max_weight_kg,
            });
        }

        let booking = MarketplaceBooking::place(
            &listing,
            booked_by_user_id,
            cmd.shipment_id,
            cmd.awb,
            cmd.consumer_name,
            cmd.consumer_phone,
            cmd.pickup_label,
            cmd.dropoff_label,
            cmd.cargo_weight_kg,
            cmd.cargo_volume_m3,
            cmd.distance_km,
            cmd.pickup_at,
        );

        match cmd.payment_method {
            BookingPaymentMethod::Invoice => {
                self.repo.create_booking(&booking).await?;
                self.publish_requested(&booking).await;
                Ok(PlacedBooking { booking, checkout_url: None })
            }
            BookingPaymentMethod::Online => {
                let payments = self.payments.as_ref().ok_or(BookingError::OnlineUnavailable)?;
                let mut booking = booking.with_online_payment();

                // Persisted BEFORE the gateway call, unlike the Invoice branch.
                // `authorize` stamps our booking id as the intent's
                // reference_id, and the webhook can arrive before this function
                // returns -- a consumer looking up a row that does not exist yet
                // would drop a real authorization on the floor.
                self.repo.create_booking(&booking).await?;

                let return_url = format!(
                    "{}?booking_id={}",
                    self.payment_return_url_base.trim_end_matches('/'),
                    booking.id,
                );
                let authorized = payments
                    .authorize(tenant_id, booking.id, booking.quoted_price_cents, &self.currency, &return_url)
                    .await?;

                booking.payment_intent_id = Some(authorized.intent_id);
                booking.payment_checkout_url = Some(authorized.checkout_url.clone());
                // A second write rather than one: the id and URL only exist
                // after the call the first write had to precede.
                self.repo
                    .save_booking_payment_reference(&booking)
                    .await?;

                Ok(PlacedBooking { booking, checkout_url: Some(authorized.checkout_url) })
            }
        }
    }

    /// A merchant's own bookings, newest first.
    pub async fn list_bookings_for_merchant(
        &self,
        tenant_id: Uuid,
        user_id:   Uuid,
        limit:     i64,
        offset:    i64,
    ) -> AppResult<Vec<MarketplaceBooking>> {
        self.repo
            .list_bookings_by_booker(tenant_id, user_id, limit.clamp(1, 100), offset.max(0))
            .await
            .map_err(AppError::internal)
    }

    /// Releases holds on pending bookings whose carrier never answered.
    ///
    /// Returns how many were expired. One bad row is logged and skipped rather
    /// than aborting the batch -- a sweep that stops at the first failure leaves
    /// every later merchant's money ring-fenced.
    pub async fn sweep_expired_bookings(&self) -> anyhow::Result<usize> {
        const BATCH: i64 = 200;
        let now = Utc::now();
        let mut expired = 0;

        for booking in self.repo.list_pending_bookings(BATCH).await? {
            if !booking.response_window_expired(now) {
                continue;
            }
            match self.expire_booking(booking).await {
                Ok(true) => expired += 1,
                Ok(false) => {}
                Err(e) => tracing::error!(err = %e, "booking expiry failed; will retry next tick"),
            }
        }
        Ok(expired)
    }

    /// Void first, then cancel. A failed void leaves the merchant's funds
    /// ring-fenced, which is the money-safety-critical direction, so the
    /// booking stays `Pending` and the next tick tries again rather than
    /// advancing past a hold that was never released.
    async fn expire_booking(&self, mut booking: MarketplaceBooking) -> anyhow::Result<bool> {
        if booking.payment_status == BookingPaymentStatus::Authorized {
            let Some(intent_id) = booking.payment_intent_id else {
                anyhow::bail!("booking {} is authorized with no intent id", booking.id);
            };
            let payments = self
                .payments
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("booking {} holds funds but payments is not configured", booking.id))?;
            payments.void(intent_id).await?;
            booking.payment_voided().map_err(|e| anyhow::anyhow!("{e}"))?;
        }

        booking.expire()?;
        self.repo.save_booking(&booking).await?;
        tracing::warn!(booking_id = %booking.id, carrier_id = %booking.carrier_id,
            "carrier did not answer inside the response window — booking cancelled");
        Ok(true)
    }

    async fn publish_requested(&self, booking: &MarketplaceBooking) {
        let payload = serde_json::json!({
            "booking_id":         booking.id,
            "listing_id":         booking.listing_id,
            "carrier_id":         booking.carrier_id,
            "tenant_id":          booking.tenant_id,
            "quoted_price_cents": booking.quoted_price_cents,
            "pickup_at":          booking.pickup_at.to_rfc3339(),
            "requested_at":       Utc::now().to_rfc3339(),
        });
        let ev = Event::new(
            "logisticos/carrier", "marketplace.booking.requested", booking.tenant_id, payload,
        );
        if let Err(e) = self.kafka.publish_event(topics::MARKETPLACE_BOOKING_REQUESTED, &ev).await {
            tracing::warn!("Failed to publish marketplace.booking.requested: {e}");
        }
    }

    pub async fn list_bookings(
        &self,
        carrier_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> AppResult<Vec<MarketplaceBooking>> {
        let bookings = self
            .repo
            .list_bookings_by_carrier(carrier_id, limit.clamp(1, 100), offset.max(0))
            .await
            .map_err(AppError::internal)?;
        // An online booking whose hold has not landed is not an offer yet.
        // Showing it would have a carrier hold a truck for a job the merchant
        // has not funded and may never fund.
        Ok(bookings.into_iter().filter(MarketplaceBooking::is_offered_to_carrier).collect())
    }

    pub async fn accept_booking(
        &self,
        booking_id: Uuid,
        carrier_id: Uuid,
    ) -> AppResult<MarketplaceBooking> {
        let mut booking = self.load_owned_booking(booking_id, carrier_id).await?;

        // A carrier must not be able to see, let alone accept, an online
        // booking whose hold never landed -- `list_bookings` filters those out,
        // and this is the same rule enforced where it decides something.
        if !booking.is_offered_to_carrier() {
            return Err(AppError::NotFound {
                resource: "MarketplaceBooking", id: booking_id.to_string(),
            });
        }

        booking.accept().map_err(|e| AppError::BusinessRule(e.to_string()))?;

        // Capture BEFORE persisting the acceptance, mirroring the
        // credit-before-advance rule OmniDeliv's courier consumer uses: if the
        // capture fails the acceptance is not recorded either, so the carrier
        // sees the booking still pending and can accept again, rather than
        // being committed to a job whose money was never taken.
        if booking.payment_status == BookingPaymentStatus::Authorized {
            let intent_id = booking.payment_intent_id.ok_or_else(|| {
                AppError::internal(anyhow::anyhow!("booking {booking_id} is authorized with no intent id"))
            })?;
            let payments = self.payments.as_ref().ok_or_else(|| {
                AppError::internal(anyhow::anyhow!("booking {booking_id} holds funds but payments is not configured"))
            })?;
            payments.capture(intent_id).await.map_err(AppError::internal)?;
            booking.payment_captured().map_err(|e| AppError::internal(anyhow::anyhow!("{e}")))?;
        }

        self.repo.save_booking(&booking).await.map_err(AppError::internal)?;

        let payload = logisticos_events::payloads::MarketplaceBookingAccepted {
            booking_id:  booking.id,
            listing_id:  booking.listing_id,
            carrier_id:  booking.carrier_id,
            tenant_id:   booking.tenant_id,
            accepted_at: Utc::now().to_rfc3339(),
        };
        let ev = Event::new("logisticos/carrier", "marketplace.booking.accepted", booking.tenant_id, payload);
        if let Err(e) = self.kafka.publish_event(topics::MARKETPLACE_BOOKING_ACCEPTED, &ev).await {
            tracing::warn!("Failed to publish marketplace.booking.accepted: {e}");
        }

        Ok(booking)
    }

    pub async fn reject_booking(
        &self,
        booking_id: Uuid,
        carrier_id: Uuid,
    ) -> AppResult<MarketplaceBooking> {
        let mut booking = self.load_owned_booking(booking_id, carrier_id).await?;

        if !booking.is_offered_to_carrier() {
            return Err(AppError::NotFound {
                resource: "MarketplaceBooking", id: booking_id.to_string(),
            });
        }

        booking.reject().map_err(|e| AppError::BusinessRule(e.to_string()))?;

        // Release the hold before recording the rejection, for the same reason
        // accept captures first: a failed void must leave the booking pending
        // and retryable rather than rejected with the merchant's funds still
        // ring-fenced and nothing left that would ever release them.
        if booking.payment_status == BookingPaymentStatus::Authorized {
            let intent_id = booking.payment_intent_id.ok_or_else(|| {
                AppError::internal(anyhow::anyhow!("booking {booking_id} is authorized with no intent id"))
            })?;
            let payments = self.payments.as_ref().ok_or_else(|| {
                AppError::internal(anyhow::anyhow!("booking {booking_id} holds funds but payments is not configured"))
            })?;
            payments.void(intent_id).await.map_err(AppError::internal)?;
            booking.payment_voided().map_err(|e| AppError::internal(anyhow::anyhow!("{e}")))?;
        }

        self.repo.save_booking(&booking).await.map_err(AppError::internal)?;

        let payload = logisticos_events::payloads::MarketplaceBookingRejected {
            booking_id:  booking.id,
            listing_id:  booking.listing_id,
            carrier_id:  booking.carrier_id,
            tenant_id:   booking.tenant_id,
            rejected_at: Utc::now().to_rfc3339(),
        };
        let ev = Event::new("logisticos/carrier", "marketplace.booking.rejected", booking.tenant_id, payload);
        if let Err(e) = self.kafka.publish_event(topics::MARKETPLACE_BOOKING_REJECTED, &ev).await {
            tracing::warn!("Failed to publish marketplace.booking.rejected: {e}");
        }

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
            .record_pickup(input.picked_up_by.clone(), input.pickup_notes.clone())
            .map_err(|e| AppError::BusinessRule(e.to_string()))?;
        self.repo.save_booking(&booking).await.map_err(AppError::internal)?;

        let payload = logisticos_events::payloads::MarketplacePickupRecorded {
            booking_id:   booking.id,
            listing_id:   booking.listing_id,
            carrier_id:   booking.carrier_id,
            tenant_id:    booking.tenant_id,
            picked_up_by: input.picked_up_by,
            pickup_notes: input.pickup_notes,
            picked_up_at: Utc::now().to_rfc3339(),
        };
        let ev = Event::new("logisticos/carrier", "marketplace.pickup.recorded", booking.tenant_id, payload);
        if let Err(e) = self.kafka.publish_event(topics::MARKETPLACE_PICKUP_RECORDED, &ev).await {
            tracing::warn!("Failed to publish marketplace.pickup.recorded: {e}");
        }

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

#[cfg(test)]
mod booking_tests {
    use super::*;
    use chrono::Duration;
    use crate::domain::entities::{quote_price_cents, SizeClass};
    use std::sync::Mutex;

    // ── Fixtures ──────────────────────────────────────────────────────────

    fn listing(tenant_id: Uuid) -> VehicleListing {
        VehicleListing::new(
            tenant_id,
            Uuid::new_v4(),
            "ABC-1234".into(),
            SizeClass::L300,
            1_500.0,
            Some(12.0),
            50_00,   // base AED 50.00
            2_00,    // AED 2.00/km
            Some(10), // AED 0.10/kg
            "Metro Manila".into(),
            Utc::now() - Duration::hours(1),
            Utc::now() + Duration::hours(6),
            15,
        )
    }

    fn cmd(listing_id: Uuid, method: BookingPaymentMethod) -> CreateBookingCommand {
        CreateBookingCommand {
            listing_id,
            shipment_id: Uuid::new_v4(),
            awb: "CM-PH1-0000001X".into(),
            consumer_name: "Maria Reyes".into(),
            consumer_phone: Some("+639171234567".into()),
            pickup_label: "Warehouse 4, Pasig".into(),
            dropoff_label: "Ermita".into(),
            cargo_weight_kg: 800.0,
            cargo_volume_m3: Some(6.0),
            distance_km: 22.0,
            pickup_at: Utc::now() + Duration::hours(2),
            payment_method: method,
        }
    }

    // ── Fake repository ───────────────────────────────────────────────────

    #[derive(Default)]
    struct FakeRepo {
        listings: Mutex<Vec<VehicleListing>>,
        bookings: Mutex<Vec<MarketplaceBooking>>,
    }

    impl FakeRepo {
        fn with_listing(l: VehicleListing) -> Arc<Self> {
            let r = Arc::new(Self::default());
            r.listings.lock().unwrap().push(l);
            r
        }
        fn booking(&self, id: Uuid) -> MarketplaceBooking {
            self.bookings.lock().unwrap().iter().find(|b| b.id == id).cloned()
                .expect("booking should be persisted")
        }
    }

    #[async_trait::async_trait]
    impl MarketplaceRepository for FakeRepo {
        async fn create_listing(&self, l: &VehicleListing) -> anyhow::Result<()> {
            self.listings.lock().unwrap().push(l.clone());
            Ok(())
        }
        async fn find_listing_by_id(&self, id: Uuid) -> anyhow::Result<Option<VehicleListing>> {
            Ok(self.listings.lock().unwrap().iter().find(|l| l.id == id).cloned())
        }
        async fn list_listings_by_carrier(&self, _c: Uuid, _l: i64, _o: i64) -> anyhow::Result<Vec<VehicleListing>> {
            Ok(self.listings.lock().unwrap().clone())
        }
        async fn update_listing(&self, _l: &VehicleListing) -> anyhow::Result<()> { Ok(()) }
        async fn delete_listing(&self, _id: Uuid) -> anyhow::Result<bool> { Ok(true) }
        async fn find_available_listings(
            &self, _t: Uuid, _w: f32, _s: Option<&str>, _at: DateTime<Utc>, _l: i64,
        ) -> anyhow::Result<Vec<VehicleListing>> {
            Ok(self.listings.lock().unwrap().clone())
        }
        async fn create_booking(&self, b: &MarketplaceBooking) -> anyhow::Result<()> {
            self.bookings.lock().unwrap().push(b.clone());
            Ok(())
        }
        async fn save_booking_payment_reference(&self, b: &MarketplaceBooking) -> anyhow::Result<()> {
            let mut all = self.bookings.lock().unwrap();
            if let Some(e) = all.iter_mut().find(|x| x.id == b.id) {
                e.payment_intent_id = b.payment_intent_id;
                e.payment_checkout_url = b.payment_checkout_url.clone();
            }
            Ok(())
        }
        async fn find_booking_by_id(&self, id: Uuid) -> anyhow::Result<Option<MarketplaceBooking>> {
            Ok(self.bookings.lock().unwrap().iter().find(|b| b.id == id).cloned())
        }
        async fn list_bookings_by_carrier(&self, c: Uuid, _l: i64, _o: i64) -> anyhow::Result<Vec<MarketplaceBooking>> {
            Ok(self.bookings.lock().unwrap().iter().filter(|b| b.carrier_id == c).cloned().collect())
        }
        async fn list_bookings_by_booker(&self, _t: Uuid, u: Uuid, _l: i64, _o: i64) -> anyhow::Result<Vec<MarketplaceBooking>> {
            Ok(self.bookings.lock().unwrap().iter()
                .filter(|b| b.booked_by_user_id == Some(u)).cloned().collect())
        }
        async fn list_pending_bookings(&self, _l: i64) -> anyhow::Result<Vec<MarketplaceBooking>> {
            Ok(self.bookings.lock().unwrap().iter()
                .filter(|b| b.status == BookingStatus::Pending).cloned().collect())
        }
        async fn save_booking(&self, b: &MarketplaceBooking) -> anyhow::Result<()> {
            let mut all = self.bookings.lock().unwrap();
            if let Some(e) = all.iter_mut().find(|x| x.id == b.id) { *e = b.clone(); }
            Ok(())
        }
    }

    // ── Fake gateway ──────────────────────────────────────────────────────

    #[derive(Default)]
    struct FakePayments {
        captures:      Mutex<Vec<Uuid>>,
        voids:         Mutex<Vec<Uuid>>,
        capture_fails: bool,
        void_fails:    bool,
    }

    #[async_trait::async_trait]
    impl BookingPayments for FakePayments {
        async fn authorize(
            &self, _t: Uuid, booking_id: Uuid, _a: i64, _c: &str, _r: &str,
        ) -> anyhow::Result<AuthorizedIntent> {
            Ok(AuthorizedIntent {
                intent_id: Uuid::new_v4(),
                checkout_url: format!("https://ni.example/pay/{booking_id}"),
            })
        }
        async fn capture(&self, intent_id: Uuid) -> anyhow::Result<()> {
            self.captures.lock().unwrap().push(intent_id);
            if self.capture_fails { anyhow::bail!("gateway refused the capture") }
            Ok(())
        }
        async fn void(&self, intent_id: Uuid) -> anyhow::Result<()> {
            self.voids.lock().unwrap().push(intent_id);
            if self.void_fails { anyhow::bail!("gateway refused the void") }
            Ok(())
        }
    }

    /// An in-process broker, so publishing costs nothing and does not need a
    /// cluster. `KafkaProducer` is a concrete type on `MarketplaceService`, so
    /// it cannot be substituted with a trait object.
    fn noop_kafka() -> Arc<KafkaProducer> {
        let cluster = rdkafka::mocking::MockCluster::new(1).expect("mock kafka cluster");
        let brokers = cluster.bootstrap_servers();
        Box::leak(Box::new(cluster));
        Arc::new(KafkaProducer::new(&brokers).expect("noop kafka producer"))
    }

    fn service(repo: Arc<FakeRepo>, payments: Option<Arc<FakePayments>>) -> MarketplaceService {
        let svc = MarketplaceService::new(repo, noop_kafka());
        match payments {
            Some(p) => svc.with_payments(p, "AED".into(), "https://example.invalid/return".into()),
            None => svc,
        }
    }

    // ── The quote ─────────────────────────────────────────────────────────

    /// The price comes from the listing's own rate card. `CreateBookingCommand`
    /// deliberately has no price field -- a client-supplied price is a
    /// client-supplied discount.
    #[test]
    fn the_quote_is_the_rate_card_applied_to_the_job() {
        let l = listing(Uuid::new_v4());
        // 50.00 base + 22km x 2.00 + 800kg x 0.10 = 50.00 + 44.00 + 80.00
        assert_eq!(quote_price_cents(&l, 22.0, 800.0), 50_00 + 44_00 + 80_00);
    }

    /// A listing with no per-kg rate prices on distance alone rather than
    /// treating the missing rate as an error or as zero-weight.
    #[test]
    fn a_listing_without_a_per_kg_rate_prices_on_distance_only() {
        let mut l = listing(Uuid::new_v4());
        l.per_kg_cents = None;
        assert_eq!(quote_price_cents(&l, 10.0, 900.0), 50_00 + 20_00);
    }

    /// Negative quantities floor at zero rather than subtracting from the base
    /// price -- otherwise `distance_km: -1000` is a discount.
    #[test]
    fn negative_quantities_cannot_reduce_the_price() {
        let l = listing(Uuid::new_v4());
        assert_eq!(quote_price_cents(&l, -1000.0, -1000.0), l.base_price_cents);
    }

    // ── Placing a booking ─────────────────────────────────────────────────

    #[tokio::test]
    async fn an_invoice_booking_is_visible_to_the_carrier_immediately() {
        let tenant = Uuid::new_v4();
        let l = listing(tenant);
        let repo = FakeRepo::with_listing(l.clone());
        let svc = service(repo.clone(), None);

        let placed = svc
            .create_booking(tenant, Uuid::new_v4(), cmd(l.id, BookingPaymentMethod::Invoice))
            .await
            .expect("invoice booking should succeed with no gateway configured");

        assert!(placed.checkout_url.is_none(), "invoice needs no card page");
        assert!(placed.booking.is_offered_to_carrier());
        assert_eq!(placed.booking.quoted_price_cents, 50_00 + 44_00 + 80_00);
    }

    /// The rule the deferred design exists for: a carrier must not be shown a
    /// booking the merchant has not funded, or they would hold a truck for a
    /// job that may never be paid for.
    #[tokio::test]
    async fn an_online_booking_is_hidden_from_the_carrier_until_the_hold_lands() {
        let tenant = Uuid::new_v4();
        let l = listing(tenant);
        let repo = FakeRepo::with_listing(l.clone());
        let svc = service(repo.clone(), Some(Arc::new(FakePayments::default())));

        let placed = svc
            .create_booking(tenant, Uuid::new_v4(), cmd(l.id, BookingPaymentMethod::Online))
            .await
            .unwrap();

        assert!(placed.checkout_url.is_some(), "online must return a card page");
        assert!(!placed.booking.is_offered_to_carrier());

        let visible = svc.list_bookings(l.carrier_id, 50, 0).await.unwrap();
        assert!(visible.is_empty(), "an unfunded booking must not appear in the carrier's inbox");

        // ...and once the hold lands, it does.
        let mut b = repo.booking(placed.booking.id);
        b.payment_authorized(Uuid::new_v4()).unwrap();
        repo.save_booking(&b).await.unwrap();
        assert_eq!(svc.list_bookings(l.carrier_id, 50, 0).await.unwrap().len(), 1);
    }

    /// The row must exist before the gateway call: `authorize` stamps the
    /// booking id as the intent's reference, and the webhook can arrive before
    /// `create_booking` has even returned.
    #[tokio::test]
    async fn an_online_booking_is_persisted_before_the_gateway_is_called() {
        let tenant = Uuid::new_v4();
        let l = listing(tenant);
        let repo = FakeRepo::with_listing(l.clone());
        let svc = service(repo.clone(), Some(Arc::new(FakePayments::default())));

        let placed = svc
            .create_booking(tenant, Uuid::new_v4(), cmd(l.id, BookingPaymentMethod::Online))
            .await
            .unwrap();

        let stored = repo.booking(placed.booking.id);
        assert!(stored.payment_intent_id.is_some(), "the intent id must be written back");
        assert_eq!(stored.resumable_checkout_url(), placed.checkout_url.as_deref());
    }

    #[tokio::test]
    async fn online_is_refused_when_no_gateway_is_configured() {
        let tenant = Uuid::new_v4();
        let l = listing(tenant);
        let svc = service(FakeRepo::with_listing(l.clone()), None);
        let err = svc
            .create_booking(tenant, Uuid::new_v4(), cmd(l.id, BookingPaymentMethod::Online))
            .await
            .expect_err("online must be refused rather than silently downgraded to invoice");
        assert!(matches!(err, BookingError::OnlineUnavailable), "got {err:?}");
    }

    #[tokio::test]
    async fn a_load_the_vehicle_cannot_carry_is_refused() {
        let tenant = Uuid::new_v4();
        let l = listing(tenant);
        let svc = service(FakeRepo::with_listing(l.clone()), None);
        let mut c = cmd(l.id, BookingPaymentMethod::Invoice);
        c.cargo_weight_kg = 9_000.0;
        assert!(matches!(
            svc.create_booking(tenant, Uuid::new_v4(), c).await,
            Err(BookingError::TooHeavy { .. })
        ));
    }

    /// Cross-tenant listings read as absent, not forbidden, so ids cannot be
    /// enumerated across tenants.
    #[tokio::test]
    async fn another_tenants_listing_is_not_bookable_and_reads_as_missing() {
        let l = listing(Uuid::new_v4());
        let svc = service(FakeRepo::with_listing(l.clone()), None);
        let err = svc
            .create_booking(Uuid::new_v4(), Uuid::new_v4(), cmd(l.id, BookingPaymentMethod::Invoice))
            .await
            .unwrap_err();
        assert!(matches!(err, BookingError::Unavailable(_)), "got {err:?}");
    }

    // ── Accept captures, reject voids ─────────────────────────────────────

    async fn funded_booking(
        payments: Arc<FakePayments>,
    ) -> (Arc<FakeRepo>, MarketplaceService, MarketplaceBooking) {
        let tenant = Uuid::new_v4();
        let l = listing(tenant);
        let repo = FakeRepo::with_listing(l.clone());
        let svc = service(repo.clone(), Some(payments));
        let placed = svc
            .create_booking(tenant, Uuid::new_v4(), cmd(l.id, BookingPaymentMethod::Online))
            .await
            .unwrap();
        let mut b = repo.booking(placed.booking.id);
        b.payment_authorized(b.payment_intent_id.unwrap()).unwrap();
        repo.save_booking(&b).await.unwrap();
        (repo, svc, b)
    }

    #[tokio::test]
    async fn accepting_captures_the_hold() {
        let pay = Arc::new(FakePayments::default());
        let (repo, svc, b) = funded_booking(pay.clone()).await;

        svc.accept_booking(b.id, b.carrier_id).await.unwrap();

        assert_eq!(pay.captures.lock().unwrap().as_slice(), &[b.payment_intent_id.unwrap()]);
        assert!(pay.voids.lock().unwrap().is_empty());
        let saved = repo.booking(b.id);
        assert_eq!(saved.status, BookingStatus::Accepted);
        assert_eq!(saved.payment_status, BookingPaymentStatus::Captured);
    }

    /// Capture before persisting the acceptance. A failed capture must leave
    /// the booking pending and retryable, not commit a carrier to a job whose
    /// money was never taken.
    #[tokio::test]
    async fn a_failed_capture_leaves_the_booking_unaccepted() {
        let pay = Arc::new(FakePayments { capture_fails: true, ..Default::default() });
        let (repo, svc, b) = funded_booking(pay.clone()).await;

        svc.accept_booking(b.id, b.carrier_id).await.expect_err("capture failed");

        let saved = repo.booking(b.id);
        assert_eq!(saved.status, BookingStatus::Pending, "must stay acceptable");
        assert_eq!(saved.payment_status, BookingPaymentStatus::Authorized, "hold is still open");
    }

    #[tokio::test]
    async fn rejecting_releases_the_hold() {
        let pay = Arc::new(FakePayments::default());
        let (repo, svc, b) = funded_booking(pay.clone()).await;

        svc.reject_booking(b.id, b.carrier_id).await.unwrap();

        assert_eq!(pay.voids.lock().unwrap().as_slice(), &[b.payment_intent_id.unwrap()]);
        assert!(pay.captures.lock().unwrap().is_empty(), "a rejected booking must never be charged");
        let saved = repo.booking(b.id);
        assert_eq!(saved.status, BookingStatus::Rejected);
        assert_eq!(saved.payment_status, BookingPaymentStatus::Voided);
    }

    /// The money-safety-critical direction: a failed void must not record the
    /// rejection, because a rejected booking is one nothing would ever come
    /// back to release.
    #[tokio::test]
    async fn a_failed_void_leaves_the_booking_pending_so_it_is_retried() {
        let pay = Arc::new(FakePayments { void_fails: true, ..Default::default() });
        let (repo, svc, b) = funded_booking(pay.clone()).await;

        svc.reject_booking(b.id, b.carrier_id).await.expect_err("void failed");

        let saved = repo.booking(b.id);
        assert_eq!(saved.status, BookingStatus::Pending);
        assert_eq!(saved.payment_status, BookingPaymentStatus::Authorized);
    }

    /// The same gate `list_bookings` applies, enforced where it decides
    /// something: a carrier who learns an id must not be able to accept a
    /// booking the merchant never funded.
    #[tokio::test]
    async fn a_carrier_cannot_accept_an_unfunded_online_booking() {
        let tenant = Uuid::new_v4();
        let l = listing(tenant);
        let repo = FakeRepo::with_listing(l.clone());
        let svc = service(repo.clone(), Some(Arc::new(FakePayments::default())));
        let placed = svc
            .create_booking(tenant, Uuid::new_v4(), cmd(l.id, BookingPaymentMethod::Online))
            .await
            .unwrap();

        let err = svc.accept_booking(placed.booking.id, l.carrier_id).await.unwrap_err();
        assert!(matches!(err, AppError::NotFound { .. }), "got {err:?}");
        assert_eq!(repo.booking(placed.booking.id).status, BookingStatus::Pending);
    }

    // ── The response window ───────────────────────────────────────────────

    #[tokio::test]
    async fn a_booking_nobody_answered_in_time_is_voided_and_cancelled() {
        let pay = Arc::new(FakePayments::default());
        let (repo, svc, b) = funded_booking(pay.clone()).await;

        // Wind the authorization back past the listing's 15-minute window.
        let mut aged = repo.booking(b.id);
        aged.payment_authorized_at = Some(Utc::now() - Duration::minutes(16));
        repo.save_booking(&aged).await.unwrap();

        assert_eq!(svc.sweep_expired_bookings().await.unwrap(), 1);

        assert_eq!(pay.voids.lock().unwrap().as_slice(), &[b.payment_intent_id.unwrap()]);
        let saved = repo.booking(b.id);
        assert_eq!(saved.status, BookingStatus::Cancelled);
        assert_eq!(saved.payment_status, BookingPaymentStatus::Voided);
    }

    /// Still inside the window: nothing is touched. A sweep that voids early is
    /// a sweep that cancels bookings a carrier was about to accept.
    #[tokio::test]
    async fn a_booking_still_inside_its_window_is_left_alone() {
        let pay = Arc::new(FakePayments::default());
        let (repo, svc, b) = funded_booking(pay.clone()).await;

        assert_eq!(svc.sweep_expired_bookings().await.unwrap(), 0);
        assert!(pay.voids.lock().unwrap().is_empty());
        assert_eq!(repo.booking(b.id).status, BookingStatus::Pending);
    }

    /// The window runs from the authorization, not from when the merchant hit
    /// Book: an online booking is not shown to the carrier at all until the
    /// hold lands, so counting from `created_at` would spend the carrier's
    /// whole window while they could not see it.
    #[test]
    fn the_window_counts_from_the_authorization_not_from_placement() {
        let l = listing(Uuid::new_v4());
        let mut b = MarketplaceBooking::place(
            &l, Uuid::new_v4(), Uuid::new_v4(), "AWB".into(), "N".into(), None,
            "p".into(), "d".into(), 100.0, None, 5.0, Utc::now(),
        ).with_online_payment();

        b.created_at = Utc::now() - Duration::minutes(60);
        assert!(b.response_window_expired(Utc::now()), "no authorization — falls back to created_at");

        b.payment_authorized_at = Some(Utc::now() - Duration::minutes(1));
        assert!(!b.response_window_expired(Utc::now()), "the clock restarts at the authorization");
    }

    /// A booking that already has an answer never expires, whatever the clock
    /// says -- otherwise the sweep would cancel accepted jobs.
    #[test]
    fn an_answered_booking_never_expires() {
        let l = listing(Uuid::new_v4());
        let mut b = MarketplaceBooking::place(
            &l, Uuid::new_v4(), Uuid::new_v4(), "AWB".into(), "N".into(), None,
            "p".into(), "d".into(), 100.0, None, 5.0, Utc::now(),
        );
        b.created_at = Utc::now() - Duration::days(3);
        b.accept().unwrap();
        assert!(!b.response_window_expired(Utc::now()));
    }

    // ── Resuming an unfinished payment ────────────────────────────────────

    #[test]
    fn a_spent_checkout_page_is_never_offered_again() {
        let l = listing(Uuid::new_v4());
        let base = MarketplaceBooking::place(
            &l, Uuid::new_v4(), Uuid::new_v4(), "AWB".into(), "N".into(), None,
            "p".into(), "d".into(), 100.0, None, 5.0, Utc::now(),
        ).with_online_payment();

        for status in [
            BookingPaymentStatus::Authorized,
            BookingPaymentStatus::Captured,
            BookingPaymentStatus::Voided,
            BookingPaymentStatus::Failed,
        ] {
            let mut b = base.clone();
            b.payment_checkout_url = Some("https://ni.example/pay/x".into());
            b.payment_status = status;
            assert_eq!(
                b.resumable_checkout_url(), None,
                "{status:?} must not hand back a second chance to authorize",
            );
        }
    }

    #[test]
    fn an_invoice_booking_never_offers_a_checkout_page() {
        let l = listing(Uuid::new_v4());
        let mut b = MarketplaceBooking::place(
            &l, Uuid::new_v4(), Uuid::new_v4(), "AWB".into(), "N".into(), None,
            "p".into(), "d".into(), 100.0, None, 5.0, Utc::now(),
        );
        b.payment_checkout_url = Some("https://ni.example/pay/x".into());
        assert_eq!(b.resumable_checkout_url(), None);
    }
}
