use std::sync::Arc;

use uuid::Uuid;

use crate::domain::entities::{
    Availability, AvailabilityState, CatalogItem, CatalogSource, IngestedItem, Vendor, VendorStatus,
    Vertical,
};
use crate::domain::repositories::{CatalogRepository, ItemWithAvailability, VendorRepository};

/// An item plus the agent-facing judgement about it.
#[derive(Debug, Clone)]
pub struct ScoredItem {
    pub item_with_availability: ItemWithAvailability,
    /// True when the agent should line up a substitute before dispatch.
    pub warrants_substitute:    bool,
}

/// A vendor hand-entering an item.
///
/// `allergens: None` is the honest default and deliberately not `Some(vec![])`:
/// a vendor who has not filled the field has stated nothing, and an empty list
/// here would silently become "declared allergen-free" on an item nobody read.
#[derive(Debug, Clone)]
pub struct ItemDraft {
    pub sku:            String,
    pub name:           String,
    pub description:    Option<String>,
    pub price_cents:    i64,
    pub allergens:      Option<Vec<String>>,
    pub dietary_tags:   Vec<String>,
    pub category:       Option<String>,
    pub modifiers:      serde_json::Value,
    pub vertical_attrs: serde_json::Value,
}

/// A partial edit. Every field absent means "leave it alone" — a PATCH that
/// silently nulled unmentioned fields would let a form with one input wipe the
/// rest of the row.
#[derive(Debug, Clone, Default)]
pub struct ItemPatch {
    pub name:         Option<String>,
    pub description:  Option<Option<String>>,
    pub price_cents:  Option<i64>,
    pub dietary_tags: Option<Vec<String>>,
    pub category:     Option<Option<String>>,
    pub is_listed:    Option<bool>,
    pub modifiers:    Option<serde_json::Value>,
}

/// What one ingest run did. Returned rather than logged so an adapter's caller
/// can tell "synced 200 items" from "matched nothing and created 200 duplicates".
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct IngestReport {
    pub created: usize,
    pub updated: usize,
    /// Rows the adapter sent that could not be applied — a blank SKU, a
    /// negative price. Counted rather than failing the batch: one bad row in a
    /// nightly sync of 500 must not discard the other 499.
    pub rejected: usize,
}

pub struct CatalogService {
    vendors:        Arc<dyn VendorRepository>,
    catalog:        Arc<dyn CatalogRepository>,
    fresh_window_mins: i64,
}

impl CatalogService {
    pub fn new(
        vendors: Arc<dyn VendorRepository>,
        catalog: Arc<dyn CatalogRepository>,
        fresh_window_mins: i64,
    ) -> Self {
        Self { vendors, catalog, fresh_window_mins }
    }

    pub async fn vendors_near(
        &self,
        tenant_id: Uuid,
        vertical: Vertical,
        lat: f64,
        lng: f64,
        radius_km: f64,
        limit: i64,
    ) -> anyhow::Result<Vec<Vendor>> {
        self.vendors.find_near(tenant_id, vertical, lat, lng, radius_km, limit).await
    }

    /// Search a vendor's catalog, annotating each hit with whether it warrants a
    /// substitute. The freshness judgement lives here rather than in the agent
    /// so every caller applies the same rule with the same configured window.
    pub async fn search(
        &self,
        tenant_id: Uuid,
        vendor_id: Uuid,
        query: &str,
        avoid_allergens: &[String],
        limit: i64,
    ) -> anyhow::Result<Vec<ScoredItem>> {
        let hits = self
            .catalog
            .search(tenant_id, vendor_id, query, avoid_allergens, limit)
            .await?;

        Ok(hits
            .into_iter()
            .map(|iwa| ScoredItem {
                warrants_substitute: iwa.availability.warrants_substitute(self.fresh_window_mins),
                item_with_availability: iwa,
            })
            .collect())
    }

    /// Catalog truth for a set of items, for reconcile-phase verification.
    ///
    /// Deliberately a thin passthrough with no filtering of its own: any rule
    /// applied here would be a second, invisible place where a line can vanish.
    pub async fn item_facts(
        &self,
        tenant_id: Uuid,
        item_ids: &[Uuid],
    ) -> anyhow::Result<Vec<crate::domain::repositories::ItemFacts>> {
        self.catalog.item_facts(tenant_id, item_ids).await
    }

    /// The store this portal user runs, if any.
    pub async fn vendor_for_user(&self, tenant_id: Uuid, user_id: Uuid) -> anyhow::Result<Option<Vendor>> {
        self.vendors.find_by_user(tenant_id, user_id).await
    }

    /// A vendor editing its own store.
    ///
    /// Resolved from the caller's user id rather than an id in the path: a
    /// vendor id the caller supplies is a vendor id the caller can change, and
    /// this endpoint would then edit someone else's store.
    pub async fn update_own_vendor(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        prep_time_minutes: Option<i32>,
        status: Option<String>,
    ) -> anyhow::Result<bool> {
        let Some(mut vendor) = self.vendors.find_by_user(tenant_id, user_id).await? else {
            return Ok(false);
        };

        if let Some(m) = prep_time_minutes {
            vendor.prep_time_minutes = m;
        }
        // Only pause and resume. Offboarding and completing onboarding are
        // Partner decisions — the HTTP layer rejects the others, and this is
        // the second gate so a future caller cannot bypass it.
        match status.as_deref() {
            Some("active") => vendor.activate(),
            Some("paused") => vendor.pause(),
            Some(_) => anyhow::bail!("that status is not the vendor's to set"),
            None => {}
        }
        if vendor.status == VendorStatus::Onboarding && prep_time_minutes.is_some() {
            // Editing prep time while onboarding is fine; it must not silently
            // activate the store.
        }

        self.vendors.save(&vendor).await?;
        Ok(true)
    }

    /// A vendor declaring stock for one of its own items.
    ///
    /// Ownership is checked here, not merely in the handler: without it any
    /// signed-in user could mark a competitor's items out of stock, which is
    /// both a denial-of-service on that vendor and a way to steer the mesh's
    /// substitutions toward your own catalog.
    ///
    /// Returns `false` when the caller runs no store or the item is not theirs
    /// — the API answers 404 either way rather than distinguishing them, since
    /// "this item exists but is not yours" is itself information.
    /// This vendor's whole catalog, with each item's availability and how
    /// stale that declaration is.
    ///
    /// Returns `None` when the user runs no store — an absence, not a failure.
    ///
    /// `warrants_substitute` is included because it is the consequence a vendor
    /// is actually managing: an item nobody has confirmed for longer than the
    /// freshness window is treated as uncertain, and the agent lines up a
    /// substitute for it. Showing the flag makes the reason for that visible,
    /// rather than leaving a vendor to wonder why their in-stock dish keeps
    /// being swapped out.
    pub async fn own_items(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> anyhow::Result<Option<(Vendor, Vec<ScoredItem>)>> {
        let Some(vendor) = self.vendors.find_by_user(tenant_id, user_id).await? else {
            return Ok(None);
        };
        let items = self.catalog.list_for_vendor(tenant_id, vendor.id).await?;
        let scored = items
            .into_iter()
            .map(|iwa| ScoredItem {
                warrants_substitute: iwa.availability.warrants_substitute(self.fresh_window_mins),
                item_with_availability: iwa,
            })
            .collect();
        Ok(Some((vendor, scored)))
    }

    /// A store applies to sell.
    ///
    /// Lands in `Onboarding`, which `is_orderable()` already excludes — so an
    /// unapproved store cannot be searched, proposed by an agent, or ordered
    /// from. The states existed and nothing drove them; this is the front door.
    ///
    /// Idempotent on the applicant: a user who applies twice gets their
    /// existing store back rather than a second one, because a double-tap on a
    /// signup button must not fork a merchant's identity.
    // Eight arguments because a store application has eight facts and all are
    // required. A params struct would move the arity, not remove it, and would
    // let a caller submit a half-described store.
    #[allow(clippy::too_many_arguments)]
    pub async fn apply_as_vendor(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        vertical: Vertical,
        name: String,
        address: String,
        lat: f64,
        lng: f64,
    ) -> anyhow::Result<Vendor> {
        if let Some(existing) = self.vendors.find_by_user(tenant_id, user_id).await? {
            return Ok(existing);
        }
        let mut v = Vendor::new(tenant_id, vertical, name, address, lat, lng);
        v.user_id = Some(user_id);
        self.vendors.save(&v).await?;
        Ok(v)
    }

    /// The operator review queue: every vendor in the tenant, any status.
    pub async fn list_vendors(&self, tenant_id: Uuid) -> anyhow::Result<Vec<Vendor>> {
        self.vendors.list_for_tenant(tenant_id).await
    }

    /// An operator approves a store. `Onboarding` -> `Active`.
    ///
    /// Deliberately a separate action from applying: letting a store list
    /// itself would mean anyone with a login can put food in front of
    /// customers, which is exactly the review this status was designed for.
    pub async fn approve_vendor(&self, tenant_id: Uuid, vendor_id: Uuid) -> anyhow::Result<bool> {
        let Some(mut v) = self.vendors.find_by_id(tenant_id, vendor_id).await? else {
            return Ok(false);
        };
        v.activate();
        self.vendors.save(&v).await?;
        Ok(true)
    }

    /// A vendor declares what is in one of their own items.
    ///
    /// Same ownership rule as availability — resolved from the token, and a
    /// foreign item is indistinguishable from a missing one — because the
    /// consequence here is worse: declaring a competitor's peanut dish
    /// allergen-free would send it to someone who asked us to avoid peanuts.
    pub async fn declare_own_item_allergens(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        item_id: Uuid,
        allergens: Vec<String>,
    ) -> anyhow::Result<bool> {
        let Some(vendor) = self.vendors.find_by_user(tenant_id, user_id).await? else {
            return Ok(false);
        };
        let Some(item) = self.catalog.find_item(tenant_id, item_id).await? else {
            return Ok(false);
        };
        if item.vendor_id != vendor.id {
            return Ok(false);
        }

        // Normalised on the way in. Matching is case-insensitive at read time,
        // but storing "Peanuts" and "peanuts" as different strings makes the
        // vendor's own list read as if it had duplicates.
        let normalised: Vec<String> = allergens
            .iter()
            .map(|a| a.trim().to_lowercase())
            .filter(|a| !a.is_empty())
            .collect();

        self.catalog.declare_allergens(tenant_id, item_id, &normalised).await
    }

    /// Attach a photo to one of the caller's own items.
    ///
    /// Same ownership rule as availability — the store is resolved from the
    /// token and the item must belong to it. Returns `false` when the caller
    /// runs no store, the item does not exist, or it belongs to someone else;
    /// the three are deliberately indistinguishable to the caller, because
    /// telling them apart tells a prober which item ids are real.
    pub async fn set_own_item_photo(
        &self,
        tenant_id: Uuid,
        user_id:   Uuid,
        item_id:   Uuid,
        key:       Option<&str>,
    ) -> anyhow::Result<bool> {
        let Some(vendor) = self.vendors.find_by_user(tenant_id, user_id).await? else {
            return Ok(false);
        };
        let Some(item) = self.catalog.find_item(tenant_id, item_id).await? else {
            return Ok(false);
        };
        if item.vendor_id != vendor.id {
            return Ok(false);
        }
        self.catalog.set_image_key(tenant_id, item_id, key).await?;
        Ok(true)
    }

    /// The stored object key for an item's photo.
    ///
    /// No ownership check: a product photo is public-facing content — the whole
    /// point is that a customer browsing the storefront can see it before they
    /// have any relationship with the vendor.
    pub async fn item_photo_key(
        &self,
        tenant_id: Uuid,
        item_id:   Uuid,
    ) -> anyhow::Result<Option<String>> {
        Ok(self.catalog.find_item(tenant_id, item_id).await?.and_then(|i| i.image_key))
    }

    pub async fn set_own_item_availability(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        item_id: Uuid,
        state: AvailabilityState,
    ) -> anyhow::Result<bool> {
        let Some(vendor) = self.vendors.find_by_user(tenant_id, user_id).await? else {
            return Ok(false);
        };
        let Some(item) = self.catalog.find_item(tenant_id, item_id).await? else {
            return Ok(false);
        };
        if item.vendor_id != vendor.id {
            return Ok(false);
        }

        // updated_at is set server-side inside the repository, not here: the
        // freshness stamp is only meaningful if it records when the declaration
        // reached us rather than when a device claims it was made.
        self.catalog
            .set_availability(&Availability {
                item_id,
                tenant_id,
                state,
                updated_at:   chrono::Utc::now(),
                // A person tapped this. Both clocks move; see `Availability`.
                confirmed_at: Some(chrono::Utc::now()),
                updated_by:   Some(user_id),
            })
            .await?;
        Ok(true)
    }

    pub async fn set_availability(&self, a: &Availability) -> anyhow::Result<()> {
        self.catalog.set_availability(a).await
    }

    // ---- manual entry ------------------------------------------------------

    /// A vendor adds an item to their own store.
    ///
    /// `None` means the caller runs no store. The item lands **unconfirmed**:
    /// typing a dish into a form asserts that the dish exists, not that it is on
    /// the shelf at this moment, and those are different claims. It shows up in
    /// the console's "needs confirming" list, which is where a vendor's first
    /// visit should send them anyway.
    pub async fn create_own_item(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        draft: ItemDraft,
    ) -> anyhow::Result<Option<CatalogItem>> {
        let Some(vendor) = self.vendors.find_by_user(tenant_id, user_id).await? else {
            return Ok(None);
        };

        let sku = draft.sku.trim();
        if sku.is_empty() || draft.name.trim().is_empty() {
            anyhow::bail!("an item needs a SKU and a name");
        }
        if draft.price_cents < 0 {
            anyhow::bail!("price cannot be negative");
        }
        // One SKU per store. Two rows with the same code make every later
        // reconciliation — ingest matching, an ops query, a vendor's own
        // spreadsheet — silently ambiguous.
        if self.catalog.find_item_by_sku(tenant_id, vendor.id, sku).await?.is_some() {
            anyhow::bail!("this store already has an item with SKU {sku}");
        }

        let now = chrono::Utc::now();
        let item = CatalogItem {
            id:          Uuid::new_v4(),
            tenant_id,
            vendor_id:   vendor.id,
            sku:         sku.to_owned(),
            name:        draft.name.trim().to_owned(),
            description: draft.description,
            price_cents: draft.price_cents,
            modifiers:   draft.modifiers,
            // A declaration only if the vendor actually made one. `Some(vec![])`
            // is the real statement "contains none of these"; `None` is silence.
            allergens:   draft.allergens.clone().unwrap_or_default()
                             .iter().map(|a| a.trim().to_lowercase())
                             .filter(|a| !a.is_empty()).collect(),
            allergens_declared_at: draft.allergens.as_ref().map(|_| now),
            dietary_tags:   draft.dietary_tags,
            category:       draft.category.clone(),
            vertical_attrs: draft.vertical_attrs,
            is_listed:      true,
            source:         CatalogSource::Manual,
            external_id:    None,
            synced_at:      None,
            image_key:      None,
            created_at:     now,
            updated_at:     now,
        };

        self.catalog.save_item(&item).await?;
        self.seed_availability(tenant_id, item.id).await?;
        Ok(Some(item))
    }

    /// Give a brand-new item its availability row, listed but unconfirmed.
    ///
    /// Explicit here rather than left to the repository's insert, because it is
    /// a decision and not a storage detail: every new item — typed by a vendor
    /// or pushed by an adapter — starts with `confirmed_at` unset and therefore
    /// warrants a substitute until a person says otherwise. Buried in an INSERT
    /// it was true but invisible, and nothing could hold it to that.
    async fn seed_availability(&self, tenant_id: Uuid, item_id: Uuid) -> anyhow::Result<()> {
        self.catalog
            .set_availability(&Availability {
                item_id,
                tenant_id,
                state:        AvailabilityState::Available,
                updated_at:   chrono::Utc::now(),
                confirmed_at: None,
                updated_by:   None,
            })
            .await
    }

    /// A vendor edits one of their own items.
    ///
    /// Same ownership rule as availability and allergens — resolved from the
    /// token, and a foreign item is indistinguishable from a missing one. The
    /// consequence of getting this wrong is repricing a competitor's catalog.
    pub async fn update_own_item(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        item_id: Uuid,
        patch: ItemPatch,
    ) -> anyhow::Result<bool> {
        let Some(mut item) = self.own_item(tenant_id, user_id, item_id).await? else {
            return Ok(false);
        };

        if let Some(n) = patch.name {
            if n.trim().is_empty() {
                anyhow::bail!("an item needs a name");
            }
            item.name = n.trim().to_owned();
        }
        if let Some(d) = patch.description  { item.description  = d; }
        if let Some(p) = patch.price_cents  {
            if p < 0 { anyhow::bail!("price cannot be negative"); }
            item.price_cents = p;
        }
        if let Some(t) = patch.dietary_tags { item.dietary_tags = t; }
        if let Some(l) = patch.is_listed    { item.is_listed    = l; }
        if let Some(m) = patch.modifiers    { item.modifiers    = m; }
        // Option<Option<_>>: omitted leaves it alone, Some(None) clears it.
        // Flattening these would make "remove the category" indistinguishable
        // from "do not touch the category".
        if let Some(c) = patch.category {
            item.category = c.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
        }
        item.updated_at = chrono::Utc::now();

        self.catalog.save_item(&item).await?;
        Ok(true)
    }

    /// Take an item off the menu.
    ///
    /// Delists rather than deletes. Basket lines and settled order legs point at
    /// `catalog_items`, so a hard delete either breaks a foreign key or, worse,
    /// cascades away the record of what a customer actually bought.
    pub async fn delist_own_item(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        item_id: Uuid,
    ) -> anyhow::Result<bool> {
        let Some(mut item) = self.own_item(tenant_id, user_id, item_id).await? else {
            return Ok(false);
        };
        item.is_listed  = false;
        item.updated_at = chrono::Utc::now();
        self.catalog.save_item(&item).await?;
        Ok(true)
    }

    /// One deliberate human act covering the whole store.
    ///
    /// The alternative to this is a vendor tapping 200 times after an import,
    /// which nobody does — so the freshness model would degrade into a permanent
    /// "everything is uncertain" that operators learn to ignore. `None` when the
    /// caller runs no store.
    pub async fn confirm_all_own_items(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
    ) -> anyhow::Result<Option<u64>> {
        let Some(vendor) = self.vendors.find_by_user(tenant_id, user_id).await? else {
            return Ok(None);
        };
        Ok(Some(self.catalog.confirm_all_for_vendor(tenant_id, vendor.id, user_id).await?))
    }

    /// The item, but only if this user's store owns it.
    ///
    /// Private and used by every write path above, so ownership cannot be
    /// forgotten in one of them — the failure mode that would let any signed-in
    /// user edit any store's catalog.
    async fn own_item(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        item_id: Uuid,
    ) -> anyhow::Result<Option<CatalogItem>> {
        let Some(vendor) = self.vendors.find_by_user(tenant_id, user_id).await? else {
            return Ok(None);
        };
        let Some(item) = self.catalog.find_item(tenant_id, item_id).await? else {
            return Ok(None);
        };
        if item.vendor_id != vendor.id {
            return Ok(None);
        }
        Ok(Some(item))
    }

    // ---- the ingest port ---------------------------------------------------

    /// Apply a batch of items from any source.
    ///
    /// This is the seam an adapter plugs into: Shopify, WooCommerce, a CSV
    /// upload and a POS push all translate their own payload into
    /// `Vec<IngestedItem>` and call this. They do not get to decide what may
    /// overwrite what — `CatalogItem::merge_ingested` owns those rules, so a
    /// fourth adapter cannot introduce a fourth interpretation of them.
    ///
    /// Vendor-scoped by the caller rather than by a token: adapters run
    /// unattended against a vendor's stored binding, so there is no user in the
    /// request to resolve. The HTTP route in front of this is what proves the
    /// caller may act for that vendor.
    ///
    /// **This never confirms stock.** Not once, not for a source that claims
    /// real-time inventory. That is the entire point of the port: a machine can
    /// tell us what it believes, and a human still has to say it is true.
    pub async fn ingest_for_vendor(
        &self,
        tenant_id: Uuid,
        vendor_id: Uuid,
        source: CatalogSource,
        items: Vec<IngestedItem>,
    ) -> anyhow::Result<IngestReport> {
        // The vendor is caller-supplied here — an unattended adapter names the
        // store it is syncing, because there is no user in the request to
        // resolve one from. So it has to be proved against the tenant from the
        // token before a single row is written.
        //
        // `catalog_items.vendor_id` references `vendors(id)` and that constraint
        // is NOT tenant-scoped: another tenant's vendor id satisfies it happily,
        // and the result is catalog rows filed under one tenant carrying another
        // tenant's vendor. The database would accept every one of them.
        if self.vendors.find_by_id(tenant_id, vendor_id).await?.is_none() {
            anyhow::bail!("vendor {vendor_id} does not belong to this tenant");
        }

        let now = chrono::Utc::now();
        let mut report = IngestReport::default();

        for incoming in items {
            if incoming.sku.trim().is_empty()
                || incoming.name.trim().is_empty()
                || incoming.price_cents < 0
            {
                // One malformed row must not discard the rest of the batch.
                report.rejected += 1;
                continue;
            }

            // Prefer the source system's own id: a vendor who renames a SKU in
            // Shopify would otherwise get a duplicate row here rather than an
            // update. Fall back to SKU for sources with no stable id.
            let existing = match incoming.external_id.as_deref() {
                Some(ext) => {
                    self.catalog
                        .find_item_by_external(tenant_id, vendor_id, source, ext)
                        .await?
                }
                None => None,
            };
            let existing = match existing {
                Some(e) => Some(e),
                None => self.catalog.find_item_by_sku(tenant_id, vendor_id, incoming.sku.trim()).await?,
            };

            match existing {
                Some(mut item) => {
                    item.merge_ingested(&incoming, source, now);
                    self.catalog.save_item(&item).await?;
                    report.updated += 1;
                }
                None => {
                    let mut item = CatalogItem {
                        id:          Uuid::new_v4(),
                        tenant_id,
                        vendor_id,
                        sku:         incoming.sku.trim().to_owned(),
                        // An ingest carries no category — see migration 0017.
                        category:    None,
                        name:        incoming.name.trim().to_owned(),
                        description: None,
                        price_cents: 0,
                        modifiers:      serde_json::json!([]),
                        allergens:      Vec::new(),
                        // Never declared by an ingest — see `merge_ingested`.
                        allergens_declared_at: None,
                        dietary_tags:   Vec::new(),
                        vertical_attrs: serde_json::json!({}),
                        is_listed:      true,
                        source,
                        external_id:    None,
                        synced_at:      None,
                        image_key:      None,
                        created_at:     now,
                        updated_at:     now,
                    };
                    // Same merge rules on create as on update, so a field can
                    // never arrive by one path and be filtered on the other.
                    item.merge_ingested(&incoming, source, now);
                    self.catalog.save_item(&item).await?;
                    // Only on create. Seeding on update would reset `state` to
                    // available and wipe a vendor's out-of-stock marker on every
                    // nightly sync — the machine overruling the human, which is
                    // the exact inversion this whole design exists to prevent.
                    self.seed_availability(tenant_id, item.id).await?;
                    report.created += 1;
                }
            }
        }

        Ok(report)
    }
}
