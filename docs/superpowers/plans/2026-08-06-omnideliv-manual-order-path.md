# OmniDeliv Manual Order Path Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a customer browse, build a basket and place an order **with no LLM in the path at all** — closing a gap that Plans 3 and 7 claimed to cover and did not.

**Architecture:** Manual lines enter the basket through a *browse sub-intent* — a synthetic sub-intent per vertical — so every existing invariant survives untouched: `Basket::apply` still partitions by sub-intent, the substitution loop still works, and `CheckoutService` needs no change at all. A new `Basket::add_line` provides append semantics alongside `apply`'s replace semantics. Because a customer can now write to a basket, the optimistic lock Plan 3 deferred becomes required.

**Tech Stack:** Rust 2021, Axum, SQLx (backend); Expo / React Native (app).

---

## Why this plan exists

Plans 3 and 7 both assert that the Quick Intent Pills are the non-AI fallback and that "if the mesh is down, this is still a working app." Tracing it end to end shows that is false:

| Layer | What was built | What is missing |
|---|---|---|
| App | `app/browse/[vertical].tsx` renders vendors in a plain `<View>` | No `Pressable`, no navigation — the vendor list is a dead end |
| API | `POST /v1/baskets`, `GET /v1/baskets/:id` | Nothing adds a line |
| Domain | `Basket::apply(BasketDelta)` | A delta needs a `sub_intent_id`, and only the Concierge creates sub-intents |

The sole producer of basket lines across the whole plan set is the LLM. The pills prove the app does not crash; they do not let anyone order.

This also breaks an explicit platform rule: *"AI features are additive enhancements — all operations must have a non-AI fallback."* This plan makes that true rather than aspirational.

**What does not need changing:** `CheckoutService::place` works on any basket with lines and never touches mesh code. Once lines exist, the rest of the chain — consolidation, settlement, dispatch, tracking — already works.

---

## Dependencies

**Requires Plan 3** (catalog, basket, `apply`). **Requires Plan 7** (the app shell and browse screen this extends). Plan 4 (the mesh) is deliberately **not** required — the point is that this path works without it.

Verify:

```bash
CARGO_INCREMENTAL=0 cargo check -p logisticos-omnideliv && cd apps/omnideliv-app && npx tsc --noEmit
```

---

## Task 1: The browse sub-intent

**Files:**
- Create: `services/omnideliv/migrations/0008_sub_intent_source.sql`
- Modify: `services/omnideliv/src/domain/entities/basket.rs`

- [ ] **Step 1: Write the migration**

An `ALTER` rather than an edit to Plan 3's migration 0003, so this applies cleanly whether or not 0003 has already run against a live database.

```sql
-- Where a sub-intent came from. `mesh` is the Concierge's decomposition;
-- `browse` is the synthetic sub-intent that carries manually-added lines when
-- the customer is shopping without the agent.
--
-- Manual lines need a sub-intent because basket_lines.sub_intent_id is NOT NULL
-- and is the partition key Basket::apply scopes by. Giving browsing its own
-- sub-intent keeps that partitioning intact rather than making the column
-- nullable, which would weaken the single-writer guarantee for everyone.
ALTER TABLE omnideliv.sub_intents
    ADD COLUMN IF NOT EXISTS source TEXT NOT NULL DEFAULT 'mesh'
        CHECK (source IN ('mesh', 'browse'));

-- One browse sub-intent per vertical per basket — the find-or-create in
-- Basket::browse_sub_intent relies on this being enforced, not merely intended.
CREATE UNIQUE INDEX IF NOT EXISTS uq_browse_sub_intent
    ON omnideliv.sub_intents (basket_id, vertical)
    WHERE source = 'browse';
```

- [ ] **Step 2: Write the failing test**

```rust
// services/omnideliv/src/domain/entities/basket.rs — append to the tests block
    #[test]
    fn a_browse_sub_intent_is_created_on_first_use() {
        let mut b = basket();
        let id = b.browse_sub_intent(Vertical::Grocery);

        assert_eq!(b.sub_intents.len(), 1);
        assert_eq!(b.sub_intents[0].id, id);
        assert_eq!(b.sub_intents[0].source, SubIntentSource::Browse);
        assert_eq!(b.sub_intents[0].vertical, Vertical::Grocery);
    }

    /// Find-or-create. Tapping "add" twice in the same vertical must not create
    /// a second partition, or `apply` would later wipe half the customer's cart.
    #[test]
    fn the_browse_sub_intent_is_reused_within_a_vertical() {
        let mut b = basket();
        let first  = b.browse_sub_intent(Vertical::Grocery);
        let second = b.browse_sub_intent(Vertical::Grocery);

        assert_eq!(first, second);
        assert_eq!(b.sub_intents.len(), 1);
    }

    #[test]
    fn each_vertical_gets_its_own_browse_sub_intent() {
        let mut b = basket();
        let grocery = b.browse_sub_intent(Vertical::Grocery);
        let food    = b.browse_sub_intent(Vertical::Restaurant);

        assert_ne!(grocery, food);
        assert_eq!(b.sub_intents.len(), 2);
    }

    /// A mesh sub-intent must never be mistaken for a browse one — otherwise a
    /// manual add would land inside a specialist's partition and be wiped the
    /// next time that specialist proposes.
    #[test]
    fn a_mesh_sub_intent_is_never_reused_for_browsing() {
        let mut b = basket();
        b.sub_intents.push(SubIntent {
            id: Uuid::new_v4(),
            basket_id: b.id,
            tenant_id: b.tenant_id,
            vertical: Vertical::Grocery,
            vendor_hint: None,
            raw_text: "milk and eggs".into(),
            constraints: serde_json::json!({}),
            status: SubIntentStatus::Pending,
            source: SubIntentSource::Mesh,
            created_at: chrono::Utc::now(),
        });

        let browse = b.browse_sub_intent(Vertical::Grocery);

        assert_eq!(b.sub_intents.len(), 2, "browsing must get its own partition");
        assert_ne!(browse, b.sub_intents[0].id);
    }
```

- [ ] **Step 3: Run it to verify it fails**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv basket::`
Expected: FAIL to compile — `cannot find type 'SubIntentSource' in this scope`.

- [ ] **Step 4: Implement**

Add to `basket.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubIntentSource {
    /// Produced by the Concierge's decomposition.
    Mesh,
    /// The synthetic partition that carries manually-added lines.
    Browse,
}

impl SubIntentSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            SubIntentSource::Mesh   => "mesh",
            SubIntentSource::Browse => "browse",
        }
    }
}
```

Add `pub source: SubIntentSource,` to `SubIntent`, and on `Basket`:

```rust
    /// Find or create the browse partition for a vertical.
    ///
    /// Manual lines need a sub-intent because it is the key `apply` partitions
    /// by. Giving browsing its own — rather than reusing a mesh sub-intent or
    /// making the column nullable — means a specialist proposing later cannot
    /// wipe what the customer added by hand, and vice versa.
    pub fn browse_sub_intent(&mut self, vertical: Vertical) -> Uuid {
        if let Some(existing) = self
            .sub_intents
            .iter()
            .find(|s| s.source == SubIntentSource::Browse && s.vertical == vertical)
        {
            return existing.id;
        }

        let si = SubIntent {
            id: Uuid::new_v4(),
            basket_id: self.id,
            tenant_id: self.tenant_id,
            vertical,
            vendor_hint: None,
            raw_text: String::new(),
            constraints: serde_json::json!({}),
            status: SubIntentStatus::Satisfied,
            source: SubIntentSource::Browse,
            created_at: Utc::now(),
        };
        let id = si.id;
        self.sub_intents.push(si);
        self.updated_at = Utc::now();
        id
    }
```

Update the repository's sub-intent read and write to carry `source`, and export `SubIntentSource` from `entities/mod.rs`.

- [ ] **Step 5: Run the tests**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv basket::`
Expected: PASS — 10 passed (6 from Plan 3 plus 4 new).

- [ ] **Step 6: Commit**

```bash
git add services/omnideliv/
git commit -m "feat(omnideliv): browse sub-intent so manual lines have a partition

basket_lines.sub_intent_id is the key Basket::apply partitions by. Giving
browsing its own sub-intent — rather than making the column nullable — means a
specialist proposing later cannot wipe what the customer added by hand."
```

---

## Task 2: `add_line` — append, not replace

`apply` replaces a sub-intent's lines so a retrying specialist cannot double the basket. A customer adding an item needs the opposite. These are genuinely different operations and get different methods.

**Files:**
- Modify: `services/omnideliv/src/domain/entities/basket.rs`

- [ ] **Step 1: Write the failing test**

```rust
    #[test]
    fn add_line_appends_rather_than_replacing() {
        let mut b = basket();
        let si = b.browse_sub_intent(Vertical::Grocery);

        b.add_line(line(b.id, si, 10_000, 1));
        b.add_line(line(b.id, si, 15_000, 1));

        assert_eq!(b.lines.len(), 2, "a second add must not replace the first");
        assert_eq!(b.goods_total_cents(), 25_000);
    }

    /// Standard cart behaviour: adding the same item again bumps quantity
    /// rather than creating a duplicate row the customer then has to remove twice.
    #[test]
    fn adding_the_same_item_again_increments_quantity() {
        let mut b = basket();
        let si = b.browse_sub_intent(Vertical::Grocery);
        let item = Uuid::new_v4();
        let vendor = Uuid::new_v4();

        // Capture the ids by value, not the basket: a closure borrowing `b`
        // would still hold that borrow when `add_line` needs `&mut b`.
        let (bid, tid) = (b.id, b.tenant_id);
        let mk = || BasketLine::propose(bid, si, tid, vendor, item, 1, 12_000, "browse");
        b.add_line(mk());
        b.add_line(mk());

        assert_eq!(b.lines.len(), 1, "same item merges");
        assert_eq!(b.lines[0].qty, 2);
        assert_eq!(b.goods_total_cents(), 24_000);
    }

    /// The same item at two different vendors is two lines — the customer chose
    /// each one, and merging them would silently move an order between vendors.
    #[test]
    fn the_same_item_at_different_vendors_stays_separate() {
        let mut b = basket();
        let si = b.browse_sub_intent(Vertical::Grocery);
        let item = Uuid::new_v4();

        b.add_line(BasketLine::propose(b.id, si, b.tenant_id, Uuid::new_v4(), item, 1, 12_000, "browse"));
        b.add_line(BasketLine::propose(b.id, si, b.tenant_id, Uuid::new_v4(), item, 1, 12_000, "browse"));

        assert_eq!(b.lines.len(), 2);
    }

    #[test]
    fn removing_a_line_drops_it_from_the_total() {
        let mut b = basket();
        let si = b.browse_sub_intent(Vertical::Grocery);
        let l = line(b.id, si, 9_000, 1);
        let id = l.id;
        b.add_line(l);

        assert!(b.remove_line(id));
        assert!(b.lines.is_empty());
        assert_eq!(b.goods_total_cents(), 0);
    }

    #[test]
    fn removing_a_line_that_is_not_there_reports_false() {
        let mut b = basket();
        assert!(!b.remove_line(Uuid::new_v4()));
    }

    /// The invariant that matters: a manual line and a mesh proposal coexist,
    /// and a specialist re-proposing does not touch the browse partition.
    #[test]
    fn a_specialist_reproposing_leaves_manual_lines_alone() {
        let mut b = basket();
        let browse = b.browse_sub_intent(Vertical::Grocery);
        b.add_line(line(b.id, browse, 8_000, 1));

        let mesh_si = Uuid::new_v4();
        b.apply(BasketDelta { sub_intent_id: mesh_si, lines: vec![line(b.id, mesh_si, 30_000, 1)], note: None });
        b.apply(BasketDelta { sub_intent_id: mesh_si, lines: vec![line(b.id, mesh_si, 32_000, 1)], note: None });

        assert_eq!(b.lines.len(), 2, "the manual line survives both proposals");
        assert_eq!(b.goods_total_cents(), 8_000 + 32_000);
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv basket::add_line`
Expected: FAIL to compile — `no method named 'add_line'`.

- [ ] **Step 3: Implement**

```rust
    /// Append a line the customer added by hand.
    ///
    /// Deliberately *not* `apply`. `apply` replaces a sub-intent's lines so a
    /// retrying specialist cannot double the basket; a customer tapping "add"
    /// needs the opposite. Two operations, two methods — collapsing them would
    /// mean either losing manual adds or letting a retry duplicate a proposal.
    ///
    /// The same item at the same vendor merges into one line with a bumped
    /// quantity. Different vendors stay separate: the customer chose each, and
    /// merging would silently move part of an order to another vendor.
    pub fn add_line(&mut self, line: BasketLine) {
        if let Some(existing) = self.lines.iter_mut().find(|l| {
            l.sub_intent_id == line.sub_intent_id
                && l.item_id == line.item_id
                && l.vendor_id == line.vendor_id
                && l.state != LineState::Rejected
        }) {
            existing.qty += line.qty;
        } else {
            self.lines.push(line);
        }
        self.updated_at = Utc::now();
    }

    /// Remove a line. Returns whether anything was removed, so the API can
    /// answer 404 rather than reporting success for a line that never existed.
    pub fn remove_line(&mut self, line_id: Uuid) -> bool {
        let before = self.lines.len();
        self.lines.retain(|l| l.id != line_id);
        let removed = self.lines.len() != before;
        if removed {
            self.updated_at = Utc::now();
        }
        removed
    }
```

- [ ] **Step 4: Run the tests**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv basket::`
Expected: PASS — 16 passed.

- [ ] **Step 5: Commit**

```bash
git add services/omnideliv/src/domain/entities/basket.rs
git commit -m "feat(omnideliv): Basket::add_line with append semantics

apply replaces so a retrying specialist cannot double the basket; add_line
appends because a customer tapping add needs the opposite. Two operations, two
methods — collapsing them would mean either losing manual adds or letting a
retry duplicate a proposal."
```

---

## Task 3: Optimistic locking

Plan 3 deferred this on the grounds that the mesh is a single writer. That reasoning no longer holds: a customer can now write too, and a double-tap on "add" is a lost update.

**Files:**
- Create: `services/omnideliv/migrations/0009_basket_version.sql`
- Modify: `src/domain/entities/basket.rs`, `src/infrastructure/db/basket_repo.rs`, `src/application/services/basket_service.rs`

- [ ] **Step 1: Write the migration**

```sql
-- Optimistic lock. Plan 3 deferred this because the mesh was the only writer;
-- once a customer can add lines from the app, a double-tap is a lost update.
ALTER TABLE omnideliv.baskets
    ADD COLUMN IF NOT EXISTS version BIGINT NOT NULL DEFAULT 0;
```

- [ ] **Step 2: Write the failing test**

```rust
// services/omnideliv/src/domain/entities/basket.rs
    #[test]
    fn a_new_basket_starts_at_version_zero() {
        assert_eq!(basket().version, 0);
    }

    #[test]
    fn every_mutation_bumps_the_version() {
        let mut b = basket();
        let si = b.browse_sub_intent(Vertical::Grocery);
        assert_eq!(b.version, 1, "creating the browse partition is a mutation");

        b.add_line(line(b.id, si, 1_000, 1));
        assert_eq!(b.version, 2);

        b.apply(BasketDelta { sub_intent_id: si, lines: vec![], note: None });
        assert_eq!(b.version, 3);
    }
```

- [ ] **Step 3: Run it to verify it fails**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv basket::version`
Expected: FAIL to compile — `no field 'version' on type 'Basket'`.

- [ ] **Step 4: Implement**

Add `pub version: i64,` to `Basket`, initialised to `0` in `new`. Replace the three `self.updated_at = Utc::now();` assignments in `apply`, `add_line`, `remove_line` and `browse_sub_intent` with a single helper so no future mutation can forget it:

```rust
    /// Every mutation goes through here, so a new one cannot silently skip the
    /// version bump and reopen the lost-update window.
    fn touch(&mut self) {
        self.version += 1;
        self.updated_at = Utc::now();
    }
```

In `PgBasketRepository::save`, make the basket UPDATE conditional and detect the conflict:

```rust
        let result = sqlx::query(
            r#"
            INSERT INTO omnideliv.baskets (id, tenant_id, customer_id, status, mesh_session_id, version, created_at, updated_at)
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
            ON CONFLICT (id) DO UPDATE SET
                status          = EXCLUDED.status,
                mesh_session_id = EXCLUDED.mesh_session_id,
                version         = EXCLUDED.version,
                updated_at      = EXCLUDED.updated_at
            WHERE omnideliv.baskets.version < EXCLUDED.version
            "#,
        )
        .bind(basket.id).bind(basket.tenant_id).bind(basket.customer_id)
        .bind(basket.status.as_str()).bind(basket.mesh_session_id)
        .bind(basket.version).bind(basket.created_at).bind(basket.updated_at)
        .execute(&mut *tx)
        .await?;

        // Zero rows means another writer got there first with an equal or
        // higher version. Roll back rather than writing lines against a basket
        // row we did not update — that would leave the two disagreeing.
        if result.rows_affected() == 0 {
            tx.rollback().await?;
            anyhow::bail!("basket {} was modified concurrently", basket.id);
        }
```

In `BasketService`, add a bounded retry so a genuine double-tap resolves rather than surfacing to the customer:

```rust
    /// Read-modify-write with a bounded retry.
    ///
    /// A conflict here is an ordinary double-tap, not a fault — retrying once
    /// against fresh state resolves it. Retrying unboundedly would turn a hot
    /// basket into a livelock, so three attempts then surface the error.
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
```

and route `apply_delta`, `add_line` and `remove_line` through it.

- [ ] **Step 5: Run the tests**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv`
Expected: PASS — 18 basket-related tests plus the rest of the suite.

- [ ] **Step 6: Commit**

```bash
git add services/omnideliv/
git commit -m "feat(omnideliv): optimistic lock on baskets

Plan 3 deferred this because the mesh was the only writer. A customer adding
lines from the app makes a double-tap a lost update, so the version column is
now required rather than optional. Every mutation bumps it through one helper
so a future one cannot silently skip it."
```

---

## Task 4: Line endpoints

**Files:**
- Modify: `services/omnideliv/src/api/http/baskets.rs`

- [ ] **Step 1: Write the routes**

```rust
#[derive(Debug, Deserialize)]
pub struct AddLineRequest {
    pub vendor_id: Uuid,
    pub item_id:   Uuid,
    #[serde(default = "one")]
    pub qty:       i32,
}

fn one() -> i32 { 1 }

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/baskets", post(create))
        .route("/v1/baskets/:id", get(fetch))
        .route("/v1/baskets/:id/lines", post(add_line))
        .route("/v1/baskets/:id/lines/:line_id", delete(remove_line))
}

async fn add_line(
    State(st): State<Arc<AppState>>,
    claims: Claims,
    Path(basket_id): Path<Uuid>,
    Json(req): Json<AddLineRequest>,
) -> Result<Json<BasketResponse>, (StatusCode, String)> {
    if req.qty < 1 {
        return Err((StatusCode::BAD_REQUEST, "qty must be at least 1".into()));
    }

    // Price and vertical are read server-side from the catalog, never taken
    // from the client. A client-supplied price is a client-supplied discount.
    let basket = st
        .baskets
        .add_item(claims.tenant_id, basket_id, req.vendor_id, req.item_id, req.qty)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "add line failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "could not add the item".into())
        })?;

    Ok(Json(BasketResponse::from(&basket)))
}

async fn remove_line(
    State(st): State<Arc<AppState>>,
    claims: Claims,
    Path((basket_id, line_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<BasketResponse>, (StatusCode, String)> {
    let (basket, removed) = st
        .baskets
        .remove_item(claims.tenant_id, basket_id, line_id)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "could not remove the item".into()))?;

    if !removed {
        return Err((StatusCode::NOT_FOUND, "no such line".into()));
    }

    Ok(Json(BasketResponse::from(&basket)))
}
```

- [ ] **Step 2: Write the service methods**

```rust
// BasketService
    /// Add a catalog item to a basket.
    ///
    /// Price and vertical come from the catalog, not from the caller — the
    /// client supplies only *what* and *how many*. Taking a price from the
    /// request would let a customer name their own.
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

        if item.vendor_id != vendor_id {
            anyhow::bail!("item {item_id} does not belong to vendor {vendor_id}");
        }

        self.mutate(tenant_id, basket_id, |b| {
            let si = b.browse_sub_intent(vendor.vertical);
            b.add_line(BasketLine::propose(
                b.id, si, tenant_id, vendor_id, item_id, qty, item.price_cents, "browse",
            ));
        })
        .await
    }

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
```

`CatalogRepository` needs one addition: `find_item(tenant_id, item_id) -> Option<CatalogItem>`, a single-row `SELECT` mirroring `list_for_vendor`.

- [ ] **Step 3: Verify and commit**

Run: `CARGO_INCREMENTAL=0 cargo check -p logisticos-omnideliv`
Expected: PASS.

```bash
git add services/omnideliv/
git commit -m "feat(omnideliv): basket line add/remove endpoints

Price and vertical are read server-side from the catalog; the client supplies
only what and how many. A client-supplied price is a client-supplied discount."
```

---

## Task 5: The app — vendor detail and basket

**Files:**
- Create: `apps/omnideliv-app/app/vendor/[vendorId].tsx`, `app/basket.tsx`, `src/api/basket.ts`, `src/hooks/useActiveBasket.ts`
- Modify: `app/browse/[vertical].tsx`

- [ ] **Step 1: Write the basket API and active-basket hook**

```ts
// apps/omnideliv-app/src/api/basket.ts
import { apiFetch } from "./client";

export interface BasketView {
  id: string;
  status: string;
  goods_total_cents: number;
  lines_awaiting_review: number;
}

export const createBasket = () =>
  apiFetch<BasketView>("/v1/baskets", { method: "POST", body: JSON.stringify({}) });

export const addLine = (basketId: string, vendorId: string, itemId: string, qty = 1) =>
  apiFetch<BasketView>(`/v1/baskets/${basketId}/lines`, {
    method: "POST",
    body: JSON.stringify({ vendor_id: vendorId, item_id: itemId, qty }),
  });

export const removeLine = (basketId: string, lineId: string) =>
  apiFetch<BasketView>(`/v1/baskets/${basketId}/lines/${lineId}`, { method: "DELETE" });

export const getBasket = (id: string) => apiFetch<BasketView>(`/v1/baskets/${id}`);
```

```ts
// apps/omnideliv-app/src/hooks/useActiveBasket.ts
/**
 * The basket the customer is currently filling.
 *
 * Persisted so a cart survives the app being backgrounded — a shopper who
 * loses their basket switching apps does not come back. SecureStore is used
 * because it is already a dependency; the id is not a secret, and a plain
 * key-value store would be equally correct.
 */
import { useCallback, useEffect, useState } from "react";
import * as SecureStore from "expo-secure-store";

import { createBasket } from "@/api/basket";

const KEY = "active_basket_id";

export function useActiveBasket() {
  const [basketId, setBasketId] = useState<string | null>(null);
  const [ready, setReady] = useState(false);

  useEffect(() => {
    SecureStore.getItemAsync(KEY)
      .then(setBasketId)
      .finally(() => setReady(true));
  }, []);

  /** Returns the current basket, creating one on first use. */
  const ensure = useCallback(async (): Promise<string> => {
    if (basketId) return basketId;
    const b = await createBasket();
    await SecureStore.setItemAsync(KEY, b.id);
    setBasketId(b.id);
    return b.id;
  }, [basketId]);

  const clear = useCallback(async () => {
    await SecureStore.deleteItemAsync(KEY);
    setBasketId(null);
  }, []);

  return { basketId, ready, ensure, clear };
}
```

- [ ] **Step 2: Make the vendor list navigable**

In `app/browse/[vertical].tsx`, wrap each row in a `Pressable`:

```tsx
        renderItem={({ item }) => (
          <Pressable
            accessibilityRole="button"
            accessibilityLabel={`${item.name}, about ${item.prep_time_minutes} minutes to prepare`}
            onPress={() => router.push(`/vendor/${item.id}`)}
            style={{
              backgroundColor: theme.surface, borderColor: theme.border, borderWidth: 1,
              borderRadius: theme.radius.md, padding: 13,
            }}
          >
            <Text style={{ color: theme.text, fontSize: 13, fontWeight: "600" }}>{item.name}</Text>
            <Text style={{ color: theme.muted, fontSize: 11 }}>
              ~{item.prep_time_minutes} min to prepare
            </Text>
          </Pressable>
        )}
```

adding `Pressable` to the import and `const router = useRouter();` to the component.

- [ ] **Step 3: Write the vendor detail screen**

```tsx
// apps/omnideliv-app/app/vendor/[vendorId].tsx
import { useEffect, useState } from "react";
import { FlatList, Pressable, Text, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { useLocalSearchParams, useRouter } from "expo-router";

import { apiFetch } from "@/api/client";
import { addLine } from "@/api/basket";
import { useActiveBasket } from "@/hooks/useActiveBasket";
import { theme } from "@/theme";

interface Item {
  item_id: string;
  name: string;
  price_cents: number;
  availability: "available" | "limited" | "out_of_stock";
}

export default function VendorDetail() {
  const { vendorId } = useLocalSearchParams<{ vendorId: string }>();
  const [items, setItems] = useState<Item[]>([]);
  const [count, setCount] = useState(0);
  const [busy, setBusy] = useState<string | null>(null);
  const { ensure } = useActiveBasket();
  const router = useRouter();

  useEffect(() => {
    if (vendorId) {
      apiFetch<Item[]>(`/v1/catalog/items?vendor_id=${vendorId}`).then(setItems).catch(() => setItems([]));
    }
  }, [vendorId]);

  async function add(item: Item) {
    if (busy) return;
    setBusy(item.item_id);
    try {
      const id = await ensure();
      await addLine(id, vendorId!, item.item_id);
      setCount((c) => c + 1);
    } catch {
      // Left deliberately quiet: the count simply does not move, so the
      // customer sees the add did not take without a modal interrupting them.
    } finally {
      setBusy(null);
    }
  }

  return (
    <SafeAreaView style={{ flex: 1, backgroundColor: theme.canvas }}>
      <FlatList
        contentContainerStyle={{ padding: 20, gap: 8, paddingBottom: 96 }}
        data={items}
        keyExtractor={(i) => i.item_id}
        ListEmptyComponent={
          <Text style={{ color: theme.faint, fontSize: 12 }}>Nothing on the menu right now.</Text>
        }
        renderItem={({ item }) => {
          const out = item.availability === "out_of_stock";
          return (
            <View
              style={{
                flexDirection: "row", alignItems: "center", justifyContent: "space-between",
                backgroundColor: theme.surface, borderColor: theme.border, borderWidth: 1,
                borderRadius: theme.radius.md, padding: 13, opacity: out ? 0.45 : 1,
              }}
            >
              <View style={{ flex: 1 }}>
                <Text style={{ color: theme.text, fontSize: 13, fontWeight: "600" }}>{item.name}</Text>
                <Text style={{ color: theme.muted, fontSize: 11 }}>
                  ₱{(item.price_cents / 100).toFixed(2)}
                  {item.availability === "limited" ? " · only a few left" : ""}
                  {out ? " · out of stock" : ""}
                </Text>
              </View>
              <Pressable
                accessibilityRole="button"
                accessibilityLabel={out ? `${item.name} is out of stock` : `Add ${item.name}`}
                disabled={out || busy === item.item_id}
                onPress={() => add(item)}
                style={{
                  backgroundColor: out ? "rgba(255,255,255,0.06)" : theme.cyan,
                  borderRadius: 999, paddingHorizontal: 14, paddingVertical: 7,
                }}
              >
                <Text style={{ color: out ? theme.faint : theme.canvas, fontWeight: "700", fontSize: 12 }}>
                  {busy === item.item_id ? "…" : "Add"}
                </Text>
              </Pressable>
            </View>
          );
        }}
      />

      {count > 0 && (
        <Pressable
          accessibilityRole="button"
          onPress={() => router.push("/basket")}
          style={{
            position: "absolute", left: 20, right: 20, bottom: 24,
            backgroundColor: theme.cyan, borderRadius: theme.radius.md, paddingVertical: 14,
          }}
        >
          <Text style={{ textAlign: "center", color: theme.canvas, fontWeight: "750", fontSize: 14 }}>
            View basket · {count} item{count === 1 ? "" : "s"}
          </Text>
        </Pressable>
      )}
    </SafeAreaView>
  );
}
```

- [ ] **Step 4: Write the basket screen**

```tsx
// apps/omnideliv-app/app/basket.tsx
import { useEffect, useState } from "react";
import { Pressable, Text, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { useRouter } from "expo-router";

import { getBasket, type BasketView } from "@/api/basket";
import { useActiveBasket } from "@/hooks/useActiveBasket";
import { theme } from "@/theme";

export default function BasketScreen() {
  const { basketId, ready } = useActiveBasket();
  const [basket, setBasket] = useState<BasketView | null>(null);
  const router = useRouter();

  useEffect(() => {
    if (basketId) getBasket(basketId).then(setBasket).catch(() => setBasket(null));
  }, [basketId]);

  if (!ready) return <Screen>Loading…</Screen>;
  if (!basketId || !basket) return <Screen>Your basket is empty.</Screen>;

  return (
    <SafeAreaView style={{ flex: 1, backgroundColor: theme.canvas, padding: 20, gap: 14 }}>
      <Text style={{ color: theme.text, fontSize: 18, fontWeight: "650" }}>Your basket</Text>

      <View
        style={{
          backgroundColor: theme.surface, borderColor: theme.border, borderWidth: 1,
          borderRadius: theme.radius.md, padding: 13,
        }}
      >
        <Text style={{ color: theme.text, fontSize: 14, fontWeight: "700" }}>
          ₱{(basket.goods_total_cents / 100).toFixed(2)}
        </Text>
      </View>

      <Pressable
        accessibilityRole="button"
        onPress={() => router.push({ pathname: "/review", params: { basketId: basket.id } })}
        style={{ backgroundColor: theme.cyan, borderRadius: theme.radius.md, paddingVertical: 14 }}
      >
        <Text style={{ textAlign: "center", color: theme.canvas, fontWeight: "750", fontSize: 14 }}>
          Continue to checkout
        </Text>
      </Pressable>
    </SafeAreaView>
  );
}

function Screen({ children }: { children: React.ReactNode }) {
  return (
    <SafeAreaView style={{ flex: 1, backgroundColor: theme.canvas, padding: 20 }}>
      <Text style={{ color: theme.faint }}>{children}</Text>
    </SafeAreaView>
  );
}
```

- [ ] **Step 5: Verify and commit**

```bash
cd apps/omnideliv-app && npx tsc --noEmit && npx jest
```

Expected: both pass.

```bash
git add apps/omnideliv-app/
git commit -m "feat(omnideliv-app): vendor detail and basket screens complete the manual path

Browse now navigates: vertical → vendor → items → basket → checkout, with no
model anywhere in the chain. The active basket is persisted so a cart survives
backgrounding."
```

---

## Task 6: Prove it with the mesh stopped

The test that would have caught this gap.

**Files:**
- Create: `services/omnideliv/tests/manual_order_path.rs`

- [ ] **Step 1: Write the test**

```rust
// services/omnideliv/tests/manual_order_path.rs
//! A complete order with no LLM anywhere in the path.
//!
//! This is the test whose absence let Plans 3 and 7 both claim a working
//! fallback that dead-ended at a vendor list. It touches no mesh code, and it
//! must keep passing with the Claude API key unset.

use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use logisticos_omnideliv::domain::entities::{
    Basket, BasketLine, CatalogItem, Vendor, Vertical,
};
use logisticos_omnideliv::domain::repositories::{BasketRepository, CatalogRepository, VendorRepository};
use logisticos_omnideliv::infrastructure::db::{
    PgBasketRepository, PgCatalogRepository, PgVendorRepository,
};

#[tokio::test]
async fn a_customer_can_build_and_check_out_a_basket_without_the_mesh() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    // Proving the point: no Claude credentials in this process.
    std::env::remove_var("CLAUDE_API_KEY");
    std::env::remove_var("ANTHROPIC_API_KEY");

    let pool = PgPoolOptions::new()
        .after_connect(|c, _| Box::pin(async move {
            sqlx::query("SET search_path TO omnideliv, public").execute(&mut *c).await?;
            Ok(())
        }))
        .connect(&url).await.expect("connect");

    logisticos_common::migrations::run(&pool, "omnideliv", &sqlx::migrate!("./migrations"))
        .await.expect("migrate");

    let tenant = Uuid::new_v4();
    let vendors = PgVendorRepository::new(pool.clone());
    let catalog = PgCatalogRepository::new(pool.clone());
    let baskets = PgBasketRepository::new(pool.clone());

    let mut vendor = Vendor::new(tenant, Vertical::Grocery, "Corner Store".into(),
                                 "1 Test St".into(), 14.6, 120.98);
    vendor.activate();
    vendors.save(&vendor).await.expect("save vendor");

    let now = chrono::Utc::now();
    let item = CatalogItem {
        id: Uuid::new_v4(), tenant_id: tenant, vendor_id: vendor.id,
        sku: "milk-1l".into(), name: "Milk 1L".into(), description: None,
        price_cents: 8_500, modifiers: serde_json::json!([]),
        allergens: vec![], dietary_tags: vec![], vertical_attrs: serde_json::json!({}),
        is_listed: true, created_at: now, updated_at: now,
    };
    catalog.save_item(&item).await.expect("save item");

    // Build the basket by hand — exactly what the app does.
    let mut basket = Basket::new(tenant, Uuid::new_v4());
    let si = basket.browse_sub_intent(Vertical::Grocery);
    basket.add_line(BasketLine::propose(
        basket.id, si, tenant, vendor.id, item.id, 2, item.price_cents, "browse",
    ));
    baskets.save(&basket).await.expect("save basket");

    let loaded = baskets.find_by_id(tenant, basket.id).await.expect("load").expect("exists");

    assert_eq!(loaded.lines.len(), 1);
    assert_eq!(loaded.goods_total_cents(), 17_000, "2 × ₱85.00");
    assert_eq!(
        loaded.lines_awaiting_review().len(),
        0,
        "a hand-built basket has nothing to review, so checkout is not blocked"
    );

    // Checkout's precondition is satisfied — it is reachable from here with no
    // mesh involvement. CheckoutService itself is covered by its own tests.
    assert!(!loaded.subtotals_by_vendor().is_empty());
}
```

- [ ] **Step 2: Run it**

```bash
DATABASE_URL="postgres://logisticos:logisticos@localhost:5432/svc_omnideliv" CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv --test manual_order_path
```

Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add services/omnideliv/tests/manual_order_path.rs
git commit -m "test(omnideliv): a complete order with no LLM in the path

The test whose absence let two plans claim a working fallback that dead-ended
at a vendor list. It unsets the Claude credentials and never touches mesh code."
```

---

## Definition of done

- [ ] `cargo test -p logisticos-omnideliv` — 18 basket tests plus the rest of the suite pass
- [ ] `cargo test -p logisticos-omnideliv --test manual_order_path` — passes
- [ ] `npx tsc --noEmit && npx jest` in `apps/omnideliv-app` — clean
- [ ] **With `services/omnideliv` running but the Claude API key unset**, a customer can tap a Quick Intent Pill, pick a vendor, add items, and place an order
- [ ] `rg -n "omnideliv-mesh" services/omnideliv/src/application/services/basket_service.rs` returns nothing — the manual path has no mesh dependency

## What this does not fix

- **Screen A's conversational input still requires the mesh.** That is correct: the input bar is the AI feature, the pills are the fallback. Degrading the text box to keyword search would be a worse experience than sending the customer to browse.
- **Substitutions are a mesh concept.** A manually-built basket never has lines awaiting review, so Screen C's substitution card is simply absent. Manual shoppers see stock state on the item row instead and choose for themselves.
