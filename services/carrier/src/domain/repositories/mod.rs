use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;
use logisticos_types::TenantId;
use crate::domain::entities::{Carrier, CarrierId, MarketplaceBooking, SlaRecord, VehicleListing, ZoneSlaRow};

#[async_trait]
pub trait CarrierRepository: Send + Sync {
    async fn find_by_id(&self, id: &CarrierId) -> anyhow::Result<Option<Carrier>>;
    async fn find_by_code(&self, tenant_id: &TenantId, code: &str) -> anyhow::Result<Option<Carrier>>;
    async fn find_by_contact_email(&self, tenant_id: &TenantId, email: &str) -> anyhow::Result<Option<Carrier>>;
    async fn list(&self, tenant_id: &TenantId, limit: i64, offset: i64) -> anyhow::Result<Vec<Carrier>>;
    async fn list_active(&self, tenant_id: &TenantId) -> anyhow::Result<Vec<Carrier>>;
    async fn save(&self, carrier: &Carrier) -> anyhow::Result<()>;
}

/// Repository for per-shipment SLA commitment records.
/// Created by dispatch when a carrier is allocated; updated on delivery outcome.
#[async_trait]
pub trait SlaRecordRepository: Send + Sync {
    /// Persist a new SLA record (status = in_transit).
    async fn create(&self, record: &SlaRecord) -> anyhow::Result<()>;

    /// Look up SLA record by shipment_id to find the carrier_id on delivery events.
    async fn find_by_shipment(&self, shipment_id: Uuid) -> anyhow::Result<Option<SlaRecord>>;

    /// Persist updated outcome fields (delivered_at, on_time, status, failure_reason).
    async fn save_outcome(&self, record: &SlaRecord) -> anyhow::Result<()>;

    /// Paginated history for a single carrier — used by partner portal detail view.
    async fn list_by_carrier(&self, carrier_id: Uuid, limit: i64, offset: i64) -> anyhow::Result<Vec<SlaRecord>>;

    /// Zone-level SLA aggregate for a carrier over a time window.
    /// Used by `GET /v1/carriers/:id/sla-summary`.
    async fn zone_summary(
        &self,
        carrier_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> anyhow::Result<Vec<ZoneSlaRow>>;

    /// Aggregate failed SLA records by failure_reason for a carrier over a time
    /// window. Used by `GET /v1/carriers/breach-reasons` (partner-portal SLA page).
    async fn breach_reasons(
        &self,
        carrier_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> anyhow::Result<Vec<BreachReasonRow>>;
}

#[derive(Debug, serde::Serialize)]
pub struct BreachReasonRow {
    pub reason: String,
    pub count:  i64,
}

/// Repository for the carrier marketplace — vehicle listings and spot bookings.
#[async_trait]
pub trait MarketplaceRepository: Send + Sync {
    // ── Listings ──────────────────────────────────────────────────────────────

    async fn create_listing(&self, listing: &VehicleListing) -> anyhow::Result<()>;
    async fn find_listing_by_id(&self, id: Uuid) -> anyhow::Result<Option<VehicleListing>>;
    async fn list_listings_by_carrier(
        &self,
        carrier_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<VehicleListing>>;
    async fn update_listing(&self, listing: &VehicleListing) -> anyhow::Result<()>;
    async fn delete_listing(&self, id: Uuid) -> anyhow::Result<bool>;

    // ── Bookings ──────────────────────────────────────────────────────────────

    /// Listings a merchant may book right now: active, inside their idle
    /// window, and big enough for the load. Tenant-scoped -- the buy side is
    /// the one marketplace read that deliberately crosses carriers, because
    /// choosing between them is the product.
    async fn find_available_listings(
        &self,
        tenant_id:     Uuid,
        min_weight_kg: f32,
        size_class:    Option<&str>,
        at:            chrono::DateTime<chrono::Utc>,
        limit:         i64,
    ) -> anyhow::Result<Vec<VehicleListing>>;

    async fn create_booking(&self, booking: &MarketplaceBooking) -> anyhow::Result<()>;

    /// Writes back the intent id and checkout URL only.
    ///
    /// Its own method rather than a field on `save_booking`, because those two
    /// columns are set exactly once — between the row being inserted and the
    /// gateway call returning — and `save_booking` is the hot path every
    /// status transition runs through. Folding them in would mean every later
    /// save re-asserting a URL it has no business touching.
    async fn save_booking_payment_reference(&self, booking: &MarketplaceBooking) -> anyhow::Result<()>;

    /// The bookings a merchant placed, newest first. Scoped to the user, not
    /// the tenant: another merchant's freight rates and destinations are not
    /// theirs to read.
    async fn list_bookings_by_booker(
        &self,
        tenant_id: Uuid,
        user_id:   Uuid,
        limit:     i64,
        offset:    i64,
    ) -> anyhow::Result<Vec<MarketplaceBooking>>;

    /// Pending bookings whose carrier-response window may have run out. The
    /// window itself is per-booking, so this returns candidates and
    /// `MarketplaceBooking::response_window_expired` makes the decision.
    async fn list_pending_bookings(&self, limit: i64) -> anyhow::Result<Vec<MarketplaceBooking>>;
    async fn find_booking_by_id(&self, id: Uuid) -> anyhow::Result<Option<MarketplaceBooking>>;
    async fn list_bookings_by_carrier(
        &self,
        carrier_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<MarketplaceBooking>>;
    async fn save_booking(&self, booking: &MarketplaceBooking) -> anyhow::Result<()>;
}
