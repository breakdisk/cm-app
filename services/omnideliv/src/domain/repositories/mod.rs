//! Repository contracts.
//!
//! TENANCY: every method takes `tenant_id` first. There is no database-level
//! policy in this schema (see migration 0001), so the signature is the
//! enforcement point.

use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::entities::{
    Availability, Basket, BasketConflict, CatalogItem, CatalogSource, LedgerStatus, Order,
    TelemetryEvent, Vendor, VendorLedger, Vertical,
};

/// One period's headline figures, without its entries.
///
/// A payout has three distinct meanings and the console had been showing one
/// number for all of them: still accruing (`Open`), owed but not yet paid
/// (`Closed`), and paid (`Settled`). Collapsing them tells a vendor money is on
/// its way when the figure is still moving, or that it has arrived when it has
/// not.
#[derive(Debug, Clone)]
pub struct LedgerPeriod {
    pub period:        String,
    pub status:        LedgerStatus,
    pub balance_cents: i64,
    pub updated_at:    chrono::DateTime<chrono::Utc>,
}

#[async_trait]
pub trait VendorRepository: Send + Sync {
    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<Option<Vendor>>;
    async fn save(&self, vendor: &Vendor) -> anyhow::Result<()>;

    /// The vendor a portal user operates. `None` when the user runs no store —
    /// which is the answer for every customer, so it is not an error.
    async fn find_by_user(&self, tenant_id: Uuid, user_id: Uuid) -> anyhow::Result<Option<Vendor>>;

    /// Every vendor in the tenant, newest first. The operator review queue
    /// reads this; `find_near` cannot serve it because it only ever returns
    /// stores that are already active and therefore already past review.
    async fn list_for_tenant(&self, tenant_id: Uuid) -> anyhow::Result<Vec<Vendor>>;

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

    /// Several vendors in one round trip.
    ///
    /// One query, not a lookup per leg. An order with four stops would
    /// otherwise issue four queries to draw four dots — the same N+1 that
    /// `CatalogRepository::find_items` was introduced to avoid on the basket.
    async fn find_by_ids(&self, tenant_id: Uuid, ids: &[Uuid]) -> anyhow::Result<Vec<Vendor>>;
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
    /// Several items at once, for callers holding a list of ids.
    ///
    /// One query rather than one per line: a basket view needs a name for every
    /// line, and looping `find_item` would make rendering a basket cost a round
    /// trip per item. Missing ids are simply absent from the result — an item
    /// deleted since it was added to a basket is a real case, not an error.
    async fn find_items(&self, tenant_id: Uuid, ids: &[Uuid]) -> anyhow::Result<Vec<CatalogItem>>;

    /// Match by the vendor's own product code. The fallback ingest key, and the
    /// duplicate check for manual entry — two items with one SKU in a store make
    /// every later reconciliation ambiguous.
    async fn find_item_by_sku(
        &self,
        tenant_id: Uuid,
        vendor_id: Uuid,
        sku: &str,
    ) -> anyhow::Result<Option<CatalogItem>>;

    /// Match by the id in the source system. The preferred ingest key: a vendor
    /// who renames a SKU in Shopify must not get a duplicate row here.
    async fn find_item_by_external(
        &self,
        tenant_id: Uuid,
        vendor_id: Uuid,
        source: CatalogSource,
        external_id: &str,
    ) -> anyhow::Result<Option<CatalogItem>>;

    async fn set_availability(&self, a: &Availability) -> anyhow::Result<()>;

    /// Point an item at a stored photo, or clear it with `None`.
    ///
    /// Its own method rather than a field on `save_item`, because the catalog
    /// upsert is also the ingest path: folding the photo into it would let a
    /// Shopify or CSV re-sync wipe a picture the vendor uploaded by hand.
    async fn set_image_key(
        &self,
        tenant_id: Uuid,
        item_id:   Uuid,
        key:       Option<&str>,
    ) -> anyhow::Result<()>;

    /// Stamp a human confirmation across every listed item in a store, in one
    /// statement. The bulk answer to the console's opening state — a vendor who
    /// has just synced 200 items should confirm them with one deliberate act,
    /// not 200 taps or (worse) a checkbox nobody reads.
    ///
    /// Returns how many rows it confirmed.
    async fn confirm_all_for_vendor(
        &self,
        tenant_id: Uuid,
        vendor_id: Uuid,
        user_id: Uuid,
    ) -> anyhow::Result<u64>;

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

    /// Record a vendor's assertion of an item's contents, stamping the time.
    ///
    /// An empty list is a real answer here — "I confirm it contains none of
    /// these" — which is precisely what an undeclared item cannot say.
    async fn declare_allergens(
        &self,
        tenant_id: Uuid,
        item_id: Uuid,
        allergens: &[String],
    ) -> anyhow::Result<bool>;
}

/// Catalog truth about one item: the item's own fields plus the two that live
/// One row of a customer's order list. Query-shaped, like `ItemFacts`.
#[derive(Debug, Clone)]
pub struct OrderSummary {
    pub id:                Uuid,
    pub status:            String,
    pub grand_total_cents: i64,
    pub stops_total:       i64,
    /// Comma-joined vendor names, for "Kuya's Silog House, Puregold Ermita".
    /// Empty when an order somehow has no legs — rendered as nothing rather
    /// than as a placeholder that reads like a real shop.
    pub vendor_names:      String,
    pub placed_at:         chrono::DateTime<chrono::Utc>,
    pub delivered_at:      Option<chrono::DateTime<chrono::Utc>>,
}

/// on its vendor. Mirrors `omnideliv_mesh::ItemFacts`, which the mesh crate
/// owns — this is the domain-side shape, converted at the adapter.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemFacts {
    pub item_id:           Uuid,
    pub allergens:         Vec<String>,
    /// NULL `allergens_declared_at` in the database. See migration 0014.
    pub allergens_declared: bool,
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

    /// A customer's own orders, newest first.
    ///
    /// Returns summaries rather than `Order`, because an `Order` carries its
    /// vendor legs and hydrating those per row is an N+1 for a screen that only
    /// needs a count. The detail view already loads the full order.
    async fn list_summaries_for_customer(
        &self,
        tenant_id:   Uuid,
        customer_id: Uuid,
        limit:       i64,
    ) -> anyhow::Result<Vec<OrderSummary>>;
}

#[async_trait]
pub trait VendorLedgerRepository: Send + Sync {
    /// The open ledger for this vendor and period, if one exists.
    async fn find_open(&self, tenant_id: Uuid, vendor_id: Uuid, period: &str)
        -> anyhow::Result<Option<VendorLedger>>;
    /// Recent periods, newest first, headline figures only.
    ///
    /// Without this a vendor can only ever see the period still accruing —
    /// `find_open` returns nothing once a period closes, so what they are owed
    /// and what has already been paid were both invisible. Entries are left
    /// unloaded on purpose: this feeds a summary, and fetching every entry of
    /// every period to display three totals would be a scan per card render.
    async fn list_recent(&self, tenant_id: Uuid, vendor_id: Uuid, limit: i64)
        -> anyhow::Result<Vec<LedgerPeriod>>;
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
