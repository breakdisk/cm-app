use std::sync::Arc;

use uuid::Uuid;

use crate::domain::entities::{Availability, AvailabilityState, Vendor, VendorStatus, Vertical};
use crate::domain::repositories::{CatalogRepository, ItemWithAvailability, VendorRepository};

/// An item plus the agent-facing judgement about it.
#[derive(Debug, Clone)]
pub struct ScoredItem {
    pub item_with_availability: ItemWithAvailability,
    /// True when the agent should line up a substitute before dispatch.
    pub warrants_substitute:    bool,
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
                updated_at: chrono::Utc::now(),
                updated_by: Some(user_id),
            })
            .await?;
        Ok(true)
    }

    pub async fn set_availability(&self, a: &Availability) -> anyhow::Result<()> {
        self.catalog.set_availability(a).await
    }
}
