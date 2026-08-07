//! Implements the mesh's MeshBasket port over BasketService.
//!
//! All writes go through `BasketService`, so the mesh inherits the optimistic
//! lock and bounded retry rather than opening a second write path into the
//! basket — which would put the customer and the agent back in a race.

use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use omnideliv_mesh::tools::MeshBasket;
use omnideliv_mesh::transition::ProposedLine;

use crate::application::services::BasketService;
use crate::domain::entities::{BasketConflict, BasketDelta, BasketLine, Vertical};

pub struct BasketServiceAdapter {
    baskets: Arc<BasketService>,
}

impl BasketServiceAdapter {
    pub fn new(baskets: Arc<BasketService>) -> Self { Self { baskets } }
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
impl MeshBasket for BasketServiceAdapter {
    async fn create(&self, tenant_id: Uuid, customer_id: Uuid) -> anyhow::Result<Uuid> {
        Ok(self.baskets.create(tenant_id, customer_id).await?.id)
    }

    async fn record_conflicts(
        &self,
        tenant_id: Uuid,
        basket_id: Uuid,
        conflicts: &[omnideliv_mesh::Conflict],
    ) -> anyhow::Result<()> {
        // `kind` crosses as opaque JSON. The mesh owns that enum and will grow
        // variants; re-encoding it into a product-side enum here would make
        // every new conflict kind a change in two crates.
        let rows: Vec<BasketConflict> = conflicts
            .iter()
            .map(|c| {
                Ok(BasketConflict {
                    kind:        serde_json::to_value(&c.kind)?,
                    blocking:    c.blocking,
                    description: c.description.clone(),
                })
            })
            .collect::<anyhow::Result<_>>()?;

        self.baskets.record_conflicts(tenant_id, basket_id, &rows).await
    }

    async fn write_delta(
        &self,
        tenant_id: Uuid,
        basket_id: Uuid,
        sub_intent_id: Uuid,
        vertical: &str,
        raw_text: &str,
        lines: Vec<ProposedLine>,
    ) -> anyhow::Result<()> {
        let vertical = parse_vertical(vertical)?;

        self.baskets
            .apply_mesh_delta(tenant_id, basket_id, sub_intent_id, vertical, raw_text, |basket| {
                BasketDelta {
                    sub_intent_id,
                    lines: lines
                        .iter()
                        .map(|l| BasketLine::propose(
                            basket.id, sub_intent_id, tenant_id,
                            l.vendor_id, l.item_id, l.qty, l.unit_price_cents, "mesh",
                        ))
                        .collect(),
                    note: None,
                }
            })
            .await?;

        Ok(())
    }

    async fn lines_awaiting_review(&self, tenant_id: Uuid, basket_id: Uuid) -> anyhow::Result<usize> {
        Ok(self
            .baskets
            .get(tenant_id, basket_id)
            .await?
            .map(|b| b.lines_awaiting_review().len())
            .unwrap_or(0))
    }
}
