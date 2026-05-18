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
    async fn list_all_active_listings(&self) -> anyhow::Result<Vec<VehicleListing>>;
    async fn update_listing(&self, listing: &VehicleListing) -> anyhow::Result<()>;
    async fn delete_listing(&self, id: Uuid) -> anyhow::Result<bool>;

    // ── Bookings ──────────────────────────────────────────────────────────────

    async fn create_booking(&self, booking: &MarketplaceBooking) -> anyhow::Result<()>;
    async fn find_booking_by_id(&self, id: Uuid) -> anyhow::Result<Option<MarketplaceBooking>>;
    async fn list_bookings_by_carrier(
        &self,
        carrier_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> anyhow::Result<Vec<MarketplaceBooking>>;
    async fn save_booking(&self, booking: &MarketplaceBooking) -> anyhow::Result<()>;
}
