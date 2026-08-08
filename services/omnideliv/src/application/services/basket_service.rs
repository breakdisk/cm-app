use std::sync::Arc;

use uuid::Uuid;

use crate::domain::entities::{
    Basket, BasketDelta, BasketLine, SubIntent, SubIntentSource, SubIntentStatus, Vertical,
};
use crate::domain::repositories::{BasketRepository, CatalogRepository, VendorRepository};

pub struct BasketService {
    baskets: Arc<dyn BasketRepository>,
    vendors: Arc<dyn VendorRepository>,
    catalog: Arc<dyn CatalogRepository>,
}

impl BasketService {
    pub fn new(
        baskets: Arc<dyn BasketRepository>,
        vendors: Arc<dyn VendorRepository>,
        catalog: Arc<dyn CatalogRepository>,
    ) -> Self {
        Self { baskets, vendors, catalog }
    }

    /// Record what a mesh run's verification found. Not a basket mutation:
    /// it does not go through the optimistic lock, because the run is
    /// describing lines it just wrote rather than changing them.
    pub async fn record_conflicts(
        &self,
        tenant_id: Uuid,
        basket_id: Uuid,
        conflicts: &[crate::domain::entities::BasketConflict],
    ) -> anyhow::Result<()> {
        self.baskets.set_conflicts(tenant_id, basket_id, conflicts).await
    }

    pub async fn create(&self, tenant_id: Uuid, customer_id: Uuid) -> anyhow::Result<Basket> {
        let b = Basket::new(tenant_id, customer_id);
        self.baskets.save(&b).await?;
        Ok(b)
    }

    /// A basket a mesh run is about to fill, linked to the run that made it.
    pub async fn create_for_mesh(
        &self,
        tenant_id: Uuid,
        customer_id: Uuid,
        mesh_session_id: Uuid,
    ) -> anyhow::Result<Basket> {
        let mut b = Basket::new(tenant_id, customer_id);
        b.mesh_session_id = Some(mesh_session_id);
        self.baskets.save(&b).await?;
        Ok(b)
    }

    pub async fn get(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<Option<Basket>> {
        self.baskets.find_by_id(tenant_id, id).await
    }

    /// Read-modify-write with a bounded retry.
    ///
    /// A conflict here is an ordinary double-tap, not a fault — retrying once
    /// against fresh state resolves it. Retrying unboundedly would turn a hot
    /// basket into a livelock, so three attempts then surface the error.
    ///
    /// Every mutating method routes through here. That is what makes the
    /// optimistic lock actually cover the basket rather than covering whichever
    /// paths remembered to check it.
    async fn mutate<F>(&self, tenant_id: Uuid, basket_id: Uuid, mut f: F) -> anyhow::Result<Basket>
    where
        F: FnMut(&mut Basket),
    {
        for attempt in 0..3 {
            let mut basket = self
                .baskets
                .find_by_id(tenant_id, basket_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("basket {basket_id} not found"))?;

            f(&mut basket);

            match self.baskets.save(&basket).await {
                Ok(()) => return Ok(basket),
                Err(e) if attempt < 2 => {
                    tracing::warn!(%basket_id, attempt, err = %e, "basket write conflict, retrying");
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
        unreachable!("loop returns on the final attempt")
    }

    /// Apply a specialist's delta. Replace semantics — a retrying specialist
    /// cannot double the basket.
    pub async fn apply_delta(
        &self,
        tenant_id: Uuid,
        basket_id: Uuid,
        delta: BasketDelta,
    ) -> anyhow::Result<Basket> {
        let mut delta = Some(delta);
        self.mutate(tenant_id, basket_id, move |b| {
            if let Some(d) = delta.take() {
                b.apply(d);
            }
        })
        .await
    }

    /// Persist a mesh sub-intent and its lines together.
    ///
    /// The sub-intent row must exist before lines can reference it
    /// (`basket_lines.sub_intent_id` is a NOT NULL foreign key), and both must
    /// land in the same versioned write or a crash between them leaves an
    /// orphaned partition.
    pub async fn apply_mesh_delta<F>(
        &self,
        tenant_id: Uuid,
        basket_id: Uuid,
        sub_intent_id: Uuid,
        vertical: Vertical,
        raw_text: &str,
        build: F,
    ) -> anyhow::Result<Basket>
    where
        F: Fn(&Basket) -> BasketDelta,
    {
        let raw_text = raw_text.to_string();
        self.mutate(tenant_id, basket_id, move |b| {
            if !b.sub_intents.iter().any(|s| s.id == sub_intent_id) {
                b.sub_intents.push(SubIntent {
                    id: sub_intent_id,
                    basket_id: b.id,
                    tenant_id,
                    vertical,
                    vendor_hint: None,
                    raw_text: raw_text.clone(),
                    constraints: serde_json::json!({}),
                    status: SubIntentStatus::Satisfied,
                    source: SubIntentSource::Mesh,
                    created_at: chrono::Utc::now(),
                });
            }
            let delta = build(b);
            b.apply(delta);
        })
        .await
    }

    /// Add a catalog item to a basket.
    ///
    /// Price and vertical come from the catalog, not from the caller — the
    /// client supplies only *what* and *how many*. Taking a price from the
    /// request would let a customer name their own, and taking the vertical
    /// would let them file a restaurant order into the grocery partition.
    pub async fn add_item(
        &self,
        tenant_id: Uuid,
        basket_id: Uuid,
        vendor_id: Uuid,
        item_id: Uuid,
        qty: i32,
    ) -> anyhow::Result<Basket> {
        let vendor = self
            .vendors
            .find_by_id(tenant_id, vendor_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("vendor {vendor_id} not found"))?;

        if !vendor.is_orderable() {
            anyhow::bail!("vendor {vendor_id} is not accepting orders");
        }

        let item = self
            .catalog
            .find_item(tenant_id, item_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("item {item_id} not found"))?;

        // Without this a caller could pair any item id with any vendor id and
        // have the line attributed — and paid out — to the wrong vendor.
        if item.vendor_id != vendor_id {
            anyhow::bail!("item {item_id} does not belong to vendor {vendor_id}");
        }

        self.mutate(tenant_id, basket_id, move |b| {
            let si = b.browse_sub_intent(vendor.vertical);
            b.add_line(BasketLine::propose(
                b.id, si, tenant_id, vendor_id, item_id, qty, item.price_cents, "browse",
            ));
        })
        .await
    }

    /// Remove a line. The bool reports whether anything was removed, so the API
    /// can answer 404 rather than reporting success for a no-op.
    pub async fn remove_item(
        &self,
        tenant_id: Uuid,
        basket_id: Uuid,
        line_id: Uuid,
    ) -> anyhow::Result<(Basket, bool)> {
        let mut removed = false;
        let basket = self
            .mutate(tenant_id, basket_id, |b| {
                removed = b.remove_line(line_id);
            })
            .await?;

        Ok((basket, removed))
    }
}
