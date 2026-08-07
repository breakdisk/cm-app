# OmniDeliv Mesh Orchestration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Assemble the mesh. Plan 4 built the parts — roles, transitions, fan-out, reconcile — and no method that runs them. This adds `MeshRunner::run`, the phase that writes deltas into a basket, and the read endpoints three screens assume.

**Architecture:** One orchestration method driving phases 1–6 in order, emitting a `MeshEvent` at every observable step, and writing the reconciled result through `BasketService` — the single writer. Everything it calls already exists; this plan adds no new domain concepts.

---

## Why this plan exists

Plan 7's SSE route calls `mesh.run(utterance, tx)`. Plan 4 defines `MeshRunner::parse` and `MeshRunner::fan_out` and nothing else. There is no method that:

- runs the phases in order,
- turns `SpecialistResult`s into `BasketDelta`s and persists them,
- runs the Fleet phase over the merged basket,
- emits `Completed` or `Failed` so the app can navigate.

Plan 4's tests pass because they call `plan_workers` and `reconcile_results` directly. **No test in that plan exercises a full run**, which is how a missing entry point looked like a complete feature.

Four endpoints are also assumed in prose by Plans 6 and 7 without an owning task: `GET /v1/omnideliv/vendors`, `GET /v1/omnideliv/vendors/me`, `PATCH /v1/omnideliv/vendors/me`. (`GET /v1/omnideliv/orders/:id/track` belongs with the lifecycle work in Plan 10.)

---

## Dependencies

**Requires Plans 3, 4 and 8.** Verify:

```bash
CARGO_INCREMENTAL=0 cargo check -p omnideliv-mesh -p logisticos-omnideliv
```

---

## Task 1: The basket-writing port

The mesh must not depend on `services/omnideliv`'s concrete `BasketService` — that would invert the crate boundary Plan 4 established. It declares what it needs.

**Files:**
- Modify: `services/omnideliv/crates/mesh/src/tools.rs`

- [ ] **Step 1: Add the port**

```rust
// services/omnideliv/crates/mesh/src/tools.rs — alongside MeshCatalog

/// What the mesh needs in order to persist a run's result.
///
/// A trait rather than a dependency on the host service's `BasketService`:
/// the mesh crate is the split seam, and a concrete dependency across it would
/// make the later two-deployable split a refactor again.
#[async_trait]
pub trait MeshBasket: Send + Sync {
    /// Create the basket a run writes into.
    async fn create(&self, tenant_id: Uuid, customer_id: Uuid) -> anyhow::Result<Uuid>;

    /// Persist one specialist's lines. Scoped by sub-intent — this is the
    /// single-writer path, called serially by the Concierge after the join.
    async fn write_delta(
        &self,
        tenant_id: Uuid,
        basket_id: Uuid,
        sub_intent_id: Uuid,
        vertical: &str,
        raw_text: &str,
        lines: Vec<crate::transition::ProposedLine>,
    ) -> anyhow::Result<()>;

    /// How many lines still need a customer decision. Drives `needs_review`.
    async fn lines_awaiting_review(&self, tenant_id: Uuid, basket_id: Uuid) -> anyhow::Result<usize>;
}
```

- [ ] **Step 2: Commit**

```bash
git add services/omnideliv/crates/mesh/src/tools.rs
git commit -m "feat(mesh): MeshBasket port so the crate stays behind its seam"
```

---

## Task 2: `MeshRunner::run`

**Files:**
- Modify: `services/omnideliv/crates/mesh/src/runner.rs`

- [ ] **Step 1: Write the failing test**

The test Plan 4 should have had: a full run, stubbed Claude, asserting the event sequence and that a basket was written.

```rust
// services/omnideliv/crates/mesh/src/runner.rs — tests block
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingBasket {
        created: Mutex<Vec<Uuid>>,
        writes:  Mutex<Vec<(Uuid, usize)>>,
    }

    #[async_trait::async_trait]
    impl crate::tools::MeshBasket for RecordingBasket {
        async fn create(&self, _: Uuid, _: Uuid) -> anyhow::Result<Uuid> {
            let id = Uuid::new_v4();
            self.created.lock().unwrap().push(id);
            Ok(id)
        }
        async fn write_delta(
            &self, _: Uuid, basket_id: Uuid, _: Uuid, _: &str, _: &str,
            lines: Vec<ProposedLine>,
        ) -> anyhow::Result<()> {
            self.writes.lock().unwrap().push((basket_id, lines.len()));
            Ok(())
        }
        async fn lines_awaiting_review(&self, _: Uuid, _: Uuid) -> anyhow::Result<usize> { Ok(0) }
    }

    fn collect(rx: &mut mpsc::Receiver<MeshEvent>) -> Vec<MeshEvent> {
        let mut out = Vec::new();
        while let Ok(e) = rx.try_recv() { out.push(e); }
        out
    }

    /// A run with no parseable decomposition must fail loudly and terminally.
    /// Emitting `Completed` with an empty basket would send the customer to a
    /// checkout screen showing nothing, which reads as "we lost your order".
    #[tokio::test]
    async fn a_run_that_cannot_decompose_emits_failed_not_completed() {
        let (tx, mut rx) = mpsc::channel(32);
        let basket = Arc::new(RecordingBasket::default());
        let runner = MeshRunner::new(
            // The Concierge replies in prose without calling decompose_intent.
            Arc::new(StubClaude::new(vec![StubClaude::text("I'm not sure what you need.")])),
            Arc::new(crate::tools::MeshToolBox::new(Arc::new(NoopCatalog), Uuid::new_v4())),
            Arc::new(InMemoryStore::default()),
            basket.clone(),
            MeshConfig::default(),
        );

        runner.run(TenantId::from_uuid(Uuid::new_v4()), Uuid::new_v4(), "???".into(), tx).await;

        let events = collect(&mut rx);
        assert!(
            events.iter().any(|e| matches!(e, MeshEvent::Failed { .. })),
            "must emit Failed, got {events:?}"
        );
        assert!(
            !events.iter().any(|e| matches!(e, MeshEvent::Completed { .. })),
            "must not emit Completed"
        );
        assert!(basket.writes.lock().unwrap().is_empty(), "nothing should be written");
    }

    /// The event contract Screen B renders against: parsed, then one
    /// started/finished pair per worker, then completed.
    #[tokio::test]
    async fn a_run_emits_the_screen_b_event_sequence() {
        let (tx, mut rx) = mpsc::channel(64);
        let basket = Arc::new(RecordingBasket::default());

        let runner = MeshRunner::new(
            Arc::new(StubClaude::new(vec![
                // Phase 1 — the Concierge decomposes.
                StubClaude::tool_call("t1", "decompose_intent", serde_json::json!({
                    "sub_intents": [
                        {"vertical": "restaurant", "raw_text": "dinner", "constraints": {}},
                        {"vertical": "grocery",    "raw_text": "milk",   "constraints": {}}
                    ]
                })),
                StubClaude::text("split"),
                // Phase 2 — two specialists, each proposing once.
                StubClaude::tool_call("t2", "propose_lines", serde_json::json!({ "lines": [] })),
                StubClaude::text("done"),
                StubClaude::tool_call("t3", "propose_lines", serde_json::json!({ "lines": [] })),
                StubClaude::text("done"),
                // Phase 4 — Fleet plans.
                StubClaude::tool_call("t4", "plan_route", serde_json::json!({
                    "vendor_order": [], "flat_fee_cents": 4900, "total_minutes": 30
                })),
                StubClaude::text("planned"),
            ])),
            Arc::new(crate::tools::MeshToolBox::new(Arc::new(NoopCatalog), Uuid::new_v4())),
            Arc::new(InMemoryStore::default()),
            basket.clone(),
            MeshConfig::default(),
        );

        runner.run(TenantId::from_uuid(Uuid::new_v4()), Uuid::new_v4(), "dinner and milk".into(), tx).await;

        let events = collect(&mut rx);
        assert!(events.iter().any(|e| matches!(e, MeshEvent::IntentParsed { sub_intent_count: 2 })));
        assert_eq!(
            events.iter().filter(|e| matches!(e, MeshEvent::SpecialistStarted { .. })).count(), 2,
            "one card per sub-intent"
        );
        assert_eq!(
            events.iter().filter(|e| matches!(e, MeshEvent::SpecialistFinished { .. })).count(), 2
        );
        assert!(events.iter().any(|e| matches!(e, MeshEvent::Completed { .. })));
        assert_eq!(basket.created.lock().unwrap().len(), 1, "exactly one basket per run");
    }

    /// Emitting Completed before the last SpecialistFinished would let the app
    /// navigate away mid-run and lose the remaining cards.
    #[tokio::test]
    async fn completed_is_the_last_event() {
        let (tx, mut rx) = mpsc::channel(64);
        let runner = MeshRunner::new(
            Arc::new(StubClaude::new(vec![
                StubClaude::tool_call("t1", "decompose_intent", serde_json::json!({
                    "sub_intents": [{"vertical": "grocery", "raw_text": "milk", "constraints": {}}]
                })),
                StubClaude::text("split"),
                StubClaude::tool_call("t2", "propose_lines", serde_json::json!({ "lines": [] })),
                StubClaude::text("done"),
                StubClaude::tool_call("t3", "plan_route", serde_json::json!({
                    "vendor_order": [], "flat_fee_cents": 4900, "total_minutes": 20
                })),
                StubClaude::text("planned"),
            ])),
            Arc::new(crate::tools::MeshToolBox::new(Arc::new(NoopCatalog), Uuid::new_v4())),
            Arc::new(InMemoryStore::default()),
            Arc::new(RecordingBasket::default()),
            MeshConfig::default(),
        );

        runner.run(TenantId::from_uuid(Uuid::new_v4()), Uuid::new_v4(), "milk".into(), tx).await;

        let events = collect(&mut rx);
        let last = events.last().expect("at least one event");
        assert!(matches!(last, MeshEvent::Completed { .. }), "got {last:?}");
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `CARGO_INCREMENTAL=0 cargo test -p omnideliv-mesh runner::`
Expected: FAIL to compile — `MeshRunner::new` takes 4 arguments, 5 supplied; `no method named 'run'`.

- [ ] **Step 3: Implement**

Add `basket: Arc<dyn MeshBasket>` to `MeshRunner` and its constructor, then:

```rust
    /// The whole run, phases 1–6.
    ///
    /// Does not return a Result: every failure path is an emitted event, because
    /// the caller is an SSE stream whose only channel to the customer is the
    /// event sequence. A returned error would be invisible to the app.
    pub async fn run(
        &self,
        tenant_id: TenantId,
        customer_id: Uuid,
        utterance: String,
        events: mpsc::Sender<MeshEvent>,
    ) {
        // Phase 1 — parse.
        let (parent_id, specs) = match self.parse(tenant_id, utterance).await {
            Ok(v) => v,
            Err(e) => {
                let _ = events.send(MeshEvent::Failed { reason: format!("could not read that: {e}") }).await;
                return;
            }
        };

        if specs.is_empty() {
            // No parseable decomposition. Terminal and explicit — emitting
            // Completed with an empty basket would send the customer to a
            // checkout screen showing nothing, which reads as a lost order.
            let _ = events.send(MeshEvent::Failed {
                reason: "couldn't work out what you're after".into(),
            }).await;
            return;
        }

        let _ = events.send(MeshEvent::IntentParsed { sub_intent_count: specs.len() }).await;

        let workers = plan_workers(&specs);
        if workers.is_empty() {
            // Every vertical is one no slice-one specialist handles.
            let _ = events.send(MeshEvent::Failed {
                reason: "we can't help with that yet".into(),
            }).await;
            return;
        }

        // The basket exists before fan-out so a crash mid-run leaves something
        // the customer can reopen rather than losing the work entirely.
        let basket_id = match self.basket.create(tenant_id.inner(), customer_id).await {
            Ok(id) => id,
            Err(e) => {
                let _ = events.send(MeshEvent::Failed { reason: format!("could not start a basket: {e}") }).await;
                return;
            }
        };

        // Phase 2 — concurrent fan-out.
        let results = self.fan_out(tenant_id, parent_id, workers.clone(), events.clone()).await;

        // Phase 3 — reconcile. Single writer: results are merged and written
        // here, serially, never by the workers themselves.
        let outcome = reconcile_results(results);

        if outcome.total_failure {
            let _ = events.send(MeshEvent::Failed {
                reason: "couldn't reach any of the shops just now".into(),
            }).await;
            return;
        }

        let by_sub_intent: std::collections::HashMap<Uuid, &PlannedWorker> =
            workers.iter().map(|w| (w.sub_intent_id, w)).collect();

        for (sub_intent_id, lines) in outcome.lines {
            let Some(w) = by_sub_intent.get(&sub_intent_id) else { continue };
            if let Err(e) = self
                .basket
                .write_delta(
                    tenant_id.inner(), basket_id, sub_intent_id,
                    &w.vertical, &w.spec.raw_text, lines,
                )
                .await
            {
                // One vertical failing to persist degrades that vertical, not
                // the order — same rule as a specialist timing out.
                tracing::error!(err = %e, %sub_intent_id, "basket write failed");
                let _ = events.send(MeshEvent::SpecialistFinished {
                    sub_intent_id, lines_added: 0, degraded: true,
                    note: Some("couldn't save that part of your order".into()),
                }).await;
            }
        }

        // A basket spanning verticals with different handling is the constraint
        // Screen B surfaces. Derived here rather than asked of the model, because
        // it is a fact about the basket, not a judgement.
        let verticals: std::collections::HashSet<&str> =
            workers.iter().map(|w| w.vertical.as_str()).collect();
        if verticals.contains("restaurant") && verticals.len() > 1 {
            let _ = events.send(MeshEvent::ConstraintDetected {
                description: "Hot food and other items in one trip — we'll collect the hot food last."
                    .into(),
            }).await;
        }

        // Phase 4 — Fleet.
        match self.plan_route(tenant_id, parent_id).await {
            Ok(plan) => {
                let _ = events.send(MeshEvent::RoutePlanned {
                    stops: plan.vendor_order.len(),
                    flat_fee_cents: plan.flat_fee_cents,
                    total_minutes: plan.total_minutes,
                }).await;
            }
            Err(e) => {
                // Routing is not fatal: checkout re-plans from the basket, so a
                // failed preview only costs the customer the fee estimate.
                tracing::warn!(err = %e, "fleet planning failed; continuing without a preview");
            }
        }

        // Phases 5 and 6 belong to the customer: review on Screen C, commit on tap.
        let needs_review = self
            .basket
            .lines_awaiting_review(tenant_id.inner(), basket_id)
            .await
            .unwrap_or(0);

        let _ = events.send(MeshEvent::Completed { basket_id, needs_review }).await;
    }

    /// Phase 4. Runs the Fleet role once over the merged basket.
    async fn plan_route(&self, tenant_id: TenantId, parent_id: Uuid) -> anyhow::Result<RoutePlan> {
        let runner = AgentRunner::new(self.claude.clone(), self.tools.clone(), self.store.clone());
        let session = runner
            .run(
                tenant_id,
                roles::fleet(),
                serde_json::json!({ "parent_session_id": parent_id }),
                "Sequence the pickups for this basket and give me one flat fee.".into(),
            )
            .await?;

        session
            .actions
            .iter()
            .rev()
            .find(|a| a.tool_name == "plan_route" && a.succeeded)
            .and_then(|a| serde_json::from_value::<RoutePlan>(a.tool_input.clone()).ok())
            .ok_or_else(|| anyhow::anyhow!("fleet returned no parseable plan"))
    }
```

Import `RoutePlan` and `MeshBasket` at the top of `runner.rs`.

- [ ] **Step 4: Fix the constructor call Plan 4 already wrote**

Adding `basket` makes `MeshRunner::new` take five arguments, which breaks the one existing test that constructs it — `the_parent_session_is_recorded_before_any_specialist_runs` in Plan 4 Task 6. Add the recording double as its fifth argument:

```rust
        let runner = MeshRunner::new(
            Arc::new(StubClaude::new(vec![StubClaude::text("ok")])),
            Arc::new(crate::tools::MeshToolBox::new(Arc::new(NoopCatalog), Uuid::new_v4())),
            store.clone(),
            Arc::new(RecordingBasket::default()),
            MeshConfig::default(),
        );
```

Check for any other call site before moving on:

```bash
rg -n "MeshRunner::new" services/omnideliv
```

Expected: every hit passes five arguments.

- [ ] **Step 5: Run the tests**

Run: `CARGO_INCREMENTAL=0 cargo test -p omnideliv-mesh`
Expected: PASS — 19 tests (16 from Plan 4 plus 3 new).

- [ ] **Step 6: Commit**

```bash
git add services/omnideliv/crates/mesh/src/runner.rs
git commit -m "feat(mesh): MeshRunner::run — the orchestration Plan 4 omitted

Every failure path is an emitted event rather than a returned Result: the
caller is an SSE stream whose only channel to the customer is the event
sequence. A run that cannot decompose emits Failed, never Completed with an
empty basket — that would send the customer to a checkout screen showing
nothing, which reads as a lost order."
```

---

## Task 3: The host-side adapter — NEEDS PLAN 8 FIRST

> **Blocked, and the dependency is real rather than stylistic.** The adapter below
> calls `BasketService::mutate` and sets `SubIntentSource::Mesh`. Neither exists
> after Plans 3, 4 and 9 Tasks 1-2: `BasketService` has `create`, `get` and
> `apply_delta` only, and `SubIntent` has no `source` field. Both are built by
> [Plan 8](2026-08-06-omnideliv-manual-order-path.md), which also adds the
> optimistic lock this adapter relies on to avoid opening a second write path.
> Run Plan 8, then return here.
>
> Tasks 1 and 2 above are done and are what unblocked Plan 4 Task 7 —
> `MeshRunner::run` now exists.

**Files:**
- Create: `services/omnideliv/src/infrastructure/external/mesh_basket.rs`
- Modify: `src/application/services/basket_service.rs`, `src/bootstrap.rs`

- [ ] **Step 1: Write the adapter**

```rust
// services/omnideliv/src/infrastructure/external/mesh_basket.rs
//! Implements the mesh's MeshBasket port over BasketService.
//!
//! All writes go through `BasketService`, so the mesh inherits the optimistic
//! lock and retry Plan 8 added rather than opening a second write path.

use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use omnideliv_mesh::tools::MeshBasket;
use omnideliv_mesh::transition::ProposedLine;

use crate::application::services::BasketService;
use crate::domain::entities::{BasketDelta, BasketLine, Vertical};

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
```

- [ ] **Step 2: Add `apply_mesh_delta` to `BasketService`**

The mesh's sub-intents are real records, not the synthetic browse partition — they need creating before lines can reference them.

```rust
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
```

- [ ] **Step 3: Wire bootstrap**

Construct `BasketServiceAdapter` and pass it as the fifth argument to `MeshRunner::new`.

- [ ] **Step 4: Verify and commit**

Run: `CARGO_INCREMENTAL=0 cargo check --workspace`
Expected: PASS.

```bash
git add services/omnideliv/
git commit -m "feat(omnideliv): mesh basket adapter over BasketService

Mesh writes go through BasketService so they inherit the optimistic lock and
retry rather than opening a second write path. The sub-intent row and its
lines land in one versioned write, since a crash between them would leave an
orphaned partition."
```

---

## Task 4: The orphaned vendor endpoints — DONE

> Built 2026-08-07. `GET /v1/omnideliv/vendors` and `GET|PATCH /v1/omnideliv/vendors/me`.
> Two deviations from the code below: the extractor is `AuthClaims` with `.user_id`, not
> `Claims` with `.sub`; and `update_own_vendor` re-checks the status allowlist in the
> service rather than trusting the HTTP layer alone, so a future caller cannot bypass it.

**Files:**
- Create: `services/omnideliv/src/api/http/vendors.rs`
- Modify: `src/api/http/mod.rs`

- [ ] **Step 1: Write the routes**

```rust
// services/omnideliv/src/api/http/vendors.rs
//! Vendor read/write surface.
//!
//! `/me` resolves the vendor from the caller's claims — a vendor id in the path
//! would let any signed-in vendor read or edit another's store.

use std::sync::Arc;

use axum::{extract::{Query, State}, http::StatusCode, routing::get, Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use logisticos_auth::claims::Claims;

use crate::api::http::AppState;
use crate::domain::entities::Vertical;

#[derive(Debug, Deserialize)]
pub struct NearQuery {
    pub vertical: String,
    pub lat: f64,
    pub lng: f64,
    #[serde(default = "default_radius")]
    pub radius_km: f64,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_radius() -> f64 { 5.0 }
fn default_limit() -> i64 { 20 }

#[derive(Debug, Serialize)]
pub struct VendorSummary {
    pub id: Uuid,
    pub name: String,
    pub prep_time_minutes: i32,
}

#[derive(Debug, Serialize)]
pub struct VendorProfile {
    pub name: String,
    pub address: String,
    pub prep_time_minutes: i32,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct ProfilePatch {
    pub prep_time_minutes: Option<i32>,
    pub status: Option<String>,
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/omnideliv/vendors", get(list_near))
        .route("/v1/omnideliv/vendors/me", get(me).patch(patch_me))
}

async fn list_near(
    State(st): State<Arc<AppState>>,
    claims: Claims,
    Query(q): Query<NearQuery>,
) -> Result<Json<Vec<VendorSummary>>, StatusCode> {
    let vertical = match q.vertical.as_str() {
        "restaurant" => Vertical::Restaurant,
        "grocery"    => Vertical::Grocery,
        "pharmacy"   => Vertical::Pharmacy,
        "florist"    => Vertical::Florist,
        "retail"     => Vertical::Retail,
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    let vendors = st
        .catalog
        .vendors_near(claims.tenant_id, vertical, q.lat, q.lng, q.radius_km, q.limit)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "vendor lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(
        vendors.into_iter()
            .map(|v| VendorSummary { id: v.id, name: v.name, prep_time_minutes: v.prep_time_minutes })
            .collect(),
    ))
}

async fn me(
    State(st): State<Arc<AppState>>,
    claims: Claims,
) -> Result<Json<VendorProfile>, StatusCode> {
    let vendor = st
        .catalog
        .vendor_for_user(claims.tenant_id, claims.sub)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(VendorProfile {
        name: vendor.name,
        address: vendor.address,
        prep_time_minutes: vendor.prep_time_minutes,
        status: vendor.status.as_str().to_string(),
    }))
}

async fn patch_me(
    State(st): State<Arc<AppState>>,
    claims: Claims,
    Json(p): Json<ProfilePatch>,
) -> Result<StatusCode, (StatusCode, String)> {
    // A vendor may pause or resume itself. It may not offboard itself, or mark
    // itself active while still onboarding — those are Partner decisions.
    if let Some(s) = p.status.as_deref() {
        if !matches!(s, "active" | "paused") {
            return Err((StatusCode::FORBIDDEN, "that status is not yours to set".into()));
        }
    }
    if let Some(m) = p.prep_time_minutes {
        if !(0..=180).contains(&m) {
            return Err((StatusCode::BAD_REQUEST, "prep time must be 0–180 minutes".into()));
        }
    }

    st.catalog
        .update_own_vendor(claims.tenant_id, claims.sub, p.prep_time_minutes, p.status)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "vendor profile update failed");
            (StatusCode::INTERNAL_SERVER_ERROR, "could not save".into())
        })?;

    Ok(StatusCode::NO_CONTENT)
}
```

`CatalogService` needs `vendor_for_user` and `update_own_vendor`, backed by a `VendorRepository::find_by_user(tenant_id, user_id)` — which requires a `user_id` column on `omnideliv.vendors`. Add it:

```sql
-- services/omnideliv/migrations/0010_vendor_user.sql
-- Links a vendor to the identity user who signs into the console. Nullable:
-- slice one onboards vendors by hand, and one may exist before its login does.
ALTER TABLE omnideliv.vendors
    ADD COLUMN IF NOT EXISTS user_id UUID;

CREATE UNIQUE INDEX IF NOT EXISTS uq_vendor_user
    ON omnideliv.vendors (tenant_id, user_id)
    WHERE user_id IS NOT NULL;
```

- [ ] **Step 2: Verify and commit**

Run: `CARGO_INCREMENTAL=0 cargo check -p logisticos-omnideliv`
Expected: PASS.

```bash
git add services/omnideliv/
git commit -m "feat(omnideliv): vendor list and self-service profile endpoints

/me resolves from claims — a vendor id in the path would let any signed-in
vendor read or edit another's store. A vendor may pause and resume itself but
not offboard itself or clear its own onboarding; those stay Partner decisions."
```

---

## Task 5: Full-run integration test

**Files:**
- Create: `services/omnideliv/tests/mesh_run.rs`

- [ ] **Step 1: Write the test**

```rust
// services/omnideliv/tests/mesh_run.rs
//! A complete mesh run against a real database, with a stubbed Claude.
//!
//! The test whose absence let Plan 4 ship without its orchestration method:
//! every unit test there called plan_workers or reconcile_results directly, so
//! nothing noticed there was no way to run the phases in order.

use std::sync::Arc;

use sqlx::postgres::PgPoolOptions;
use tokio::sync::mpsc;
use uuid::Uuid;

use logisticos_agent_runtime::testing::{InMemoryStore, StubClaude};
use logisticos_types::TenantId;
use omnideliv_mesh::{events::MeshEvent, runner::MeshConfig, tools::MeshToolBox, MeshRunner};

use logisticos_omnideliv::application::services::BasketService;
use logisticos_omnideliv::infrastructure::db::PgBasketRepository;
use logisticos_omnideliv::infrastructure::external::mesh_basket::BasketServiceAdapter;

#[tokio::test]
async fn a_mesh_run_persists_a_basket_and_reports_completion() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let pool = PgPoolOptions::new()
        .after_connect(|c, _| Box::pin(async move {
            sqlx::query("SET search_path TO omnideliv, public").execute(&mut *c).await?;
            Ok(())
        }))
        .connect(&url).await.expect("connect");

    logisticos_common::migrations::run(&pool, "omnideliv", &sqlx::migrate!("./migrations"))
        .await.expect("migrate");

    let tenant = TenantId::from_uuid(Uuid::new_v4());
    let baskets = Arc::new(BasketService::new(Arc::new(PgBasketRepository::new(pool.clone()))));
    let adapter = Arc::new(BasketServiceAdapter::new(baskets.clone()));

    let runner = MeshRunner::new(
        Arc::new(StubClaude::new(vec![
            StubClaude::tool_call("t1", "decompose_intent", serde_json::json!({
                "sub_intents": [{"vertical": "grocery", "raw_text": "milk and eggs", "constraints": {}}]
            })),
            StubClaude::text("split"),
            StubClaude::tool_call("t2", "propose_lines", serde_json::json!({ "lines": [] })),
            StubClaude::text("nothing in stock"),
            StubClaude::tool_call("t3", "plan_route", serde_json::json!({
                "vendor_order": [], "flat_fee_cents": 4900, "total_minutes": 25
            })),
            StubClaude::text("planned"),
        ])),
        Arc::new(MeshToolBox::new(Arc::new(NoopCatalog), tenant.inner())),
        Arc::new(InMemoryStore::default()),
        adapter,
        MeshConfig::default(),
    );

    let (tx, mut rx) = mpsc::channel(64);
    runner.run(tenant, Uuid::new_v4(), "we're out of milk and eggs".into(), tx).await;

    let mut events = Vec::new();
    while let Ok(e) = rx.try_recv() { events.push(e); }

    let completed = events.iter().find_map(|e| match e {
        MeshEvent::Completed { basket_id, .. } => Some(*basket_id),
        _ => None,
    }).expect("the run must complete");

    // The basket must be readable afterwards — a Completed event pointing at a
    // basket the customer cannot load is worse than a clean failure.
    let loaded = baskets.get(tenant.inner(), completed).await.expect("load").expect("exists");
    assert_eq!(loaded.id, completed);
    assert_eq!(loaded.sub_intents.len(), 1, "the mesh sub-intent was persisted");
}

struct NoopCatalog;

#[async_trait::async_trait]
impl omnideliv_mesh::tools::MeshCatalog for NoopCatalog {
    async fn search(&self, _: Uuid, _: Uuid, _: &str, _: &[String], _: i64)
        -> anyhow::Result<serde_json::Value> { Ok(serde_json::json!({ "items": [] })) }
    async fn vendors_near(&self, _: Uuid, _: &str, _: f64, _: f64, _: f64, _: i64)
        -> anyhow::Result<serde_json::Value> { Ok(serde_json::json!({ "vendors": [] })) }
    async fn courier_supply(&self, _: Uuid, _: f64, _: f64, _: f64)
        -> anyhow::Result<serde_json::Value> { Ok(serde_json::json!({ "available": 4 })) }
}
```

- [ ] **Step 2: Run it**

```bash
DATABASE_URL="postgres://logisticos:logisticos@localhost:5432/svc_omnideliv" CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv --test mesh_run
```

Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add services/omnideliv/tests/mesh_run.rs
git commit -m "test(omnideliv): full mesh run against a real database

Asserts the basket a Completed event points at is actually loadable — a
completion pointing at a basket the customer cannot open is worse than a clean
failure."
```

---

## Definition of done

- [ ] `cargo test -p omnideliv-mesh` — 19 tests pass
- [ ] `cargo test -p logisticos-omnideliv --test mesh_run` — passes
- [ ] `cargo check --workspace` — clean
- [ ] `POST /v1/omnideliv/mesh/run` streams `intent_parsed`, two `specialist_started`, two `specialist_finished`, `completed` — in that order, with `completed` last
- [ ] `rg -n "mesh\.run\(" services/omnideliv/src/api/http/mesh.rs` resolves to a method that exists
