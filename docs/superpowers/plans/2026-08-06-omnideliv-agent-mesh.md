# OmniDeliv Agent Mesh Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Collaborative Agent Mesh inside `services/omnideliv` — a Concierge that decomposes one utterance into vertical sub-intents, specialists that run concurrently against the catalog, and a single-writer reconcile that merges their deltas into a basket.

**Architecture:** A `mesh` crate inside the `services/omnideliv` workspace — the split seam identified in the spec, kept isolated so the service can later split into two deployables without a refactor. It builds on `libs/agent-runtime` (Plan 1) for the agent loop, RBAC gate and audit, and on `services/omnideliv`'s catalog and basket (Plan 3) for data. Handoffs are a typed `MeshTransition` enum, not a prompt convention. Phase 2 fans out with `tokio::spawn` under a deadline; one specialist failing degrades one vertical, never the order.

**Tech Stack:** Rust 2021, Tokio, `libs/agent-runtime`, Axum SSE, SQLx.

---

## Dependencies

**Requires Plan 1 complete** — `libs/agent-runtime` must exist with `AgentRole`, `AgentRunner`, `ClaudeApi`, `ToolBox`, `SessionStore` and the `testing` feature.

**Requires Plan 3 complete** — `services/omnideliv` must exist with `CatalogService`, `BasketService`, `BasketDelta` and `Basket::apply`.

Verify both before starting:

```bash
CARGO_INCREMENTAL=0 cargo check -p logisticos-agent-runtime -p logisticos-omnideliv
```

Expected: PASS. If either fails, finish that plan first.

---

## Scope

**In:** `MeshTransition`, the three slice-one roles, per-role tool authority, mesh tools over the catalog, the six-phase runner, concurrent fan-out with degradation, SSE streaming for Screen B, parent/child session audit.

**Out:** Pharmacist and Botanist roles (slices two and three), consolidation and the Fleet agent's real routing (Plan 5 — this plan stubs Fleet's route planning to a supply check), voice input (Plan 7).

---

## File Structure

**New — `services/omnideliv/crates/mesh/`** (workspace-internal crate):

| File | Responsibility |
|---|---|
| `Cargo.toml` | Crate manifest |
| `src/lib.rs` | Re-exports |
| `src/transition.rs` | `MeshTransition`, `SubIntentSpec`, `ConsolidationPlan` stub |
| `src/roles.rs` | `concierge()`, `nutritionist()`, `fleet()` role constructors + tool authority |
| `src/tools.rs` | `MeshToolBox` — catalog-backed tools implementing `ToolBox` |
| `src/runner.rs` | `MeshRunner` — the six phases |
| `src/events.rs` | `MeshEvent` — what Screen B renders |

**Modified — `services/omnideliv/`:** `Cargo.toml`, `src/api/http/mod.rs`, `src/api/http/mesh.rs` (new), `src/bootstrap.rs`.

**Modified — `services/ai-layer/`:** `migrations/00NN_agent_session_parent.sql` (new).

---

## Task 1: Parent/child session audit

A mesh run is one parent session with one child per specialist. Without a parent link, the AI Agents dashboard shows five unrelated sessions per order and the audit trail is unreadable.

**Files:**
- Create: `services/ai-layer/migrations/0002_agent_session_parent.sql` (renumber to the next free number — check `ls services/ai-layer/migrations/`)
- Modify: `libs/agent-runtime/src/session.rs`, `src/store.rs`

- [ ] **Step 1: Write the migration**

```sql
-- A mesh run is a parent session with one child per specialist. Without this
-- link the AI Agents dashboard shows N unrelated sessions per order.
ALTER TABLE ai_layer.agent_sessions
    ADD COLUMN IF NOT EXISTS parent_session_id UUID REFERENCES ai_layer.agent_sessions(id);

-- Children of a run, for the dashboard drill-down. Partial: most sessions are
-- roots and would only bloat the index.
CREATE INDEX IF NOT EXISTS idx_agent_session_parent
    ON ai_layer.agent_sessions (parent_session_id)
    WHERE parent_session_id IS NOT NULL;
```

> Confirm the schema name matches what `services/ai-layer/migrations/0001_*.sql` creates before running — `ai_layer` is the expected value given the `search_path` set in its bootstrap, but verify rather than assume.

- [ ] **Step 2: Write the failing test**

```rust
// libs/agent-runtime/src/session.rs — append to the existing tests block
    #[test]
    fn a_root_session_has_no_parent() {
        assert!(session().parent_session_id.is_none());
    }

    #[test]
    fn a_child_session_records_its_parent() {
        let parent = session();
        let child = AgentSession::new_child(
            parent.tenant_id,
            AgentRole::unrestricted("worker", "Worker", "You work."),
            serde_json::json!({}),
            "claude-opus-4-6",
            parent.id,
        );
        assert_eq!(child.parent_session_id, Some(parent.id));
        assert_ne!(child.id, parent.id);
    }
```

- [ ] **Step 3: Run it to verify it fails**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-agent-runtime session::`
Expected: FAIL to compile — `no field 'parent_session_id' on type 'AgentSession'`.

- [ ] **Step 4: Implement**

Add to `AgentSession` in `libs/agent-runtime/src/session.rs`:

```rust
    /// Set on a specialist's session, pointing at the mesh run that spawned it.
    /// `None` on a root session.
    pub parent_session_id: Option<Uuid>,
```

Initialise it to `None` in `AgentSession::new`, and add the child constructor:

```rust
    /// A specialist's session inside a mesh run.
    pub fn new_child(
        tenant_id: TenantId,
        role: AgentRole,
        trigger: serde_json::Value,
        model: impl Into<String>,
        parent_session_id: Uuid,
    ) -> Self {
        let mut s = Self::new(tenant_id, role, trigger, model);
        s.parent_session_id = Some(parent_session_id);
        s
    }
```

Update `services/ai-layer/src/infrastructure/db/mod.rs` to bind and read the new column in `save` and the row mapper.

- [ ] **Step 5: Run the tests**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-agent-runtime`
Expected: PASS — the existing 13 plus 2 new.

- [ ] **Step 6: Commit**

```bash
git add libs/agent-runtime/src/session.rs services/ai-layer/
git commit -m "feat(agent-runtime): parent_session_id for mesh parent/child audit"
```

---

## Task 2: The mesh crate and typed transitions

**Files:**
- Create: `services/omnideliv/crates/mesh/Cargo.toml`, `src/lib.rs`, `src/transition.rs`
- Modify: root `Cargo.toml`, `services/omnideliv/Cargo.toml`

- [ ] **Step 1: Write the manifest and register the crate**

```toml
# services/omnideliv/crates/mesh/Cargo.toml
[package]
name        = "omnideliv-mesh"
description = "Collaborative Agent Mesh — Concierge orchestration over specialist agents"
version.workspace      = true
edition.workspace      = true
authors.workspace      = true
rust-version.workspace = true

[dependencies]
logisticos-agent-runtime.workspace = true
logisticos-errors.workspace        = true
logisticos-types.workspace         = true
async-trait.workspace = true
serde.workspace       = true
serde_json.workspace  = true
tokio.workspace       = true
tracing.workspace     = true
uuid.workspace        = true
chrono.workspace      = true
anyhow.workspace      = true

[dev-dependencies]
logisticos-agent-runtime = { workspace = true, features = ["testing"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread", "time"] }
```

Add `"services/omnideliv/crates/mesh",` to the root `Cargo.toml` `members`, and `omnideliv-mesh = { path = "services/omnideliv/crates/mesh" }` to `[workspace.dependencies]`. Add `omnideliv-mesh.workspace = true` to `services/omnideliv/Cargo.toml`.

> **Why a separate crate rather than a module.** The spec identifies `mesh` as the split seam — the point where `services/omnideliv` would divide into two deployables if the LLM-bound workload needs independent scaling. A crate boundary means that split is a `Cargo.toml` change rather than a refactor, and it makes accidental coupling a compile error.

- [ ] **Step 2: Write the failing test**

```rust
// services/omnideliv/crates/mesh/src/transition.rs — tests block
#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn a_decompose_transition_parses_from_a_tool_result() {
        let raw = serde_json::json!({
            "type": "decompose",
            "sub_intents": [
                {"vertical": "restaurant", "vendor_hint": "Kuya's", "raw_text": "dinner for two", "constraints": {}},
                {"vertical": "grocery", "vendor_hint": null, "raw_text": "milk and eggs", "constraints": {}}
            ]
        });
        let t: MeshTransition = serde_json::from_value(raw).expect("must parse");
        match t {
            MeshTransition::Decompose { sub_intents } => {
                assert_eq!(sub_intents.len(), 2);
                assert_eq!(sub_intents[0].vertical, "restaurant");
                assert_eq!(sub_intents[1].vendor_hint, None);
            }
            other => panic!("expected Decompose, got {other:?}"),
        }
    }

    /// A specialist that cannot satisfy its sub-intent returns an empty Propose
    /// with a note — never a partial basket, never a silent success.
    #[test]
    fn an_empty_propose_carries_the_reason() {
        let raw = serde_json::json!({
            "type": "propose",
            "sub_intent_id": Uuid::nil(),
            "lines": [],
            "note": "no eggs in stock at any nearby vendor"
        });
        let t: MeshTransition = serde_json::from_value(raw).expect("must parse");
        match t {
            MeshTransition::Propose { lines, note, .. } => {
                assert!(lines.is_empty());
                assert_eq!(note.as_deref(), Some("no eggs in stock at any nearby vendor"));
            }
            other => panic!("expected Propose, got {other:?}"),
        }
    }

    /// The whole point of typing the handoff: a malformed transition is a loud
    /// parse failure, not a plausible-looking wrong answer that flows onward.
    #[test]
    fn an_unrecognised_transition_fails_to_parse() {
        let raw = serde_json::json!({ "type": "improvise", "whatever": true });
        assert!(serde_json::from_value::<MeshTransition>(raw).is_err());
    }

    #[test]
    fn a_decompose_missing_its_sub_intents_fails_to_parse() {
        let raw = serde_json::json!({ "type": "decompose" });
        assert!(serde_json::from_value::<MeshTransition>(raw).is_err());
    }
}
```

- [ ] **Step 3: Run it to verify it fails**

Run: `CARGO_INCREMENTAL=0 cargo test -p omnideliv-mesh transition::`
Expected: FAIL to compile — `cannot find type 'MeshTransition' in this scope`.

- [ ] **Step 4: Write the transitions**

```rust
// services/omnideliv/crates/mesh/src/transition.rs
//! Typed handoffs between mesh agents.
//!
//! These are a Rust enum the runner matches on — not a convention the model is
//! asked to honour in prose. A specialist that returns something unparseable
//! fails loudly and degrades its own vertical, rather than emitting a
//! plausible-looking wrong answer that flows into the basket.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One vertical slice of a customer's utterance, as the Concierge split it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubIntentSpec {
    pub vertical:    String,
    pub vendor_hint: Option<String>,
    /// The slice of the utterance this came from — kept so the UI can show the
    /// customer what the agent thought they asked for.
    pub raw_text:    String,
    #[serde(default)]
    pub constraints: serde_json::Value,
}

/// A line a specialist wants added to the basket.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProposedLine {
    pub vendor_id:        Uuid,
    pub item_id:          Uuid,
    pub qty:              i32,
    pub unit_price_cents: i64,
    /// Set when this line replaces another — the substitution chain.
    #[serde(default)]
    pub substitutes:      Option<Uuid>,
}

/// A courier route over the merged basket. Fleet's real planning lands in
/// Plan 5; this shape is fixed now so the mesh contract does not change then.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoutePlan {
    pub vendor_order:    Vec<Uuid>,
    pub flat_fee_cents:  i64,
    pub total_minutes:   i32,
}

/// A question only the customer can answer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserPrompt {
    pub sub_intent_id: Uuid,
    pub question:      String,
    pub options:       Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MeshTransition {
    /// Concierge → specialists.
    Decompose { sub_intents: Vec<SubIntentSpec> },
    /// Specialist → Concierge. An empty `lines` with a `note` is a legitimate
    /// outcome meaning "I could not satisfy this" — not a failure to retry.
    Propose {
        sub_intent_id: Uuid,
        #[serde(default)]
        lines: Vec<ProposedLine>,
        #[serde(default)]
        note: Option<String>,
    },
    /// Any agent → the human. Surfaces on Screen C.
    NeedsUser { prompt: UserPrompt },
    /// Fleet → Concierge.
    Plan { plan: RoutePlan },
    /// Concierge → the commit path. Not an agent action — checkout is a plain
    /// user-initiated transaction.
    Settle { basket_id: Uuid },
}
```

```rust
// services/omnideliv/crates/mesh/src/lib.rs
//! Collaborative Agent Mesh.

pub mod events;
pub mod roles;
pub mod runner;
pub mod tools;
pub mod transition;

pub use runner::{MeshOutcome, MeshRunner};
pub use transition::{MeshTransition, ProposedLine, RoutePlan, SubIntentSpec, UserPrompt};
```

- [ ] **Step 5: Run the tests**

Run: `CARGO_INCREMENTAL=0 cargo test -p omnideliv-mesh transition::`
Expected: FAIL — `file not found for module 'events'` etc. The transition tests themselves cannot run until the sibling modules exist; Tasks 3–6 add them. To verify just this file now:

Run: `CARGO_INCREMENTAL=0 cargo test -p omnideliv-mesh --lib transition:: --no-fail-fast 2>&1 | head -20`

Temporarily comment out the four unwritten `pub mod` lines in `lib.rs`, run the tests, then restore them.

Expected with the modules commented: PASS — 4 passed.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml services/omnideliv/
git commit -m "feat(mesh): typed MeshTransition handoffs

Handoffs are a Rust enum the runner matches on, not a prompt convention. A
malformed transition is a loud parse failure rather than a plausible-looking
wrong answer flowing into the basket."
```

---

## Task 3: Roles and per-agent tool authority

**Files:**
- Create: `services/omnideliv/crates/mesh/src/roles.rs`

- [ ] **Step 1: Write the failing test**

The RBAC assertions are the security-critical part and get the most explicit coverage.

```rust
// services/omnideliv/crates/mesh/src/roles.rs — tests block
#[cfg(test)]
mod tests {
    use super::*;

    /// No agent in any role may hold a tool that moves money or dispatches a
    /// real courier. Those fire from the commit path on an explicit user tap.
    #[test]
    fn no_role_can_reach_a_money_or_dispatch_tool() {
        let forbidden = [
            "charge_customer", "capture_payment", "issue_refund",
            "assign_courier", "dispatch_courier", "generate_invoice",
            "credit_vendor", "debit_courier_ledger",
        ];

        for role in [concierge(), nutritionist(), fleet()] {
            for tool in forbidden {
                assert!(
                    !role.permits(tool),
                    "{} must not reach {tool}", role.key()
                );
            }
        }
    }

    #[test]
    fn the_concierge_cannot_touch_the_catalog_or_the_fleet() {
        let c = concierge();
        assert!(c.permits("get_customer_profile"));
        assert!(c.permits("decompose_intent"));
        assert!(!c.permits("search_catalog"), "the Concierge delegates catalog work");
        assert!(!c.permits("estimate_route"));
    }

    #[test]
    fn the_nutritionist_reaches_the_catalog_but_not_the_fleet() {
        let n = nutritionist();
        assert!(n.permits("search_catalog"));
        assert!(n.permits("check_availability"));
        assert!(n.permits("propose_substitution"));
        assert!(!n.permits("estimate_route"));
        assert!(!n.permits("get_customer_profile"), "specialists get constraints passed in, not PII access");
    }

    #[test]
    fn the_fleet_agent_sees_no_catalog_and_no_customer_data() {
        let f = fleet();
        assert!(f.permits("get_available_couriers"));
        assert!(f.permits("estimate_route"));
        assert!(f.permits("compute_flat_fee"));
        assert!(!f.permits("search_catalog"));
        assert!(!f.permits("get_customer_profile"));
    }

    /// Every role is restricted. An unrestricted role here would silently grant
    /// the full registry — the failure mode this gate exists to prevent.
    #[test]
    fn every_mesh_role_is_restricted() {
        for role in [concierge(), nutritionist(), fleet()] {
            assert!(
                role.allowed_tools().is_some(),
                "{} must carry an explicit allowlist", role.key()
            );
        }
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `CARGO_INCREMENTAL=0 cargo test -p omnideliv-mesh roles::`
Expected: FAIL to compile — `cannot find function 'concierge' in this scope`.

- [ ] **Step 3: Write the roles**

```rust
// services/omnideliv/crates/mesh/src/roles.rs
//! Mesh roles.
//!
//! Agents are roles, not singletons: the runner instantiates one worker per
//! sub-intent, so a single Nutritionist role yields two live workers when an
//! utterance splits into restaurant and grocery.
//!
//! Every role carries an explicit allowlist. A restricted role is never told
//! the other tools exist — the filter applies to the definitions sent to
//! Claude, not merely to execution.

use logisticos_agent_runtime::AgentRole;

pub const CONCIERGE_KEY:    &str = "concierge";
pub const NUTRITIONIST_KEY: &str = "nutritionist";
pub const FLEET_KEY:        &str = "fleet";

/// The orchestrator. Reads the customer profile, splits the utterance, and
/// owns the basket — but does no catalog or fleet work itself.
pub fn concierge() -> AgentRole {
    AgentRole::restricted(
        CONCIERGE_KEY,
        "Concierge",
        "You are the OmniDeliv Concierge. A customer has told you what they need in \
         one message. Your only job in this turn is to split it into separate \
         sub-intents, one per vertical (restaurant, grocery, pharmacy, florist, \
         retail). Call decompose_intent exactly once with the full list. \
         \
         Split by vertical, not by item: 'dinner from Kuya's and we're out of milk \
         and eggs' is two sub-intents (restaurant, grocery), not three. Carry any \
         constraint the customer stated — budget, dietary, timing — into the \
         constraints of the sub-intent it applies to. \
         \
         Do not search for products, pick vendors, or estimate delivery. \
         Specialists do that. Never invent a vertical the customer did not ask for.",
        ["get_customer_profile", "decompose_intent", "present_bundle"],
    )
}

/// Food and grocery. Owns dietary filtering, availability reasoning and
/// substitution. Instantiated once per food-or-grocery sub-intent.
pub fn nutritionist() -> AgentRole {
    AgentRole::restricted(
        NUTRITIONIST_KEY,
        "Nutritionist",
        "You are the OmniDeliv Nutritionist, working one sub-intent of a larger \
         order. Find items at the vendor that satisfy it, then call propose_lines \
         exactly once with what you chose. \
         \
         Respect every allergen in your constraints absolutely — never propose an \
         item that carries one, and never substitute around a dietary restriction. \
         \
         search_catalog returns a warrants_substitute flag per item. When it is \
         true the item is out of stock, nearly out, or last confirmed present too \
         long ago to rely on — propose a replacement alongside it via \
         propose_substitution so the customer has a choice rather than a failed \
         pickup. Do not silently swap: the customer approves substitutions. \
         \
         If nothing satisfies the sub-intent, call propose_lines with an empty \
         list and a note saying why. An honest empty result is correct; a \
         plausible wrong item is not.",
        ["search_catalog", "check_availability", "propose_substitution", "propose_lines"],
    )
}

/// Courier supply and routing. Sees no catalog and no customer data — it works
/// from vendor locations and the merged basket alone.
pub fn fleet() -> AgentRole {
    AgentRole::restricted(
        FLEET_KEY,
        "Fleet",
        "You are the OmniDeliv Fleet agent. You have a merged basket spanning one \
         or more vendors. Sequence the pickups and compute one flat delivery fee. \
         \
         Sequence by readiness, not distance alone: a grocery pick ready in 5 \
         minutes should be collected before a kitchen order ready in 20, so hot \
         food spends the least time in the bag. Where a basket mixes hot and \
         chilled items, say so in your plan. \
         \
         The fee is flat regardless of stop count — that is the product promise. \
         Call plan_route exactly once.",
        ["get_available_couriers", "estimate_route", "compute_flat_fee", "plan_route"],
    )
}
```

- [ ] **Step 4: Run the tests**

Run: `CARGO_INCREMENTAL=0 cargo test -p omnideliv-mesh roles::`
Expected: PASS — 5 passed.

- [ ] **Step 5: Commit**

```bash
git add services/omnideliv/crates/mesh/src/roles.rs
git commit -m "feat(mesh): three slice-one roles with explicit tool allowlists

No role reaches a tool that moves money or dispatches a courier — those fire
from the commit path on an explicit user tap, not from an agent."
```

---

## Task 4: Mesh tools over the catalog

**Files:**
- Create: `services/omnideliv/crates/mesh/src/tools.rs`

- [ ] **Step 1: Write the tool box**

```rust
// services/omnideliv/crates/mesh/src/tools.rs
//! The tools mesh agents may call.
//!
//! This is the only place the mesh touches product data. Each tool is a thin,
//! auditable wrapper over an application service — no business logic lives here,
//! so a tool call is always traceable to a service method in the audit log.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use uuid::Uuid;

use logisticos_agent_runtime::tools::{ToolBox, ToolContext, ToolDefinition, ToolResult};

/// What the mesh needs from the host service. A trait rather than a direct
/// dependency on `services/omnideliv` types, so the mesh crate stays testable
/// in isolation and the split seam holds.
#[async_trait]
pub trait MeshCatalog: Send + Sync {
    /// Items matching `query` at `vendor_id`, excluding allergen clashes.
    /// Each hit carries `warrants_substitute`.
    async fn search(
        &self,
        tenant_id: Uuid,
        vendor_id: Uuid,
        query: &str,
        avoid_allergens: &[String],
        limit: i64,
    ) -> anyhow::Result<serde_json::Value>;

    /// Orderable vendors of a vertical near the customer.
    async fn vendors_near(
        &self,
        tenant_id: Uuid,
        vertical: &str,
        lat: f64,
        lng: f64,
        radius_km: f64,
        limit: i64,
    ) -> anyhow::Result<serde_json::Value>;

    /// Courier supply near a point. Backed by field-ops.
    async fn courier_supply(
        &self,
        tenant_id: Uuid,
        lat: f64,
        lng: f64,
        radius_km: f64,
    ) -> anyhow::Result<serde_json::Value>;
}

pub struct MeshToolBox {
    catalog:    Arc<dyn MeshCatalog>,
    tenant_id:  Uuid,
    defs:       Vec<ToolDefinition>,
}

fn def(name: &str, description: &str, schema: serde_json::Value) -> ToolDefinition {
    ToolDefinition {
        name:         name.to_string(),
        description:  description.to_string(),
        input_schema: schema,
    }
}

impl MeshToolBox {
    pub fn new(catalog: Arc<dyn MeshCatalog>, tenant_id: Uuid) -> Self {
        // Every tool any mesh role may call. Per-role filtering happens in the
        // runner via the role's allowlist — a role never sees the others.
        let defs = vec![
            def(
                "decompose_intent",
                "Split the customer's message into one sub-intent per vertical. Call exactly once.",
                json!({
                    "type": "object",
                    "properties": {
                        "sub_intents": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "vertical":    {"type": "string", "enum": ["restaurant","grocery","pharmacy","florist","retail"]},
                                    "vendor_hint": {"type": ["string","null"], "description": "Vendor the customer named, if any"},
                                    "raw_text":    {"type": "string", "description": "The slice of the message this covers"},
                                    "constraints": {"type": "object", "description": "Budget, dietary and timing constraints that apply"}
                                },
                                "required": ["vertical", "raw_text"]
                            }
                        }
                    },
                    "required": ["sub_intents"]
                }),
            ),
            def(
                "search_catalog",
                "Search a vendor's catalog. Each result carries warrants_substitute: when true the \
                 item is out of stock, nearly out, or last confirmed present too long ago to rely on.",
                json!({
                    "type": "object",
                    "properties": {
                        "vendor_id":       {"type": "string"},
                        "query":           {"type": "string"},
                        "avoid_allergens": {"type": "array", "items": {"type": "string"}},
                        "limit":           {"type": "integer", "default": 20}
                    },
                    "required": ["vendor_id", "query"]
                }),
            ),
            def(
                "check_availability",
                "Current availability and freshness for one item.",
                json!({
                    "type": "object",
                    "properties": { "item_id": {"type": "string"} },
                    "required": ["item_id"]
                }),
            ),
            def(
                "propose_substitution",
                "Find replacements for an item that warrants one.",
                json!({
                    "type": "object",
                    "properties": {
                        "vendor_id":       {"type": "string"},
                        "original_item_id":{"type": "string"},
                        "query":           {"type": "string"},
                        "avoid_allergens": {"type": "array", "items": {"type": "string"}}
                    },
                    "required": ["vendor_id", "original_item_id", "query"]
                }),
            ),
            def(
                "propose_lines",
                "Submit the lines for your sub-intent. Call exactly once. An empty list with a \
                 note is correct when nothing satisfies the sub-intent.",
                json!({
                    "type": "object",
                    "properties": {
                        "lines": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "vendor_id":        {"type": "string"},
                                    "item_id":          {"type": "string"},
                                    "qty":              {"type": "integer", "minimum": 1},
                                    "unit_price_cents": {"type": "integer", "minimum": 0},
                                    "substitutes":      {"type": ["string","null"]}
                                },
                                "required": ["vendor_id", "item_id", "qty", "unit_price_cents"]
                            }
                        },
                        "note": {"type": ["string","null"]}
                    },
                    "required": ["lines"]
                }),
            ),
            def(
                "get_available_couriers",
                "Courier supply near a point.",
                json!({
                    "type": "object",
                    "properties": {
                        "lat": {"type": "number"},
                        "lng": {"type": "number"},
                        "radius_km": {"type": "number", "default": 5}
                    },
                    "required": ["lat", "lng"]
                }),
            ),
            def(
                "estimate_route",
                "Distance and duration for an ordered list of vendor stops.",
                json!({
                    "type": "object",
                    "properties": { "vendor_ids": {"type": "array", "items": {"type": "string"}} },
                    "required": ["vendor_ids"]
                }),
            ),
            def(
                "compute_flat_fee",
                "The single delivery fee for a route. Flat regardless of stop count.",
                json!({
                    "type": "object",
                    "properties": { "distance_km": {"type": "number"} },
                    "required": ["distance_km"]
                }),
            ),
            def(
                "plan_route",
                "Submit the pickup sequence and flat fee. Call exactly once.",
                json!({
                    "type": "object",
                    "properties": {
                        "vendor_order":   {"type": "array", "items": {"type": "string"}},
                        "flat_fee_cents": {"type": "integer", "minimum": 0},
                        "total_minutes":  {"type": "integer", "minimum": 0}
                    },
                    "required": ["vendor_order", "flat_fee_cents", "total_minutes"]
                }),
            ),
            def(
                "get_customer_profile",
                "Dietary tags, allergens and taste preferences for the current customer.",
                json!({ "type": "object", "properties": {} }),
            ),
            def(
                "present_bundle",
                "Hand the assembled bundle to the customer for review.",
                json!({
                    "type": "object",
                    "properties": { "summary": {"type": "string"} },
                    "required": ["summary"]
                }),
            ),
        ];

        Self { catalog, tenant_id, defs }
    }
}

#[async_trait]
impl ToolBox for MeshToolBox {
    fn definitions(&self) -> &[ToolDefinition] { &self.defs }

    async fn execute(
        &self,
        name: String,
        input: serde_json::Value,
        tool_use_id: String,
        _ctx: ToolContext,
    ) -> ToolResult {
        let ok = |content: serde_json::Value| ToolResult {
            tool_use_id: tool_use_id.clone(),
            content,
            is_error: false,
        };
        let err = |msg: String| ToolResult {
            tool_use_id: tool_use_id.clone(),
            content: json!({ "error": msg }),
            is_error: true,
        };

        // Terminal tools: the runner reads these off the session's actions and
        // turns them into transitions. Executing them is a no-op acknowledgement
        // — the value is that they are structured, schema-validated, and audited.
        match name.as_str() {
            "decompose_intent" | "propose_lines" | "plan_route" | "present_bundle" => {
                return ok(json!({ "accepted": true }));
            }
            _ => {}
        }

        let parse_uuid = |v: Option<&serde_json::Value>, field: &str| -> Result<Uuid, String> {
            v.and_then(|x| x.as_str())
                .ok_or_else(|| format!("{field} is required"))
                .and_then(|s| Uuid::parse_str(s).map_err(|_| format!("{field} is not a uuid")))
        };

        match name.as_str() {
            "search_catalog" | "propose_substitution" => {
                let vendor_id = match parse_uuid(input.get("vendor_id"), "vendor_id") {
                    Ok(v) => v,
                    Err(e) => return err(e),
                };
                let query = input.get("query").and_then(|v| v.as_str()).unwrap_or("");
                let avoid: Vec<String> = input
                    .get("avoid_allergens")
                    .and_then(|v| v.as_array())
                    .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect())
                    .unwrap_or_default();
                let limit = input.get("limit").and_then(|v| v.as_i64()).unwrap_or(20);

                match self.catalog.search(self.tenant_id, vendor_id, query, &avoid, limit).await {
                    Ok(v) => ok(v),
                    Err(e) => err(format!("catalog search failed: {e}")),
                }
            }

            "get_available_couriers" => {
                let lat = input.get("lat").and_then(|v| v.as_f64()).unwrap_or_default();
                let lng = input.get("lng").and_then(|v| v.as_f64()).unwrap_or_default();
                let radius = input.get("radius_km").and_then(|v| v.as_f64()).unwrap_or(5.0);
                match self.catalog.courier_supply(self.tenant_id, lat, lng, radius).await {
                    Ok(v) => ok(v),
                    Err(e) => err(format!("courier supply lookup failed: {e}")),
                }
            }

            // Deterministic tools — no service call needed.
            "estimate_route" => {
                let stops = input.get("vendor_ids").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
                ok(json!({ "stops": stops, "note": "distance is computed at consolidation time" }))
            }
            "compute_flat_fee" => {
                let km = input.get("distance_km").and_then(|v| v.as_f64()).unwrap_or(0.0);
                // Placeholder tariff until Plan 5 owns pricing. Deliberately
                // simple and visible rather than hidden behind a stub service.
                let fee = 4_900 + (km.max(0.0) * 600.0) as i64;
                ok(json!({ "flat_fee_cents": fee }))
            }
            "check_availability" => {
                match parse_uuid(input.get("item_id"), "item_id") {
                    Ok(_) => ok(json!({ "note": "use search_catalog — it returns availability inline" })),
                    Err(e) => err(e),
                }
            }
            "get_customer_profile" => {
                // Constraints are passed into each sub-intent by the Concierge;
                // specialists do not get PII access. Plan 4 leaves this to the
                // Concierge only, and it returns what the CDP extension provides.
                ok(json!({ "dietary_tags": [], "allergens": [], "taste_preferences": [] }))
            }

            other => err(format!("unknown tool: {other}")),
        }
    }
}
```

- [ ] **Step 2: Verify it type-checks**

Run: `CARGO_INCREMENTAL=0 cargo check -p omnideliv-mesh`
Expected: FAIL — `file not found for module 'events'` and `'runner'` only.

- [ ] **Step 3: Commit**

```bash
git add services/omnideliv/crates/mesh/src/tools.rs
git commit -m "feat(mesh): catalog-backed tool box behind a MeshCatalog trait

Tools are thin wrappers over application services so every call is traceable
to a service method in the audit log. The trait keeps the mesh crate testable
in isolation and holds the split seam."
```

---

## Task 5: Mesh events for Screen B

**Files:**
- Create: `services/omnideliv/crates/mesh/src/events.rs`

- [ ] **Step 1: Write the events**

```rust
// services/omnideliv/crates/mesh/src/events.rs
//! What Screen B renders.
//!
//! One event per observable change in the run. The client draws a card per
//! `SpecialistStarted` and updates it on `SpecialistProgress` / `Finished` —
//! which is why the fan-out is legible to the user as parallel work rather
//! than a single spinner.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum MeshEvent {
    /// Phase 1 done — the customer's message has been split.
    IntentParsed { sub_intent_count: usize },

    /// A specialist worker has spawned. One card appears on Screen B.
    SpecialistStarted {
        sub_intent_id: Uuid,
        role:          String,
        vertical:      String,
        /// What this worker is doing, in the customer's language.
        label:         String,
    },

    SpecialistProgress { sub_intent_id: Uuid, note: String },

    SpecialistFinished {
        sub_intent_id: Uuid,
        lines_added:   usize,
        /// True when this worker timed out or failed. Its card degrades; the
        /// rest of the order proceeds.
        degraded:      bool,
        note:          Option<String>,
    },

    /// A constraint spanning verticals — hot food beside chilled dairy.
    ConstraintDetected { description: String },

    RoutePlanned { stops: usize, flat_fee_cents: i64, total_minutes: i32 },

    /// Terminal. `needs_review` drives the jump to Screen C.
    Completed { basket_id: Uuid, needs_review: usize },

    /// Terminal failure — the mesh produced nothing usable and the client
    /// should fall back to deterministic browse.
    Failed { reason: String },
}
```

- [ ] **Step 2: Commit**

```bash
git add services/omnideliv/crates/mesh/src/events.rs
git commit -m "feat(mesh): MeshEvent stream contract for the orchestration screen"
```

---

## Task 6: The runner — phases, fan-out and degradation

**Files:**
- Create: `services/omnideliv/crates/mesh/src/runner.rs`

- [ ] **Step 1: Write the failing tests**

These are the tests that were impossible before Plan 1 introduced `ClaudeApi`.

```rust
// services/omnideliv/crates/mesh/src/runner.rs — tests block
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use logisticos_agent_runtime::testing::{InMemoryStore, StubClaude};
    use logisticos_types::TenantId;
    use uuid::Uuid;

    struct NoopCatalog;

    #[async_trait::async_trait]
    impl crate::tools::MeshCatalog for NoopCatalog {
        async fn search(&self, _: Uuid, _: Uuid, _: &str, _: &[String], _: i64)
            -> anyhow::Result<serde_json::Value> { Ok(serde_json::json!({ "items": [] })) }
        async fn vendors_near(&self, _: Uuid, _: &str, _: f64, _: f64, _: f64, _: i64)
            -> anyhow::Result<serde_json::Value> { Ok(serde_json::json!({ "vendors": [] })) }
        async fn courier_supply(&self, _: Uuid, _: f64, _: f64, _: f64)
            -> anyhow::Result<serde_json::Value> { Ok(serde_json::json!({ "available": 6 })) }
    }

    fn spec(vertical: &str, text: &str) -> SubIntentSpec {
        SubIntentSpec {
            vertical: vertical.into(),
            vendor_hint: None,
            raw_text: text.into(),
            constraints: serde_json::json!({}),
        }
    }

    /// One Nutritionist role, two sub-intents, two live workers. This is what
    /// makes Screen B show parallel cards rather than one spinner.
    #[test]
    fn one_role_yields_one_worker_per_sub_intent() {
        let specs = vec![spec("restaurant", "dinner for two"), spec("grocery", "milk and eggs")];
        let workers = plan_workers(&specs);

        assert_eq!(workers.len(), 2);
        assert!(workers.iter().all(|w| w.role.key() == crate::roles::NUTRITIONIST_KEY),
                "both restaurant and grocery are Nutritionist work");
        assert_ne!(workers[0].sub_intent_id, workers[1].sub_intent_id);
    }

    /// A vertical with no slice-one specialist degrades rather than failing the
    /// run — pharmacy arrives in slice two.
    #[test]
    fn an_unsupported_vertical_degrades_instead_of_failing() {
        let specs = vec![spec("restaurant", "soup"), spec("pharmacy", "paracetamol")];
        let workers = plan_workers(&specs);

        assert_eq!(workers.len(), 1, "only the restaurant sub-intent has a slice-one worker");
        assert_eq!(workers[0].vertical, "restaurant");
    }

    #[tokio::test]
    async fn a_specialist_that_times_out_degrades_only_its_own_vertical() {
        let outcome = reconcile_results(vec![
            SpecialistResult { sub_intent_id: Uuid::new_v4(), lines: vec![proposed()], degraded: false, note: None },
            SpecialistResult { sub_intent_id: Uuid::new_v4(), lines: vec![], degraded: true,
                               note: Some("deadline exceeded".into()) },
        ]);

        assert_eq!(outcome.lines.len(), 1, "the healthy vertical's lines survive");
        assert_eq!(outcome.degraded_count, 1);
        assert!(!outcome.total_failure, "one degraded vertical must not fail the order");
    }

    /// Every specialist failing IS a total failure — the client falls back to
    /// deterministic browse rather than showing an empty basket as success.
    #[tokio::test]
    async fn every_specialist_failing_is_a_total_failure() {
        let outcome = reconcile_results(vec![
            SpecialistResult { sub_intent_id: Uuid::new_v4(), lines: vec![], degraded: true, note: None },
            SpecialistResult { sub_intent_id: Uuid::new_v4(), lines: vec![], degraded: true, note: None },
        ]);

        assert!(outcome.total_failure);
    }

    /// An empty-but-successful result is not a failure: the specialist looked
    /// and honestly found nothing.
    #[tokio::test]
    async fn an_honest_empty_result_is_not_a_failure() {
        let outcome = reconcile_results(vec![
            SpecialistResult { sub_intent_id: Uuid::new_v4(), lines: vec![], degraded: false,
                               note: Some("no eggs anywhere nearby".into()) },
        ]);

        assert!(!outcome.total_failure);
        assert_eq!(outcome.degraded_count, 0);
    }

    fn proposed() -> ProposedLine {
        ProposedLine {
            vendor_id: Uuid::new_v4(),
            item_id: Uuid::new_v4(),
            qty: 1,
            unit_price_cents: 34_000,
            substitutes: None,
        }
    }

    #[tokio::test]
    async fn the_parent_session_is_recorded_before_any_specialist_runs() {
        let store = Arc::new(InMemoryStore::default());
        let runner = MeshRunner::new(
            Arc::new(StubClaude::new(vec![StubClaude::text("ok")])),
            Arc::new(crate::tools::MeshToolBox::new(Arc::new(NoopCatalog), Uuid::new_v4())),
            store.clone(),
            MeshConfig::default(),
        );

        let _ = runner
            .parse(TenantId::from_uuid(Uuid::new_v4()), "dinner and milk".into())
            .await;

        assert!(!store.saved.lock().unwrap().is_empty(),
                "the parent session must exist before fan-out, so a crash mid-run is recoverable");
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `CARGO_INCREMENTAL=0 cargo test -p omnideliv-mesh runner::`
Expected: FAIL to compile — `cannot find function 'plan_workers' in this scope`.

- [ ] **Step 3: Write the runner**

```rust
// services/omnideliv/crates/mesh/src/runner.rs
//! The six-phase mesh run.
//!
//!   1. Parse      — Concierge splits the utterance
//!   2. Fan-out    — one concurrent worker per sub-intent, under a deadline
//!   3. Reconcile  — single writer merges the deltas
//!   4. Plan       — Fleet sequences the merged basket
//!   5. Review     — unresolved substitutions go to the human
//!   6. Commit     — NOT an agent action; the checkout path owns it
//!
//! Phase 2 is the only concurrent one. Workers share no mutable state: each
//! returns a result scoped to its own sub-intent, and only phase 3 writes.

use std::sync::Arc;
use std::time::Duration;

use logisticos_agent_runtime::{
    claude::ClaudeApi, tools::ToolBox, AgentRole, AgentRunner, SessionStore,
};
use logisticos_types::TenantId;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::events::MeshEvent;
use crate::roles;
use crate::transition::{MeshTransition, ProposedLine, SubIntentSpec};

#[derive(Debug, Clone)]
pub struct MeshConfig {
    /// How long phase 2 waits for all workers. Partial results are usable —
    /// a worker still running at the deadline degrades its own vertical.
    pub fanout_deadline: Duration,
    /// Turn cap per specialist. Lower than the runtime's default 20: a
    /// specialist that has not proposed lines within this many turns is
    /// looping, not thinking.
    pub max_turns_per_specialist: usize,
}

impl Default for MeshConfig {
    fn default() -> Self {
        Self {
            fanout_deadline: Duration::from_secs(8),
            max_turns_per_specialist: 8,
        }
    }
}

/// One planned worker: a role instantiated against one sub-intent.
#[derive(Debug, Clone)]
pub struct PlannedWorker {
    pub sub_intent_id: Uuid,
    pub vertical:      String,
    pub role:          AgentRole,
    pub spec:          SubIntentSpec,
}

/// Which role handles a vertical in slice one. Pharmacy, florist and retail
/// return `None` — their specialists arrive in later slices, and a sub-intent
/// with no worker degrades rather than failing the run.
fn role_for(vertical: &str) -> Option<AgentRole> {
    match vertical {
        "restaurant" | "grocery" => Some(roles::nutritionist()),
        _ => None,
    }
}

/// Instantiate one worker per sub-intent. Agents are roles, not singletons:
/// two Nutritionist workers is the normal case, not a special one.
pub fn plan_workers(specs: &[SubIntentSpec]) -> Vec<PlannedWorker> {
    specs
        .iter()
        .filter_map(|spec| {
            role_for(&spec.vertical).map(|role| PlannedWorker {
                sub_intent_id: Uuid::new_v4(),
                vertical:      spec.vertical.clone(),
                role,
                spec:          spec.clone(),
            })
        })
        .collect()
}

/// What one specialist produced.
#[derive(Debug, Clone)]
pub struct SpecialistResult {
    pub sub_intent_id: Uuid,
    pub lines:         Vec<ProposedLine>,
    /// True when the worker timed out, errored, or returned an unparseable
    /// transition. Distinct from "looked and found nothing", which is not
    /// degraded — an honest empty result is a correct outcome.
    pub degraded:      bool,
    pub note:          Option<String>,
}

#[derive(Debug, Clone)]
pub struct MeshOutcome {
    pub lines:          Vec<(Uuid, Vec<ProposedLine>)>,
    pub degraded_count: usize,
    /// Every worker degraded — the mesh produced nothing usable and the client
    /// should fall back to deterministic browse.
    pub total_failure:  bool,
}

/// Phase 3. The single writer: merges results, counts degradations, and decides
/// whether the run produced anything usable.
pub fn reconcile_results(results: Vec<SpecialistResult>) -> MeshOutcome {
    let degraded_count = results.iter().filter(|r| r.degraded).count();
    let total_failure = !results.is_empty() && degraded_count == results.len();

    let lines = results
        .into_iter()
        .filter(|r| !r.degraded)
        .map(|r| (r.sub_intent_id, r.lines))
        .collect();

    MeshOutcome { lines, degraded_count, total_failure }
}

pub struct MeshRunner {
    claude: Arc<dyn ClaudeApi>,
    tools:  Arc<dyn ToolBox>,
    store:  Arc<dyn SessionStore>,
    config: MeshConfig,
}

impl MeshRunner {
    pub fn new(
        claude: Arc<dyn ClaudeApi>,
        tools: Arc<dyn ToolBox>,
        store: Arc<dyn SessionStore>,
        config: MeshConfig,
    ) -> Self {
        Self { claude, tools, store, config }
    }

    /// Phase 1. Returns the parent session id alongside the split, so every
    /// specialist can be linked to the run that spawned it.
    pub async fn parse(
        &self,
        tenant_id: TenantId,
        utterance: String,
    ) -> anyhow::Result<(Uuid, Vec<SubIntentSpec>)> {
        let runner = AgentRunner::new(self.claude.clone(), self.tools.clone(), self.store.clone());

        let session = runner
            .run(
                tenant_id,
                roles::concierge(),
                serde_json::json!({ "source": "omni_intent_canvas" }),
                utterance,
            )
            .await?;

        // The decomposition arrives as a tool call, not as prose. Reading it off
        // the audited action rather than parsing the reply is what makes the
        // handoff typed end to end.
        let specs = session
            .actions
            .iter()
            .rev()
            .find(|a| a.tool_name == "decompose_intent" && a.succeeded)
            .and_then(|a| {
                serde_json::from_value::<MeshTransition>(
                    serde_json::json!({ "type": "decompose", "sub_intents": a.tool_input.get("sub_intents") }),
                )
                .ok()
            })
            .and_then(|t| match t {
                MeshTransition::Decompose { sub_intents } => Some(sub_intents),
                _ => None,
            })
            .unwrap_or_default();

        Ok((session.id, specs))
    }

    /// Phase 2. One concurrent task per worker, joined under a deadline.
    ///
    /// A worker still running when the deadline passes is abandoned and its
    /// vertical degrades — the order proceeds with what the others returned.
    /// This is why the deadline is on the join, not on each task: one slow
    /// specialist must not extend the customer's wait.
    pub async fn fan_out(
        &self,
        tenant_id: TenantId,
        parent_session_id: Uuid,
        workers: Vec<PlannedWorker>,
        events: mpsc::Sender<MeshEvent>,
    ) -> Vec<SpecialistResult> {
        let mut handles = Vec::with_capacity(workers.len());

        for w in workers {
            let _ = events
                .send(MeshEvent::SpecialistStarted {
                    sub_intent_id: w.sub_intent_id,
                    role:          w.role.key().to_string(),
                    vertical:      w.vertical.clone(),
                    label:         format!("Checking {}", w.vertical),
                })
                .await;

            let claude = self.claude.clone();
            let tools  = self.tools.clone();
            let store  = self.store.clone();
            let tx     = events.clone();

            handles.push(tokio::spawn(async move {
                let runner = AgentRunner::new(claude, tools, store);
                let trigger = serde_json::json!({
                    "parent_session_id": parent_session_id,
                    "sub_intent_id":     w.sub_intent_id,
                    "vertical":          w.vertical,
                    "constraints":       w.spec.constraints,
                });

                let result = runner
                    .run(tenant_id, w.role.clone(), trigger, w.spec.raw_text.clone())
                    .await;

                let out = match result {
                    Ok(session) => {
                        let lines = session
                            .actions
                            .iter()
                            .rev()
                            .find(|a| a.tool_name == "propose_lines" && a.succeeded)
                            .and_then(|a| {
                                a.tool_input
                                    .get("lines")
                                    .and_then(|l| serde_json::from_value::<Vec<ProposedLine>>(l.clone()).ok())
                            });

                        match lines {
                            // Parsed cleanly — including an honest empty list.
                            Some(lines) => SpecialistResult {
                                sub_intent_id: w.sub_intent_id,
                                lines,
                                degraded: false,
                                note: session.outcome.clone(),
                            },
                            // No parseable proposal. Loud degradation, not a
                            // silent empty basket.
                            None => SpecialistResult {
                                sub_intent_id: w.sub_intent_id,
                                lines: vec![],
                                degraded: true,
                                note: Some("specialist returned no parseable proposal".into()),
                            },
                        }
                    }
                    Err(e) => SpecialistResult {
                        sub_intent_id: w.sub_intent_id,
                        lines: vec![],
                        degraded: true,
                        note: Some(format!("specialist failed: {e}")),
                    },
                };

                let _ = tx
                    .send(MeshEvent::SpecialistFinished {
                        sub_intent_id: out.sub_intent_id,
                        lines_added:   out.lines.len(),
                        degraded:      out.degraded,
                        note:          out.note.clone(),
                    })
                    .await;

                out
            }));
        }

        let joined = tokio::time::timeout(
            self.config.fanout_deadline,
            futures_join(handles),
        )
        .await;

        match joined {
            Ok(results) => results,
            Err(_) => {
                tracing::warn!("mesh fan-out hit its deadline; proceeding with partial results");
                vec![]
            }
        }
    }
}

/// Join every handle, turning a panicked or cancelled task into a degraded
/// result rather than propagating the panic into the request.
async fn futures_join(
    handles: Vec<tokio::task::JoinHandle<SpecialistResult>>,
) -> Vec<SpecialistResult> {
    let mut out = Vec::with_capacity(handles.len());
    for h in handles {
        match h.await {
            Ok(r) => out.push(r),
            Err(e) => {
                tracing::error!(err = %e, "specialist task panicked");
                out.push(SpecialistResult {
                    sub_intent_id: Uuid::nil(),
                    lines: vec![],
                    degraded: true,
                    note: Some("specialist task panicked".into()),
                });
            }
        }
    }
    out
}
```

> **On the deadline returning `vec![]`.** When the whole join exceeds the deadline, every worker is abandoned and `reconcile_results` sees an empty list — which yields `total_failure: false` and no lines, so the client falls back to browse. That is the intended behaviour but it discards work that may have completed. Task 8 replaces the blunt join-timeout with a per-worker `select!` that keeps finished results; it is called out here rather than left as a silent limitation.

- [ ] **Step 4: Run the tests**

Run: `CARGO_INCREMENTAL=0 cargo test -p omnideliv-mesh runner::`
Expected: PASS — 6 passed.

- [ ] **Step 5: Commit**

```bash
git add services/omnideliv/crates/mesh/src/runner.rs
git commit -m "feat(mesh): six-phase runner with concurrent fan-out and per-vertical degradation

Workers share no mutable state — each returns a result scoped to its own
sub-intent and only reconcile writes. A specialist that times out, errors or
returns an unparseable proposal degrades its own vertical; the order proceeds.
Every specialist degrading is a total failure, so the client falls back to
deterministic browse rather than showing an empty basket as success."
```

---

## Task 7: SSE endpoint and service wiring

**Files:**
- Create: `services/omnideliv/src/api/http/mesh.rs`
- Modify: `services/omnideliv/src/api/http/mod.rs`, `src/bootstrap.rs`

- [ ] **Step 1: Write the SSE route**

```rust
// services/omnideliv/src/api/http/mesh.rs
//! Screen B's transport.
//!
//! Server-Sent Events, not WebSockets: the orchestration window is seconds long
//! and strictly server→client, so a persistent bidirectional socket would add
//! sticky sessions, reconnect handling and mobile background-socket behaviour
//! for no benefit. The stream closes when the run ends.

use std::convert::Infallible;
use std::sync::Arc;

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    routing::post,
    Json, Router,
};
use futures_util::stream::Stream;
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

use crate::api::http::AppState;

#[derive(Debug, Deserialize)]
pub struct RunRequest {
    pub utterance: String,
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/v1/mesh/run", post(run))
}

async fn run(
    State(st): State<Arc<AppState>>,
    Json(req): Json<RunRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Buffered so a slow client cannot block a specialist. If the client
    // disconnects the run still completes and the basket is persisted — the
    // customer can reopen it rather than losing the work.
    let (tx, rx) = mpsc::channel(64);

    let mesh = st.mesh.clone();
    let utterance = req.utterance;
    tokio::spawn(async move {
        mesh.run(utterance, tx).await;
    });

    let stream = ReceiverStream::new(rx).map(|ev| {
        Ok(Event::default()
            .json_data(&ev)
            .unwrap_or_else(|_| Event::default().data("{\"event\":\"failed\",\"reason\":\"serialisation\"}")))
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}
```

Add `futures-util` and `tokio-stream` to `services/omnideliv/Cargo.toml`:

```toml
futures-util = "0.3"
tokio-stream = "0.1"
```

- [ ] **Step 2: Wire the router and bootstrap**

In `services/omnideliv/src/api/http/mod.rs`, add `pub mod mesh;`, add `pub mesh: Arc<MeshService>` to `AppState`, and merge `mesh::routes()` into the authenticated router alongside catalog and baskets.

In `src/bootstrap.rs`, construct the mesh dependencies:

```rust
    use logisticos_agent_runtime::claude::ClaudeClient;
    use omnideliv_mesh::{runner::MeshConfig, tools::MeshToolBox, MeshRunner};

    let claude = Arc::new(ClaudeClient::new(
        cfg.claude_api_key.clone(),
        cfg.claude_model.clone(),
        cfg.claude_max_tokens,
    ));

    let mesh_runner = Arc::new(MeshRunner::new(
        claude,
        Arc::new(MeshToolBox::new(catalog_adapter.clone(), tenant_id)),
        session_store.clone(),
        MeshConfig::default(),
    ));
```

Add the corresponding fields to `Config` in `src/config.rs`, mirroring the ai-layer settings from Plan 1 Task 9:

```rust
    pub claude_api_key: String,
    #[serde(default = "default_claude_model")]
    pub claude_model: String,
    #[serde(default = "default_claude_max_tokens")]
    pub claude_max_tokens: u32,

fn default_claude_model() -> String { "claude-opus-4-6".to_string() }
fn default_claude_max_tokens() -> u32 { 8192 }
```

> **Why 8192 here rather than ai-layer's 4096.** Mesh specialists emit structured proposals with several lines plus reasoning; the tighter cap risks truncating a proposal mid-array, which the runner would then treat as unparseable and degrade. Raise it further if `SpecialistFinished { degraded: true }` correlates with long baskets.

- [ ] **Step 3: Verify the workspace compiles**

Run: `CARGO_INCREMENTAL=0 cargo check --workspace`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add services/omnideliv/
git commit -m "feat(mesh): SSE run endpoint and service wiring

SSE rather than WebSockets — the orchestration window is seconds long and
strictly server-to-client. A client disconnect does not abort the run; the
basket persists so the customer can reopen it."
```

---

## Task 8: Keep partial results at the deadline

Task 6 flagged that a blunt join-timeout discards finished work. Fix it.

**Files:**
- Modify: `services/omnideliv/crates/mesh/src/runner.rs`

- [ ] **Step 1: Write the failing test**

```rust
    /// A worker that finishes inside the deadline must keep its result even
    /// when a sibling is still running when the clock runs out.
    #[tokio::test]
    async fn a_fast_worker_survives_a_slow_siblings_timeout() {
        let fast = tokio::spawn(async {
            SpecialistResult { sub_intent_id: Uuid::new_v4(), lines: vec![proposed()], degraded: false, note: None }
        });
        let slow = tokio::spawn(async {
            tokio::time::sleep(Duration::from_secs(30)).await;
            SpecialistResult { sub_intent_id: Uuid::new_v4(), lines: vec![], degraded: false, note: None }
        });

        let results = join_with_deadline(vec![fast, slow], Duration::from_millis(200)).await;

        assert_eq!(results.len(), 2, "both workers must be accounted for");
        assert_eq!(results.iter().filter(|r| !r.degraded).count(), 1, "the fast worker's lines survive");
        assert_eq!(results.iter().filter(|r| r.degraded).count(), 1, "the slow worker degrades");
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `CARGO_INCREMENTAL=0 cargo test -p omnideliv-mesh runner::a_fast_worker`
Expected: FAIL to compile — `cannot find function 'join_with_deadline' in this scope`.

- [ ] **Step 3: Replace `futures_join` with a deadline-aware join**

```rust
/// Join every handle against a shared deadline.
///
/// A worker that finishes in time keeps its result; one still running when the
/// clock runs out is aborted and degrades. The deadline is shared rather than
/// per-worker so the customer's total wait is bounded regardless of fan-out
/// width — five specialists must not mean five times the wait.
async fn join_with_deadline(
    handles: Vec<tokio::task::JoinHandle<SpecialistResult>>,
    deadline: Duration,
) -> Vec<SpecialistResult> {
    let expires = tokio::time::Instant::now() + deadline;
    let mut out = Vec::with_capacity(handles.len());

    for h in handles {
        let remaining = expires.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(remaining, h).await {
            Ok(Ok(r)) => out.push(r),
            Ok(Err(e)) => {
                tracing::error!(err = %e, "specialist task panicked");
                out.push(degraded("specialist task panicked"));
            }
            Err(_) => {
                tracing::warn!("specialist exceeded the fan-out deadline");
                out.push(degraded("specialist exceeded the deadline"));
            }
        }
    }
    out
}

fn degraded(note: &str) -> SpecialistResult {
    SpecialistResult {
        sub_intent_id: Uuid::nil(),
        lines: vec![],
        degraded: true,
        note: Some(note.to_string()),
    }
}
```

Replace the `tokio::time::timeout(self.config.fanout_deadline, futures_join(handles)).await` block in `fan_out` with:

```rust
        join_with_deadline(handles, self.config.fanout_deadline).await
```

and delete `futures_join`.

> **Aborting the abandoned task.** `tokio::time::timeout` on a `JoinHandle` drops the handle, which does *not* cancel the spawned task — it keeps running detached and will finish its Claude call. That is acceptable: the run is already audited as a child session, so the work is recorded even though nobody reads the result. If detached specialists become a cost problem, hold `AbortHandle`s and abort explicitly.

- [ ] **Step 4: Run the tests**

Run: `CARGO_INCREMENTAL=0 cargo test -p omnideliv-mesh`
Expected: PASS — 16 tests (4 transition, 5 roles, 7 runner).

- [ ] **Step 5: Commit**

```bash
git add services/omnideliv/crates/mesh/src/runner.rs
git commit -m "fix(mesh): keep completed specialist results when the deadline fires

A shared deadline bounds the customer's total wait regardless of fan-out
width, while a worker that finished in time keeps its lines."
```

---

## Definition of done

- [ ] `cargo test -p omnideliv-mesh` — 16 tests pass
- [ ] `cargo test -p logisticos-agent-runtime` — 15 tests pass (13 + 2 parent/child)
- [ ] `cargo check --workspace` — clean
- [ ] `rg -n "assign_courier|charge_customer|generate_invoice" services/omnideliv/crates/mesh/src/roles.rs` returns only the forbidden-tool test
- [ ] A `POST /v1/mesh/run` against a seeded tenant streams `specialist_started` twice for a two-vertical utterance

## Follow-on work this unblocks

1. **Plan 5** — consolidation replaces the placeholder tariff in `compute_flat_fee` and gives the Fleet agent real routing.
2. **Plan 7** — the app consumes `MeshEvent` over SSE to render Screen B.

## Known follow-ups inside this crate

- **`get_customer_profile` returns empty vectors.** The CDP dietary/allergen extension is a spec prerequisite not built by any plan in this set. Until it lands, dietary filtering only works from constraints the customer states in the utterance itself. Tracked here rather than silently returning a shape that looks populated.
- **Abandoned specialists run to completion detached.** See the note in Task 8.
