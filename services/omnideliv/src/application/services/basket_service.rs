use std::sync::Arc;

use uuid::Uuid;

use crate::domain::entities::{
    Basket, BasketDelta, BasketLine, SubIntent, SubIntentSource, SubIntentStatus, Vertical,
};
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

    /// Add a line the customer picked by hand, into the browse partition for
    /// its vertical. Append semantics — see `Basket::add_line`.
    #[allow(clippy::too_many_arguments)]
    pub async fn add_line(
        &self,
        tenant_id: Uuid,
        basket_id: Uuid,
        vertical: Vertical,
        vendor_id: Uuid,
        item_id: Uuid,
        qty: i32,
        unit_price_cents: i64,
    ) -> anyhow::Result<Basket> {
        self.mutate(tenant_id, basket_id, move |b| {
            let sub_intent_id = b.browse_sub_intent(vertical);
            b.add_line(BasketLine::propose(
                b.id, sub_intent_id, tenant_id, vendor_id, item_id, qty, unit_price_cents, "browse",
            ));
        })
        .await
    }

    /// Remove a line. `Ok(None)` when the line was not in the basket, so the
    /// API can answer 404 rather than reporting success for a no-op.
    pub async fn remove_line(
        &self,
        tenant_id: Uuid,
        basket_id: Uuid,
        line_id: Uuid,
    ) -> anyhow::Result<Option<Basket>> {
        let mut removed = false;
        let basket = self
            .mutate(tenant_id, basket_id, |b| {
                removed = b.remove_line(line_id);
            })
            .await?;

        Ok(removed.then_some(basket))
    }
}
