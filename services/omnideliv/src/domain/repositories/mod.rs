//! Repository contracts.
//!
//! TENANCY: every method takes `tenant_id` first. There is no database-level
//! policy in this schema (see migration 0001), so the signature is the
//! enforcement point.
//!
//! ONE EXCEPTION, and it is deliberate: `VenueRepository::find_table_by_token`.
//! A diner scanning a printed QR code is unauthenticated and carries no tenant,
//! so the tenant is an *output* of that lookup rather than an input. It is the
//! only method here that resolves across tenants, its key is unique
//! platform-wide to make that unambiguous, and every caller re-scopes on the
//! tenant it returns. If a second such method is ever added, say why in the
//! same breath — an unnoticed second one is how the rule quietly stops being a
//! rule.

use async_trait::async_trait;
use uuid::Uuid;

use chrono::{DateTime, Utc};

use crate::domain::entities::{
    Availability, Basket, BasketConflict, CatalogItem, CatalogSource, LedgerStatus, LegStatus,
    Order, Table, TableSession, TelemetryEvent, Vendor, VendorLedger, Venue, Vertical,
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
    /// The breakdown a receipt needs. Already columns on `omnideliv.orders` —
    /// the list simply never selected them, so a customer could see what they
    /// owed and never what for.
    pub goods_total_cents:  i64,
    pub delivery_fee_cents: i64,
    pub tip_cents:          i64,
    pub grand_total_cents: i64,
    /// `"cod"` / `"online"` and `pending` | `authorized` | `captured` |
    /// `voided` | `failed`. Strings rather than the enums because this is a
    /// projection for a list view, mapped straight onto the wire — the
    /// enums are what `Order` itself carries.
    pub payment_method:    String,
    pub payment_status:    String,
    /// Taken online. `0` for COD. The list needs it for the same reason the
    /// tracking screen does: without it a prepaid order in flight is
    /// indistinguishable from one whose full total is still owed in cash.
    pub prepaid_amount_cents: i64,
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

/// The outcome of asking a leg to move.
///
/// `NoOp` is not an error. A tablet that retried, or a second member of staff
/// who tapped Accept a moment later, should be told the leg is accepted — which
/// is true — rather than shown a failure for something that did happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegTransition {
    /// Carries the leg's identifying context because the caller publishes an
    /// event immediately afterwards and would otherwise have to read the row
    /// back to learn which order it belonged to. The UPDATE returns it.
    Applied {
        to:                   LegStatus,
        order_id:             Uuid,
        goods_subtotal_cents: i64,
    },
    NoOp { current: LegStatus },
}

/// What a vendor action returns, and what a replayed idempotency key returns
/// verbatim on a retry.
///
/// Lives here rather than in the HTTP module because the repository stores it:
/// the stored response and the live response must be the same shape, or a
/// retried request would answer differently from the original.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TransitionResponse {
    pub leg_id: Uuid,
    pub status: String,
    /// False when the leg was already in the target state — a retry from a
    /// tablet that lost its connection, or a second member of staff.
    pub changed: bool,
}

/// A queue row.
///
/// Carries the order context a store needs to cook and nothing about the
/// customer: a stall has no reason to hold a delivery address, and a foodcourt
/// neighbour has no reason to learn what this one is making.
#[derive(Debug, Clone, serde::Serialize)]
pub struct VendorLegRow {
    pub leg_id:               Uuid,
    pub order_id:             Uuid,
    pub status:               String,
    pub goods_subtotal_cents: i64,
    pub ready_in_minutes:     Option<i32>,
    pub accepted_at:          Option<chrono::DateTime<chrono::Utc>>,
    pub created_at:           chrono::DateTime<chrono::Utc>,
}

/// A leg the sweep is considering.
///
/// Carries what an alert needs and nothing else. The sweep runs across every
/// tenant, so it must not become proportional to basket size — the same reason
/// `find_awaiting_courier` does not hydrate an order's legs.
#[derive(Debug, Clone)]
pub struct AwaitingLeg {
    pub leg_id:               Uuid,
    pub order_id:             Uuid,
    pub tenant_id:            Uuid,
    pub vendor_id:            Uuid,
    pub goods_subtotal_cents: i64,
    pub created_at:           chrono::DateTime<chrono::Utc>,
    /// Set once the sweep has raised this leg to a human. Present so the sweep
    /// escalates once rather than on every tick — a leg unanswered for an hour
    /// otherwise pages ops sixty times about one order.
    pub escalated_at:         Option<chrono::DateTime<chrono::Utc>>,
}

#[async_trait]
pub trait VendorLegRepository: Send + Sync {
    /// Legs still waiting for their store to answer, oldest first.
    ///
    /// Deliberately across all tenants: an unanswered order is an operator
    /// concern, not a customer request, and scoping it per tenant would mean
    /// the sweep only runs for tenants someone remembered to enumerate. Same
    /// reasoning as `OrderRepository::find_awaiting_courier`.
    async fn find_awaiting_acceptance(&self) -> anyhow::Result<Vec<AwaitingLeg>>;

    /// Stamps a leg as raised, so it is not raised again on the next tick.
    ///
    /// Deliberately not a status change: the collection consumer refuses to
    /// credit a leg that is not awaiting collection, so moving the status here
    /// would stop a store being paid for food it actually cooked.
    async fn mark_escalated(&self, tenant_id: Uuid, leg_id: Uuid) -> anyhow::Result<()>;

    /// Moves one leg to `to`, atomically, from whichever states legally precede
    /// it.
    ///
    /// The caller does not pass a predecessor list. It is derived from
    /// `LegStatus::can_transition_to`, so the transition graph is stated in the
    /// domain exactly once instead of being re-hand-written at every call site
    /// where the two could silently drift apart.
    ///
    /// Scoped by `vendor_id` as well as `tenant_id` so a store cannot transition
    /// another store's leg by guessing an id — the same reason the HTTP surface
    /// resolves the vendor from claims rather than from the path.
    ///
    /// No network I/O happens inside this call. Publishing the event is the
    /// caller's job, after the write has committed — the same rule dispatch's
    /// claim transaction follows.
    async fn transition(
        &self,
        tenant_id:        Uuid,
        vendor_id:        Uuid,
        leg_id:           Uuid,
        to:               LegStatus,
        ready_in_minutes: Option<i32>,
        rejected_reason:  Option<&str>,
    ) -> anyhow::Result<LegTransition>;

    /// This vendor's live legs, oldest first. The queue.
    async fn list_open(&self, tenant_id: Uuid, vendor_id: Uuid)
        -> anyhow::Result<Vec<VendorLegRow>>;

    /// A previously stored response for this key, if the request is a replay.
    async fn find_idempotent_response(
        &self,
        tenant_id: Uuid,
        vendor_id: Uuid,
        key:       &str,
    ) -> anyhow::Result<Option<TransitionResponse>>;

    async fn record_idempotent_response(
        &self,
        tenant_id: Uuid,
        vendor_id: Uuid,
        key:       &str,
        leg_id:    Uuid,
        action:    &str,
        response:  &TransitionResponse,
    ) -> anyhow::Result<()>;
}

#[async_trait]
pub trait VenueRepository: Send + Sync {
    /// Resolve a printed table code.
    ///
    /// **The one method in this file that does not take `tenant_id`, and it
    /// cannot.** A diner scanning a code is unauthenticated and carries no
    /// tenant; the tenant is an OUTPUT of this lookup, not an input. That is
    /// why `tables.token` is UNIQUE platform-wide rather than per venue — a
    /// per-venue unique would make this query ambiguous.
    ///
    /// Everything downstream re-scopes on the tenant this returns. The token
    /// itself is the only credential, which is why `orderable_now` and the
    /// session cap exist: holding it must not be enough.
    async fn find_table_by_token(&self, token: &str) -> anyhow::Result<Option<(Table, Venue)>>;

    /// How many sessions are live at this table right now.
    ///
    /// Bounds how many parties one printed code can open at once. A four-top
    /// does not need fifty, and without this a photographed code is an
    /// unbounded session factory.
    async fn count_live_sessions(&self, table_id: Uuid, now: DateTime<Utc>) -> anyhow::Result<i64>;

    async fn create_session(&self, session: &TableSession) -> anyhow::Result<()>;

    /// Every table at a venue, for the operator printing them.
    async fn list_tables(&self, tenant_id: Uuid, venue_id: Uuid) -> anyhow::Result<Vec<Table>>;

    /// The live session behind a diner's token, if it is still live.
    ///
    /// The token's `user_id` IS the session id, so this is a primary-key read.
    /// Returns `None` for an ended or expired session even though the JWT may
    /// still verify — the row is the record, and a token outliving its session
    /// must not still order.
    async fn find_live_session(
        &self,
        tenant_id: Uuid,
        session_id: Uuid,
        now: DateTime<Utc>,
    ) -> anyhow::Result<Option<TableSession>>;

    /// Does this vendor sell at this venue?
    ///
    /// The guard that makes a table order a VENUE order. Without it a diner at
    /// one restaurant could add items from a vendor across town, because the
    /// basket takes `vendor_id` from the client.
    async fn vendor_is_at_venue(
        &self,
        tenant_id: Uuid,
        venue_id: Uuid,
        vendor_id: Uuid,
    ) -> anyhow::Result<bool>;

    /// Replace a table's printed code, invalidating the one on the wall.
    ///
    /// The answer to a leaked code: rotation is an operator action taking
    /// minutes, not an incident. Returns false when the table is not this
    /// tenant's, so a caller cannot rotate someone else's code by guessing.
    async fn rotate_token(
        &self,
        tenant_id: Uuid,
        table_id: Uuid,
        new_token: &str,
    ) -> anyhow::Result<bool>;

    // ---- Setup. Without these the whole feature is unreachable: the schema
    // ---- shipped with no way to put a row in it, so every venue and table on
    // ---- the platform had to be inserted by hand in SQL.

    /// Persist a new venue.
    async fn create_venue(&self, venue: &Venue) -> anyhow::Result<()>;

    /// Every venue this tenant runs, newest first.
    async fn list_venues(&self, tenant_id: Uuid) -> anyhow::Result<Vec<Venue>>;

    /// One venue, scoped to the tenant so a guessed id reads as absent.
    async fn find_venue(&self, tenant_id: Uuid, venue_id: Uuid) -> anyhow::Result<Option<Venue>>;

    /// Persist new tables, all or none.
    ///
    /// **In one transaction on purpose.** `tables` has `UNIQUE (venue_id,
    /// label)`, so one clash partway down a batch of twenty would otherwise
    /// leave the earlier rows committed and the operator unable to tell which
    /// of their labels landed.
    ///
    /// The caller must have already confirmed the venue is this tenant's --
    /// `tables.venue_id` has a foreign key but no tenant check of its own, so
    /// nothing at the database layer stops a table being hung off another
    /// tenant's venue.
    async fn create_tables(&self, tables: &[Table]) -> anyhow::Result<()>;

    /// Let a vendor sell at a venue.
    ///
    /// The write side of `vendor_is_at_venue`, which is the guard that makes a
    /// table order a venue order. With no rows here no vendor is orderable from
    /// any table, which is exactly the state the platform shipped in.
    ///
    /// Idempotent: linking twice is not an error, because an operator clicking
    /// twice is not one. Returns false when the venue or vendor is not this
    /// tenant's.
    async fn link_vendor(
        &self,
        tenant_id: Uuid,
        venue_id:  Uuid,
        vendor_id: Uuid,
    ) -> anyhow::Result<bool>;

    /// Stop a vendor selling at a venue. Returns false when no link existed.
    async fn unlink_vendor(
        &self,
        tenant_id: Uuid,
        venue_id:  Uuid,
        vendor_id: Uuid,
    ) -> anyhow::Result<bool>;

    /// The vendors selling at a venue, as (id, name).
    async fn list_venue_vendors(
        &self,
        tenant_id: Uuid,
        venue_id:  Uuid,
    ) -> anyhow::Result<Vec<(Uuid, String)>>;

    /// Stamp `printed_at` once the codes are actually on paper.
    ///
    /// The counterpart to rotation clearing it. Together they answer "is what
    /// is on the wall the current code", which is otherwise unknowable.
    async fn mark_printed(
        &self,
        tenant_id: Uuid,
        table_id:  Uuid,
        now:       DateTime<Utc>,
    ) -> anyhow::Result<bool>;
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
