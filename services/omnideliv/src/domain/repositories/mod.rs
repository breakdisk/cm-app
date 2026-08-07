//! Repository contracts.
//!
//! TENANCY: every method takes `tenant_id` first. There is no database-level
//! policy in this schema (see migration 0001), so the signature is the
//! enforcement point.

use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::entities::{
    Availability, Basket, BasketConflict, CatalogItem, Order, TelemetryEvent, Vendor, VendorLedger,
    Vertical,
};

#[async_trait]
pub trait VendorRepository: Send + Sync {
    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<Option<Vendor>>;
    async fn save(&self, vendor: &Vendor) -> anyhow::Result<()>;

    /// The vendor a portal user operates. `None` when the user runs no store —
    /// which is the answer for every customer, so it is not an error.
    async fn find_by_user(&self, tenant_id: Uuid, user_id: Uuid) -> anyhow::Result<Option<Vendor>>;

    /// Orderable vendors of a vertical within `radius_km`, nearest first.
    async fn find_near(
        &self,
        tenant_id: Uuid,
        vertical: Vertical,
        lat: f64,
        lng: f64,
        radius_km: f64,
        limit: i64,
    ) -> anyhow::Result<Vec<Vendor>>;
}

/// An item paired with its current availability declaration. Returned together
/// because an agent needs both to decide anything — an item without its
/// freshness stamp cannot be reasoned about honestly.
#[derive(Debug, Clone)]
pub struct ItemWithAvailability {
    pub item:         CatalogItem,
    pub availability: Availability,
}

#[async_trait]
pub trait CatalogRepository: Send + Sync {
    async fn save_item(&self, item: &CatalogItem) -> anyhow::Result<()>;

    /// One item by id. Needed so a manual add can read the price server-side
    /// rather than trusting the client's.
    async fn find_item(&self, tenant_id: Uuid, item_id: Uuid) -> anyhow::Result<Option<CatalogItem>>;
    async fn set_availability(&self, a: &Availability) -> anyhow::Result<()>;

    /// Listed items for a vendor, each with its availability.
    async fn list_for_vendor(
        &self,
        tenant_id: Uuid,
        vendor_id: Uuid,
    ) -> anyhow::Result<Vec<ItemWithAvailability>>;

    /// Text search within a vendor, excluding items that clash with `avoid_allergens`.
    async fn search(
        &self,
        tenant_id: Uuid,
        vendor_id: Uuid,
        query: &str,
        avoid_allergens: &[String],
        limit: i64,
    ) -> anyhow::Result<Vec<ItemWithAvailability>>;

    /// The facts reconcile verifies proposed lines against, in one round trip.
    ///
    /// Batched rather than a `find_item` loop because it runs on every mesh run
    /// with one id per proposed line, and it sits between fan-out and the
    /// customer seeing a basket.
    ///
    /// Items that do not exist are simply absent from the result — the caller
    /// treats an unresolved id as unverifiable and drops the line, so a missing
    /// row must not be an error.
    async fn item_facts(
        &self,
        tenant_id: Uuid,
        item_ids: &[Uuid],
    ) -> anyhow::Result<Vec<ItemFacts>>;
}

/// Catalog truth about one item: the item's own fields plus the two that live
/// on its vendor. Mirrors `omnideliv_mesh::ItemFacts`, which the mesh crate
/// owns — this is the domain-side shape, converted at the adapter.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemFacts {
    pub item_id:           Uuid,
    pub allergens:         Vec<String>,
    pub vertical:          String,
    pub prep_time_minutes: i32,
    pub price_cents:       i64,
}

#[async_trait]
pub trait BasketRepository: Send + Sync {
    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<Option<Basket>>;

    /// Record what the mesh's verification found, replacing any prior list.
    ///
    /// A targeted UPDATE rather than a field on `save`, and deliberately does
    /// **not** bump `version`. The optimistic lock guards against two callers
    /// losing each other's *customer* edits; this is the run recording its own
    /// findings about lines it just wrote, and making it invalidate a
    /// concurrent edit would turn an observation into a lost update.
    async fn set_conflicts(
        &self,
        tenant_id: Uuid,
        basket_id: Uuid,
        conflicts: &[BasketConflict],
    ) -> anyhow::Result<()>;

    /// Persists the basket and its sub-intents and lines as one unit.
    async fn save(&self, basket: &Basket) -> anyhow::Result<()>;
}

#[async_trait]
pub trait OrderRepository: Send + Sync {
    /// Persists the order and its vendor legs as one unit — an order without
    /// its legs cannot be settled, and legs without an order are orphaned money.
    async fn save(&self, order: &Order) -> anyhow::Result<()>;
    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<Option<Order>>;

    /// Orders that have taken payment but not yet found a courier.
    ///
    /// Deliberately across all tenants: the recovery sweep is an operator
    /// concern, not a customer request, and scoping it per tenant would mean
    /// the sweep only runs for tenants someone remembered to enumerate.
    async fn find_awaiting_courier(&self) -> anyhow::Result<Vec<Order>>;
}

#[async_trait]
pub trait VendorLedgerRepository: Send + Sync {
    /// The open ledger for this vendor and period, if one exists.
    async fn find_open(&self, tenant_id: Uuid, vendor_id: Uuid, period: &str)
        -> anyhow::Result<Option<VendorLedger>>;
    /// Persists the ledger and any entries not yet written. Entries are only
    /// ever inserted — an update would break the append-only guarantee the
    /// whole shape exists for.
    async fn save(&self, ledger: &VendorLedger) -> anyhow::Result<()>;
}

#[async_trait]
pub trait TelemetryRepository: Send + Sync {
    /// Append one event. There is deliberately no update or delete.
    async fn append(&self, event: &TelemetryEvent) -> anyhow::Result<()>;
    async fn timeline(&self, tenant_id: Uuid, order_id: Uuid) -> anyhow::Result<Vec<TelemetryEvent>>;
}
