//! Implements the mesh's `MeshCatalog` port over `CatalogService`.
//!
//! Returns plain JSON because that is what a tool result is. Shaping it here
//! rather than in the tool box keeps the mesh crate free of product types.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use uuid::Uuid;

use omnideliv_mesh::tools::MeshCatalog;

use crate::application::services::{CatalogService, CourierSupply};
use crate::domain::entities::Vertical;

pub struct CatalogServiceAdapter {
    catalog: Arc<CatalogService>,
    /// Optional on purpose. field-ops may not be deployed in every environment,
    /// and a Fleet agent that cannot ask about supply should get an honest
    /// `null` rather than the service failing to start.
    supply:  Option<Arc<dyn CourierSupply>>,
}

impl CatalogServiceAdapter {
    pub fn new(catalog: Arc<CatalogService>) -> Self {
        Self { catalog, supply: None }
    }

    /// Wire the field-ops supply lookup. Without it `courier_supply` answers
    /// `null`, which the Fleet agent is told to read as "unknown".
    pub fn with_supply(mut self, supply: Arc<dyn CourierSupply>) -> Self {
        self.supply = Some(supply);
        self
    }
}

fn parse_vertical(s: &str) -> anyhow::Result<Vertical> {
    Ok(match s {
        "restaurant" => Vertical::Restaurant,
        "grocery"    => Vertical::Grocery,
        "pharmacy"   => Vertical::Pharmacy,
        "florist"    => Vertical::Florist,
        "retail"     => Vertical::Retail,
        other => anyhow::bail!("unknown vertical from the mesh: {other}"),
    })
}

/// Shape the Fleet agent's supply answer.
///
/// Free rather than a method so the null-vs-zero rule can be tested without
/// standing up a `CatalogService` and its repositories — the rule is the whole
/// point, and a test that needs a database to check it would not get written.
async fn supply_json(
    supply: Option<&Arc<dyn CourierSupply>>,
    tenant_id: Uuid,
    lat: f64,
    lng: f64,
    radius_km: f64,
) -> serde_json::Value {
    let Some(supply) = supply else {
        return json!({
            "available": null,
            "note": "courier supply lookup is not configured in this environment"
        });
    };

    // A failed lookup answers `null`, never zero. Zero means "nobody is
    // available", which the Fleet agent plans around by declining the delivery;
    // null means "we could not find out", which it is told to treat as unknown.
    // Collapsing them turns a field-ops outage into a confident refusal to
    // take any orders at all.
    match supply.available_near(tenant_id, lat, lng, radius_km).await {
        Ok(n) => json!({ "available": n }),
        Err(e) => {
            tracing::warn!(err = %e, "courier supply lookup failed; answering unknown");
            json!({ "available": null, "note": "supply lookup failed" })
        }
    }
}

#[async_trait]
impl MeshCatalog for CatalogServiceAdapter {
    async fn search(
        &self,
        tenant_id: Uuid,
        vendor_id: Uuid,
        query: &str,
        avoid_allergens: &[String],
        limit: i64,
    ) -> anyhow::Result<serde_json::Value> {
        let hits = self
            .catalog
            .search(tenant_id, vendor_id, query, avoid_allergens, limit)
            .await?;

        Ok(json!({
            "items": hits.iter().map(|h| json!({
                "item_id":      h.item_with_availability.item.id,
                "name":         h.item_with_availability.item.name,
                "price_cents":  h.item_with_availability.item.price_cents,
                "allergens":    h.item_with_availability.item.allergens,
                "availability": h.item_with_availability.availability.state.as_str(),
                // The freshness judgement, already applied with the configured
                // window — the agent reasons over the verdict, not the clock.
                "warrants_substitute": h.warrants_substitute,
            })).collect::<Vec<_>>()
        }))
    }

    async fn vendors_near(
        &self,
        tenant_id: Uuid,
        vertical: &str,
        lat: f64,
        lng: f64,
        radius_km: f64,
        limit: i64,
    ) -> anyhow::Result<serde_json::Value> {
        let vertical = parse_vertical(vertical)?;
        let vendors = self
            .catalog
            .vendors_near(tenant_id, vertical, lat, lng, radius_km, limit)
            .await?;

        Ok(json!({
            "vendors": vendors.iter().map(|v| json!({
                "vendor_id":         v.id,
                "name":              v.name,
                "address":           v.address,
                "prep_time_minutes": v.prep_time_minutes,
            })).collect::<Vec<_>>()
        }))
    }

    async fn resolve_facts(
        &self,
        tenant_id: Uuid,
        item_ids: &[Uuid],
    ) -> anyhow::Result<Vec<omnideliv_mesh::ItemFacts>> {
        let facts = self.catalog.item_facts(tenant_id, item_ids).await?;
        Ok(facts
            .into_iter()
            .map(|f| omnideliv_mesh::ItemFacts {
                item_id:           f.item_id,
                allergens:         f.allergens,
                vertical:          f.vertical,
                prep_time_minutes: f.prep_time_minutes,
                price_cents:       f.price_cents,
            })
            .collect())
    }

    async fn courier_supply(
        &self,
        tenant_id: Uuid,
        lat: f64,
        lng: f64,
        radius_km: f64,
    ) -> anyhow::Result<serde_json::Value> {
        Ok(supply_json(self.supply.as_ref(), tenant_id, lat, lng, radius_km).await)
    }
}

#[cfg(test)]
mod supply_tests {
    use super::*;

    struct Counting(usize);
    #[async_trait]
    impl CourierSupply for Counting {
        async fn available_near(&self, _: Uuid, _: f64, _: f64, _: f64) -> anyhow::Result<usize> {
            Ok(self.0)
        }
    }

    struct Broken;
    #[async_trait]
    impl CourierSupply for Broken {
        async fn available_near(&self, _: Uuid, _: f64, _: f64, _: f64) -> anyhow::Result<usize> {
            anyhow::bail!("field-ops is down")
        }
    }

    /// Zero is a real answer — nobody is free — and the Fleet agent is expected
    /// to plan around it by declining. It must be a number, not null.
    #[tokio::test]
    async fn no_couriers_available_is_zero_not_null() {
        let s: Arc<dyn CourierSupply> = Arc::new(Counting(0));
        let v = supply_json(Some(&s), Uuid::new_v4(), 0.0, 0.0, 5.0).await;
        assert_eq!(v["available"], serde_json::json!(0));
        assert!(!v["available"].is_null());
    }

    #[tokio::test]
    async fn a_real_count_is_returned() {
        let s: Arc<dyn CourierSupply> = Arc::new(Counting(7));
        let v = supply_json(Some(&s), Uuid::new_v4(), 0.0, 0.0, 5.0).await;
        assert_eq!(v["available"], serde_json::json!(7));
    }

    /// A failed lookup must NOT collapse to zero. Zero means "decline the
    /// order"; null means "unknown". Conflating them turns a field-ops outage
    /// into a confident refusal to take any orders at all.
    #[tokio::test]
    async fn a_failed_lookup_answers_null_not_zero() {
        let s: Arc<dyn CourierSupply> = Arc::new(Broken);
        let v = supply_json(Some(&s), Uuid::new_v4(), 0.0, 0.0, 5.0).await;
        assert!(v["available"].is_null(), "got {v}");
    }

    /// Same rule when supply was never wired at all.
    #[tokio::test]
    async fn an_unconfigured_lookup_answers_null() {
        let v = supply_json(None, Uuid::new_v4(), 0.0, 0.0, 5.0).await;
        assert!(v["available"].is_null());
    }
}
