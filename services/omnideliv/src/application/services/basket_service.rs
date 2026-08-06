use std::sync::Arc;

use uuid::Uuid;

use crate::domain::entities::{Basket, BasketDelta};
use crate::domain::repositories::BasketRepository;

pub struct BasketService {
    baskets: Arc<dyn BasketRepository>,
}

impl BasketService {
    pub fn new(baskets: Arc<dyn BasketRepository>) -> Self { Self { baskets } }

    pub async fn create(&self, tenant_id: Uuid, customer_id: Uuid) -> anyhow::Result<Basket> {
        let b = Basket::new(tenant_id, customer_id);
        self.baskets.save(&b).await?;
        Ok(b)
    }

    pub async fn get(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<Option<Basket>> {
        self.baskets.find_by_id(tenant_id, id).await
    }

    /// Apply a specialist's delta.
    ///
    /// Read-modify-write is deliberate and safe here *because* the mesh has a
    /// single writer: only the Concierge calls this, serially, after joining its
    /// fan-out. If a second caller is ever added, this needs optimistic locking
    /// — a version column and a compare-and-swap — or deltas will be lost.
    pub async fn apply_delta(
        &self,
        tenant_id: Uuid,
        basket_id: Uuid,
        delta: BasketDelta,
    ) -> anyhow::Result<Basket> {
        let mut basket = self
            .baskets
            .find_by_id(tenant_id, basket_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("basket {basket_id} not found"))?;

        basket.apply(delta);
        self.baskets.save(&basket).await?;
        Ok(basket)
    }
}
