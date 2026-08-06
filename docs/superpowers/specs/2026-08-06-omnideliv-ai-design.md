# OmniDeliv AI — Hyperlocal Agentic Delivery Ecosystem

**Date:** 2026-08-06
**Status:** Design approved, ready for implementation planning
**Scope:** Vision umbrella + first buildable vertical slice

---

## 1. What this document is

The source specification for OmniDeliv AI describes a product vision spanning roughly nine
independently buildable subsystems. This document keeps that vision as the umbrella and specs
**one vertical slice deep enough to implement**: a single multi-merchant order flowing end to end
through the agent mesh.

Everything outside that slice is named in [§9 Scope boundaries](#9-scope-boundaries) so it stays
visible without being half-designed.

### 1.1 The hero flow

> "Dinner for two from Kuya's, and we're out of milk and eggs."

One utterance, two verticals (restaurant + grocery), two merchants, one courier, one flat delivery
fee. This flow was chosen over the source spec's sick-day example because it exercises every
mechanic in the architecture — intent decomposition, parallel specialists, live availability, the
substitution approval loop, prep-time-aware multi-stop batching, hot/cold constraints, dietary
profiles — **without** pulling regulated pharmacy into the first slice.

The substitution loop in particular only has teeth when groceries run out of stock. A
restaurant-only slice would leave Screens B and C with nothing real to render.

---

## 2. Decisions

Every decision below was made explicitly during design. They are recorded here because the
rationale matters more than the choice.

| # | Decision | Rationale |
|---|---|---|
| D1 | OmniDeliv is a **CargoMarket product tier**, not a standalone app | Reuses identity, wallet, engagement, CDP, courier GPS, POD and earnings. Backend in Rust/Axum per ADR-0001. Python remains only for the existing LangGraph sidecar. |
| D2 | Hero flow = **restaurant + grocery** | Proves the multi-vertical thesis with zero compliance blockers. Pharmacy follows in slice two against a proven mesh. |
| D3 | **Partner = Tenant**, white-label | Identical to LogisticOS. Reuses RLS, `tenant_branding` and the branding cascade wholesale. CargoMarket stays invisible to end customers per ADR-0009 rule 6. |
| D4 | **Three-leg settlement** | Customer pays goods + one flat fee + tip. Vendors credited goods minus commission. Courier paid per consolidated trip. Requires a new vendor payout ledger. |
| D5 | Mesh runs as a **Rust orchestrator with parallel fan-out** | Inherits `allowed_tools` RBAC, `AgentSession` audit and crash recovery. One runtime, one language, one audit trail. |
| D6 | **New Expo app** `apps/omnideliv-app` | Own brand and release cadence per ADR-0009 product isolation. Shares auth bridge, wallet and push registration as code, not by forking `customer-app`. |
| D7 | **Merchant-declared availability** with a freshness timestamp | Real POS integration is a quarter of bespoke connector work and would dominate the slice. Freshness lets the mesh reason honestly about uncertain stock. |
| D8 | **SSE** for agent state; existing push + polling for milestones | The orchestration window is seconds long and unidirectional. Avoids standing up a socket fleet, sticky sessions and mobile background-socket handling. |
| D9 | **Modular monolith** `services/omnideliv` | ADR-0009 rule 3: a service starts product-tier. Keeps the mesh's hot path in-process. Split seam to a two-service shape pre-identified. |
| D10 | **Extract `libs/agent-runtime`** from `services/ai-layer` | One agent loop, one RBAC gate, one audit shape across platform and product agents. Avoids two diverging runtimes. |
| D11 | **Minimal field-ops extraction** (new ADR-0015) | Resolves the ADR-0009 rule 2 conflict properly instead of with a documented exception that would become permanent. |
| D12 | **`Vendor`** in code, "merchant" in UI copy | LogisticOS `Merchant` pays the Partner; an OmniDeliv vendor receives money from the Partner. Opposite money flow, different lifecycle. |
| D13 | **Voice ships in slice one**, on-device STT only | Thin adapter over the same text pipeline; no server-side ASR, no custom vocabulary. Architectural cost is nil. |

---

## 3. Architecture

```
CLIENT      apps/omnideliv-app (Expo)          Vendor catalog console
            Screens A–D · SSE agent stream     Menus, SKUs, availability, prep times
                          │
GATEWAY     omnideliv.api.cargomarket.net      api.cargomarket.net
            (same Rust binary, new routing)     (platform: auth, wallet, CDP)
                          │
PRODUCT     services/omnideliv — one deploy, schema `omnideliv`
            ├── catalog        vendors, items, availability
            ├── basket         multi-vendor cart, substitution state
            ├── mesh           Concierge + specialists  ← the split seam
            ├── consolidation  multi-stop batching, flat fee
            └── orders         lifecycle, three-leg settlement
                          │
PLATFORM    libs/agent-runtime  (extracted)    ai-layer (refactored to consume it)
            cdp (+ dietary vectors)            identity · payments · engagement · analytics
                          │
FIELD-OPS   services/field-ops  (new, ADR-0015)
            courier identity · assignment · GPS ingest · earnings ledger
```

### 3.1 The field-ops extraction (ADR-0015)

OmniDeliv needs couriers. All courier capability exists today — inside **LogisticOS's product
tier**. ADR-0009 boundary rule 2 forbids one product calling another product's services directly,
and the same ADR instructs extracting a shared field-ops tier when the second of
{LogisticOS, Ride-Hailing, Food Delivery} ships. OmniDeliv is that second product.

**Extract only what slice one needs:**

| Extracted to `services/field-ops` | Stays in LogisticOS |
|---|---|
| Courier identity (the human in the field) | POD / POP capture |
| Assignment + claim | Hub operations, cross-dock |
| GPS ingest and breadcrumbs | Carrier and sub-carrier contracts |
| Earnings ledger (`DriverLedger`) | Parcel-specific routing and manifests |

This requires migrating LogisticOS onto the extracted service — real work on a production system,
outside the hero flow. It is scoped deliberately: the alternative options were a documented
boundary violation that historically becomes permanent, or a duplicate courier stack that is
exactly the copy ADR-0009 rule 4 forbids.

**ADR-0015 must be written and accepted before this work starts.**

### 3.2 `libs/agent-runtime`

Moved out of `services/ai-layer`, consumed by both it and `services/omnideliv`:

- `AgentRunner` — the Claude loop, tool execution, turn cap, per-turn persistence
- `ToolRegistry` — tool definitions and dispatch
- `AgentSession` / `AgentAction` — audit entities
- The `allowed_tools` RBAC gate

ai-layer keeps its six logistics agents and refactors to build on the crate. Its existing tests are
the safety net for that refactor.

**Why this rather than copying the pattern:** two agent runtimes means two RBAC implementations and
every agent-loop bug fixed twice. The RBAC gate in particular is a security control — it must have
exactly one implementation.

---

## 4. The Collaborative Agent Mesh

### 4.1 Agents are roles, not singletons

The mesh instantiates **one worker per sub-intent**. The source spec's single "Nutritionist
(Food & Grocery)" therefore yields two concurrent workers in the hero flow — one for the restaurant
sub-intent, one for grocery. This is why Screen B shows three cards moving simultaneously rather
than one spinner, and it is what makes the product legibly different from a chat box.

| Role | Slice | Responsibility |
|---|---|---|
| **Concierge** | 1 | Orchestrator. Decomposes intent, reconciles deltas, owns the basket. |
| **Nutritionist** | 1 | Food & grocery. Dietary profile, allergens, availability, substitutions. |
| **Fleet** | 1 | Courier supply, multi-stop sequencing, flat-fee computation. |
| **Pharmacist** | 2 | Rx validation, OTC symptom matching, PHI handling. |
| **Botanist & Retail** | 3 | Perishable windows, catalog-to-fulfilment routing. |

### 4.2 Execution phases

| # | Phase | Concurrency | Output |
|---|---|---|---|
| 1 | Parse | Concierge, 1 run | `Vec<SubIntent>` — vertical, vendor hint, items, constraints |
| 2 | Fan-out | **Concurrent**, one tokio task per sub-intent, 8s deadline | `BasketDelta` per specialist |
| 3 | Reconcile | Concierge, single writer | Merged basket + conflict list |
| 4 | Plan | Fleet, 1 run | `ConsolidationPlan` |
| 5 | Review | Human gate | Substitution approvals (Screen C) |
| 6 | Commit | **Not an agent action** | Order + payment |

### 4.3 Handoff is a typed transition

```rust
enum MeshTransition {
    Decompose { sub_intents: Vec<SubIntent> },   // Concierge → specialists
    Propose   { delta: BasketDelta },            // specialist → Concierge
    NeedsUser { prompt: UserPrompt },            // any → human review
    Plan      { plan: ConsolidationPlan },       // Fleet → Concierge
    Settle    { order: DraftOrder },             // Concierge → commit path
}
```

A Rust enum the runner matches on — **not** a convention the model is asked to honour in prose. A
specialist returning something unparseable fails loudly and degrades that one vertical. This is
also what makes the mesh testable: assertions target transition values, never generated text.

### 4.4 The basket has exactly one writer

Specialists never mutate the basket. Each returns a `BasketDelta`; only the Concierge applies them.
Concurrent agents share no mutable state, so budget, timing and temperature conflicts surface
deterministically in phase 3 rather than as a race.

### 4.5 Per-agent tool authority

Extends the existing `AgentType::allowed_tools` gate. A restricted agent is never told the other
tools exist — the filter applies to the tool definitions sent to Claude, not just to execution.

| Agent | May call | Must never reach |
|---|---|---|
| Concierge | `get_customer_profile`, `decompose_intent`, `present_bundle` | catalog writes, fleet, payments |
| Nutritionist | `search_catalog`, `check_availability`, `get_dietary_profile`, `propose_substitution` | payments, dispatch, other customers' data |
| Fleet | `get_available_couriers`, `estimate_route`, `compute_flat_fee` | catalog, customer PII, payments |

**No agent in any role holds a tool that moves money or assigns a real courier.** Those fire from
the commit path on an explicit user action.

### 4.6 Audit

A mesh run is a parent `AgentSession` with one child session per specialist. This requires a
`parent_session_id` column on `agent_sessions`. Every specialist run stays individually auditable
in the existing AI Agents dashboard.

### 4.7 Cost and latency guards

- 8s deadline on the fan-out phase; partial results remain usable
- `MAX_TURNS` of 8 per specialist (autonomous agents keep 20)
- Per-session token budget
- Redis cache on catalog search keyed by `(vendor, normalised_query)`

---

## 5. Domain model

Schema `omnideliv`. RLS on every table per ADR-0003 / ADR-0008. Migrations run via
`logisticos_common::migrations::run` per ADR-0012.

### 5.1 Catalog

| Table | Notable columns |
|---|---|
| `vendors` | `vertical`, `geo` (PostGIS), `hours`, `prep_time_minutes`, `commission_bps`, `payout_account`, `status` |
| `catalog_items` | `sku`, `price_cents`, `modifiers` JSONB, `allergens[]`, `dietary_tags[]`, `vertical_attrs` JSONB |
| `item_availability` | `state` (Available \| Limited \| OutOfStock), **`updated_at`** |

`item_availability.updated_at` is load-bearing, not bookkeeping. Because stock is vendor-declared,
freshness is what lets the Nutritionist reason honestly: a flag touched minutes ago is trustworthy;
one from yesterday means propose a substitute defensively. This is the difference between a
substitution loop that feels intelligent and one that feels random.

### 5.2 Basket

| Table | Notable columns |
|---|---|
| `baskets` | `customer_id`, `status`, `mesh_session_id` |
| `sub_intents` | `vertical`, `vendor_hint`, `raw_text`, `constraints` JSONB — one per fanned-out specialist |
| `basket_lines` | `sub_intent_id`, `item_id`, `qty`, `state` (Proposed \| Accepted \| Substituted \| Rejected), `substitution_for` → self, `proposed_by_agent` |

### 5.3 Fulfilment and settlement

| Table | Notable columns |
|---|---|
| `consolidation_plans` | ordered `stops`, `ready_at` estimates, `temperature_classes[]`, `flat_fee_cents` |
| `orders` | `goods_total_cents`, `delivery_fee_cents`, `tip_cents`, `grand_total_cents`, `courier_task_id` → field-ops |
| `order_vendor_legs` | `goods_subtotal_cents`, `commission_bps`, `commission_cents`, `payout_cents`, `picked_up_at` |
| `vendor_ledger` + `vendor_ledger_entries` | Append-only, modelled directly on the existing `DriverLedger` |
| `order_telemetry_logs` | Append-only hypertable, dual timestamps, per the CLAUDE.md telemetry directive |

### 5.4 Three-leg settlement

| Leg | Formula |
|---|---|
| Customer pays | `goods_total + flat_delivery_fee + tip` — one charge, one fee, regardless of stop count |
| Vendor receives, per leg | `goods_subtotal − (goods_subtotal × commission_bps)`, accrued at pickup, settled independently |
| Courier receives | Per **consolidated trip**, not per stop, plus tip |
| Partner retains | `commission + (flat_fee − courier_trip_cost)` |

**Consolidation is the margin lever, not a customer perk.** The fee is flat but courier cost barely
rises with a second stop, while a second vendor adds a full commission leg. Fleet agent batching
quality is therefore a revenue lever, not a nicety.

### 5.5 Events (ADR-0006)

**Publishes:** `omnideliv.basket.proposed`, `omnideliv.order.placed`,
`omnideliv.order.vendor_leg.picked_up`, `omnideliv.order.delivered`,
`omnideliv.vendor.payout_accrued`

**Consumes:** `fieldops.courier.assigned`, `fieldops.courier.location`, `fieldops.pod.captured`

### 5.6 CDP extension

Structured `dietary_tags`, `allergens` and `taste_preferences` on the platform customer profile.
Vector embeddings are deferred — see [§9](#9-scope-boundaries).

---

## 6. The Anticipatory Canvas

Dark-first glassmorphism per CLAUDE.md. The neon palette is the neutral skin; `tenant_branding`
overrides it per Partner. Mobile-first, NativeWind + Reanimated 3.

### Screen A — Omni-Intent Canvas

Time-aware greeting, reorder widget, conversational input as the hero element, Quick Intent Pills
below.

**The pills are the non-AI fallback, not decoration.** CLAUDE.md requires a non-AI path for every
operation. They route to deterministic category browse with no model involved, so if Claude is
unavailable the app still functions. The source spec already placed them; this design gives them a
job.

### Screen B — Multi-vertical orchestration

Agent Deployment Tracker with one card per live worker, streamed over SSE as agents spawn. The
Unified Constraint Display surfaces cross-category constraints — hot mains plus chilled dairy in
one trip, so Fleet sequences grocery first.

This is the only screen where the mesh's parallelism is legible to the user. Two Nutritionist
cards rather than one spinner is the point.

### Screen C — Agentic consolidation checkout

Multi-stop route map, substitution review, fee breakdown, single CTA.

The substitution card sits **above** the totals because it is the only thing blocking checkout —
Progressive Disclosure means one decision surfaces, not five. The `ONE FEE · N STOPS` badge is
deliberately prominent: it is the product promise and the margin lever in the same element.

### Screen D — Live telemetry and handoff

One timeline, one courier, one Concierge with order context. No tracking numbers, no per-merchant
threads.

### Responsive audit

Verified at 1280 / 390 / 320 px: no horizontal scroll, zero overflowing elements, zero clipped text
nodes. Frames floor at 288 px and the grid reflows to a single column.

---

## 7. Failure modes

The governing rule: **one failure degrades one vertical, never the order.**

| Failure | Behaviour |
|---|---|
| Claude down or rate-limited | Pills stay live; input bar reports Concierge unavailable |
| Specialist exceeds 8s deadline | That card degrades to manual browse; other legs proceed |
| Unparseable transition returned | Same path — loud parse failure, never a silent wrong answer |
| Concierge cannot decompose | Collapse to single-intent best-guess vertical; worst case, pills |
| Stale flag, item gone at pickup | Courier reports; line auto-refunds; engagement notifies |
| No courier available | No plan, no charge. Offer scheduled slot or hold basket |
| SSE drops mid-orchestration | Client polls session by id; `AgentRunner` per-turn persistence means resume, not restart |
| Paid but dispatch failed | `AwaitingCourier`, auto-retry for 5 min, then ops escalation and customer notice. Must be refundable by design |
| Partial pickup, one vendor closed | Per-leg state; deliver collected items, refund failed leg |

---

## 8. Testing

Typed handoffs are what make the mesh testable — assertions target `MeshTransition` values against
a stubbed Claude client, never generated prose.

| Layer | Approach |
|---|---|
| Mesh | Stubbed Claude client; assert on transitions |
| Prompt regression | Golden-transcript tests pinning utterance → decomposition |
| RBAC | Table-driven per role, cloning the existing `customer_support_cannot_reach_operational_tools` pattern |
| Consolidation | Property tests: fee flat regardless of stop count; sequence respects temperature and `ready_at` |
| Settlement | Invariant: `customer_paid == vendor_payouts + commissions + courier_earnings + partner_margin` |
| Integration | Hero flow end to end against a seeded tenant |
| App | Existing Expo CI pattern |

**Carry-over hazard.** The Android driver app lost two full 6-hour CI runs to `runTest` sharing a
virtual clock with a `while(true){delay}` poller — the build hung rather than failed. The Expo
equivalent is `jest.useFakeTimers()` against an SSE reconnect loop. Different harness, identical
trap: give the reconnect loop an injectable clock from the start.

---

## 9. Scope boundaries

**Out of slice one:**

| Item | Lands in |
|---|---|
| Pharmacist agent, Rx, PHI, age gating | Slice two, with its own compliance review |
| Botanist & Retail agents | Slice three |
| Real POS / store-system integration | Post-slice, per merchant chain |
| WebSockets | Only if SSE proves insufficient |
| Super-app shell / product switcher | Not planned; conflicts with ADR-0009 rule 6 |
| Surge pricing, scheduled orders, multi-city | Post-slice |
| Vector DB for preferences | Deferred until behavioural data exists to embed |

On the vector DB specifically: the source spec calls for one, but embedding "taste preferences"
requires behavioural history that will not exist until the product has run for a while. Slice one
uses structured dietary and allergen fields on the CDP. Revisit once there is data.

**In slice one, with constraints:** voice input via on-device STT only — no server-side ASR, no
custom vocabulary tuning. It is a thin adapter over the same text pipeline.

---

## 10. Prerequisites and follow-on work

| Item | Blocking? |
|---|---|
| **ADR-0015** — field-ops platform tier extraction | **Yes** — write and accept before implementation |
| `services/field-ops` stood up and serving OmniDeliv | Yes — no couriers otherwise |
| `libs/agent-runtime` extraction + ai-layer refactor | Yes — mesh depends on it |
| `parent_session_id` column on `agent_sessions` | Yes — mesh audit depends on it |
| `omnideliv.api.cargomarket.net` gateway + Dokploy app | Yes — before any deploy |
| Vendor catalog console | Yes — no catalog data without it |
| CDP dietary/allergen extension | Yes — Nutritionist depends on it |
| **LogisticOS migrated onto extracted field-ops** | **No** — not a slice-one blocker. OmniDeliv can consume `services/field-ops` while LogisticOS still runs its own copy. But leaving both live indefinitely recreates the duplicate-courier-stack problem ADR-0015 exists to prevent, so ADR-0015 must carry a dated commitment for this migration. |

---

## 11. Source specification

The original OmniDeliv AI specification is preserved as the product vision. This document is the
implementable slice of it, and deviates from the source in four places, each recorded above:

1. **Rust, not Python/Node** for backend services (D1, ADR-0001)
2. **SSE, not WebSockets** for agent state streaming (D8)
3. **`Vendor`, not "merchant"** in code, to avoid a live actor-model collision (D12)
4. **No vector DB** in slice one; structured CDP fields instead (§9)
