use std::sync::Arc;

use uuid::Uuid;

use crate::domain::entities::{Availability, Vendor, Vertical};
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

    pub async fn set_availability(&self, a: &Availability) -> anyhow::Result<()> {
        self.catalog.set_availability(a).await
    }
}
