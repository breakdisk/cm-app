//! Implements the mesh's `MeshCatalog` port over `CatalogService`.
//!
//! Returns plain JSON because that is what a tool result is. Shaping it here
//! rather than in the tool box keeps the mesh crate free of product types.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use uuid::Uuid;

use omnideliv_mesh::tools::MeshCatalog;

use crate::application::services::CatalogService;
use crate::domain::entities::Vertical;

pub struct CatalogServiceAdapter {
    catalog: Arc<CatalogService>,
}

impl CatalogServiceAdapter {
    pub fn new(catalog: Arc<CatalogService>) -> Self { Self { catalog } }
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
        _tenant_id: Uuid,
        _lat: f64,
        _lng: f64,
        _radius_km: f64,
    ) -> anyhow::Result<serde_json::Value> {
        // Courier supply lives in field-ops. Wiring the cross-service call is
        // Plan 11's gateway work; until then the Fleet agent sees an explicit
        // null rather than a fabricated count it would confidently plan against.
        Ok(json!({ "available": null, "note": "courier supply lookup not yet wired to field-ops" }))
    }
}
