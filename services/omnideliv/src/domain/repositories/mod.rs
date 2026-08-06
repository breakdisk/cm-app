//! Repository contracts.
//!
//! TENANCY: every method takes `tenant_id` first. There is no database-level
//! policy in this schema (see migration 0001), so the signature is the
//! enforcement point.

use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::entities::{Availability, Basket, CatalogItem, Vendor, Vertical};

#[async_trait]
pub trait VendorRepository: Send + Sync {
    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<Option<Vendor>>;
    async fn save(&self, vendor: &Vendor) -> anyhow::Result<()>;

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
}

#[async_trait]
pub trait BasketRepository: Send + Sync {
    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<Option<Basket>>;
    /// Persists the basket and its sub-intents and lines as one unit.
    async fn save(&self, basket: &Basket) -> anyhow::Result<()>;
}
