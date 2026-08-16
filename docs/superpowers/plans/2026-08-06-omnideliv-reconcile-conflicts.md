# OmniDeliv Reconcile Conflicts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the conflict list the spec's phase 3 promises and the implementation never produced — so "conflicts surface deterministically in reconcile" becomes true rather than aspirational.

> **EXECUTED 2026-08-07** — commits `607e521c` (Tasks 1–4) and `35edf132` (Task 5).
> Two places where this plan was wrong, corrected in the implementation and
> below, so nobody rebuilds from the original text:
>
> 1. **Task 1's temperature-mix test data was self-contradicting.** It used
>    restaurant prep 20 and grocery prep 5 — a spread of exactly 15, which also
>    trips the `>= READINESS_SPREAD_MINUTES` check — while asserting
>    `conflicts.len() == 1`. The shipped test uses prep 14 to isolate the axis
>    under test, and adds `the_readiness_threshold_is_inclusive` to pin the
>    boundary, precisely because the obvious data sits on it.
> 2. **Task 3 read constraints off the planned workers; it must read `specs`.**
>    Pharmacy, florist and retail get no slice-one specialist, so an allergen
>    stated while asking about pharmacy items would have been silently dropped
>    and never applied to the restaurant lines. An allergen is a fact about the
>    person, not the sub-intent. Covered by
>    `a_constraint_on_an_unhandled_vertical_still_counts`, which asserts the
>    precondition before the behaviour.
>
> Also: `MeshRunner` gained `catalog: Arc<dyn MeshCatalog>` directly rather than
> reaching it through `ToolBox`. Verification must not travel the model's tool
> surface. Task 5 landed as migration `0012_basket_conflicts.sql` plus a
> `set_conflicts` repository method that deliberately does not bump the
> optimistic-lock version.

**Architecture:** `reconcile` stays a pure function and gains a context carrying **catalog facts the runner resolved server-side**, never what the specialist reported. Allergen violations remove the offending line and are reported; everything else is advisory and shown to the customer. The linear pipeline is kept — the topology question is deferred until a slice genuinely needs a cycle.

---

## Why this plan exists

Spec §4.2 phase 3 declares reconcile's output to be "Merged basket + conflict list", and the design text claims conflicts "surface here deterministically, not as a race." What Plan 4 built:

```rust
pub fn reconcile_results(results: Vec<SpecialistResult>) -> MeshOutcome {
    let degraded_count = results.iter().filter(|r| r.degraded).count();
    let total_failure  = !results.is_empty() && degraded_count == results.len();
    let lines = results.into_iter().filter(|r| !r.degraded)
        .map(|r| (r.sub_intent_id, r.lines)).collect();
    MeshOutcome { lines, degraded_count, total_failure }
}
```

It merges and counts degradations. No budget check, no timing check, no allergen check. Determinism was achieved through the single-writer design; **conflict detection was never built**. Plan 9's `ConstraintDetected` event compounds this — it fires on a hardcoded heuristic (*restaurant plus any other vertical*) rather than on anything the specialists actually returned, so Screen B's "Unified Constraint Display" currently renders a guess dressed as a computation.

`MeshTransition::NeedsUser` is also dead: defined in the spec and Plan 4, never emitted, never handled. The human gate is Screen C, driven by `Completed { needs_review }` at the end of a run — there is no mid-run question the mesh needs to ask. Task 4 deletes it rather than inventing a use.

---

## Dependencies

**Requires Plans 4 and 9.** Verify:

```bash
CARGO_INCREMENTAL=0 cargo check -p omnideliv-mesh
```

---

## Task 1: Conflict types and detection

**Files:**
- Create: `services/omnideliv/crates/mesh/src/conflict.rs`

- [ ] **Step 1: Write the failing test**

```rust
// services/omnideliv/crates/mesh/src/conflict.rs — tests block
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use uuid::Uuid;

    fn facts(items: Vec<ItemFacts>) -> HashMap<Uuid, ItemFacts> {
        items.into_iter().map(|f| (f.item_id, f)).collect()
    }

    fn item(allergens: &[&str], vertical: &str, prep: i32, price: i64) -> ItemFacts {
        ItemFacts {
            item_id: Uuid::new_v4(),
            allergens: allergens.iter().map(|s| s.to_string()).collect(),
            vertical: vertical.into(),
            prep_time_minutes: prep,
            price_cents: price,
        }
    }

    fn line(item_id: Uuid, qty: i32, price: i64) -> ProposedLine {
        ProposedLine { vendor_id: Uuid::new_v4(), item_id, qty, unit_price_cents: price, substitutes: None }
    }

    fn ctx(budget: Option<i64>, avoid: &[&str], f: HashMap<Uuid, ItemFacts>) -> ReconcileContext {
        ReconcileContext {
            budget_cents: budget,
            avoid_allergens: avoid.iter().map(|s| s.to_string()).collect(),
            facts: f,
        }
    }

    #[test]
    fn a_clean_basket_has_no_conflicts() {
        let a = item(&[], "grocery", 5, 10_000);
        let c = ctx(None, &[], facts(vec![a.clone()]));
        let (kept, conflicts) = detect(vec![line(a.item_id, 1, 10_000)], &c);

        assert_eq!(kept.len(), 1);
        assert!(conflicts.is_empty());
    }

    /// The highest-severity conflict. A customer who must avoid peanuts should
    /// never see a peanut item in their basket — so the line is removed, not
    /// flagged for them to notice.
    #[test]
    fn an_allergen_violation_removes_the_line() {
        let bad  = item(&["peanuts"], "restaurant", 20, 30_000);
        let good = item(&[], "restaurant", 20, 25_000);
        let c = ctx(None, &["peanuts"], facts(vec![bad.clone(), good.clone()]));

        let (kept, conflicts) = detect(
            vec![line(bad.item_id, 1, 30_000), line(good.item_id, 1, 25_000)], &c,
        );

        assert_eq!(kept.len(), 1, "the offending line is removed");
        assert_eq!(kept[0].item_id, good.item_id);
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts[0].blocking);
        assert!(matches!(conflicts[0].kind, ConflictKind::AllergenViolation { .. }));
    }

    /// Allergen matching is case-insensitive: vendors type these by hand, and
    /// "Peanuts" must not slip past a filter for "peanuts".
    #[test]
    fn allergen_matching_ignores_case() {
        let bad = item(&["Peanuts"], "restaurant", 20, 30_000);
        let c = ctx(None, &["peanuts"], facts(vec![bad.clone()]));
        let (kept, conflicts) = detect(vec![line(bad.item_id, 1, 30_000)], &c);

        assert!(kept.is_empty());
        assert_eq!(conflicts.len(), 1);
    }

    /// An item the runner could not resolve is dropped, not trusted. Keeping an
    /// unverifiable line is exactly the allergen risk this check exists to
    /// remove — a specialist could name any item id.
    #[test]
    fn an_unresolvable_item_is_dropped() {
        let c = ctx(None, &["peanuts"], facts(vec![]));
        let (kept, conflicts) = detect(vec![line(Uuid::new_v4(), 1, 10_000)], &c);

        assert!(kept.is_empty());
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts[0].blocking);
        assert!(matches!(conflicts[0].kind, ConflictKind::UnverifiableItem { .. }));
    }

    /// Budget is advisory, not blocking: dropping lines to fit would mean
    /// choosing for the customer which part of their order to lose.
    #[test]
    fn exceeding_the_budget_is_reported_without_dropping_anything() {
        let a = item(&[], "restaurant", 20, 40_000);
        let c = ctx(Some(30_000), &[], facts(vec![a.clone()]));
        let (kept, conflicts) = detect(vec![line(a.item_id, 1, 40_000)], &c);

        assert_eq!(kept.len(), 1, "nothing is dropped for budget");
        assert_eq!(conflicts.len(), 1);
        assert!(!conflicts[0].blocking);
        assert!(matches!(conflicts[0].kind, ConflictKind::BudgetExceeded { limit_cents: 30_000, actual_cents: 40_000 }));
    }

    #[test]
    fn a_basket_within_budget_reports_nothing() {
        let a = item(&[], "grocery", 5, 10_000);
        let c = ctx(Some(30_000), &[], facts(vec![a.clone()]));
        let (_, conflicts) = detect(vec![line(a.item_id, 2, 10_000)], &c);
        assert!(conflicts.is_empty(), "20000 is within 30000");
    }

    /// Computed from what is actually in the basket, not guessed from which
    /// verticals were asked about — a grocery-only basket of ambient tins is
    /// not a temperature mix.
    #[test]
    fn a_hot_and_chilled_basket_reports_a_temperature_mix() {
        // prep 14, not 20: a 20/5 pair is a spread of exactly 15, which also
        // trips ReadinessSpread and makes the count assertion below meaningless.
        let hot     = item(&[], "restaurant", 14, 30_000);
        let chilled = item(&[], "grocery", 5, 8_000);
        let c = ctx(None, &[], facts(vec![hot.clone(), chilled.clone()]));
        let (_, conflicts) = detect(
            vec![line(hot.item_id, 1, 30_000), line(chilled.item_id, 1, 8_000)], &c,
        );

        assert_eq!(conflicts.len(), 1);
        assert!(!conflicts[0].blocking);
        assert!(matches!(conflicts[0].kind, ConflictKind::TemperatureMix { .. }));
    }

    #[test]
    fn a_single_vertical_basket_reports_no_temperature_mix() {
        let a = item(&[], "grocery", 5, 8_000);
        let b = item(&[], "grocery", 5, 6_000);
        let c = ctx(None, &[], facts(vec![a.clone(), b.clone()]));
        let (_, conflicts) = detect(vec![line(a.item_id, 1, 8_000), line(b.item_id, 1, 6_000)], &c);
        assert!(conflicts.is_empty());
    }

    /// A wide readiness spread means something waits. Worth telling the
    /// customer, because it is why their food may arrive later than the fastest
    /// item suggests.
    #[test]
    fn a_wide_readiness_spread_is_reported() {
        let slow = item(&[], "restaurant", 45, 30_000);
        let fast = item(&[], "grocery", 5, 8_000);
        let c = ctx(None, &[], facts(vec![slow.clone(), fast.clone()]));
        let (_, conflicts) = detect(
            vec![line(slow.item_id, 1, 30_000), line(fast.item_id, 1, 8_000)], &c,
        );

        assert!(conflicts.iter().any(|c| matches!(c.kind, ConflictKind::ReadinessSpread { .. })));
    }

    #[test]
    fn a_narrow_readiness_spread_is_not_reported() {
        let a = item(&[], "grocery", 5, 8_000);
        let b = item(&[], "grocery", 8, 6_000);
        let c = ctx(None, &[], facts(vec![a.clone(), b.clone()]));
        let (_, conflicts) = detect(vec![line(a.item_id, 1, 8_000), line(b.item_id, 1, 6_000)], &c);
        assert!(!conflicts.iter().any(|c| matches!(c.kind, ConflictKind::ReadinessSpread { .. })));
    }

    /// Every conflict carries text a customer can read. A conflict list only
    /// engineers can interpret cannot be rendered on Screen C.
    #[test]
    fn every_conflict_carries_customer_facing_text() {
        let bad = item(&["peanuts"], "restaurant", 45, 40_000);
        let ok  = item(&[], "grocery", 5, 8_000);
        let c = ctx(Some(10_000), &["peanuts"], facts(vec![bad.clone(), ok.clone()]));
        let (_, conflicts) = detect(vec![line(bad.item_id, 1, 40_000), line(ok.item_id, 1, 8_000)], &c);

        assert!(!conflicts.is_empty());
        for k in &conflicts {
            assert!(!k.description.trim().is_empty(), "{:?} has no description", k.kind);
            assert!(!k.description.contains("item_id"), "descriptions must not leak ids");
        }
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `CARGO_INCREMENTAL=0 cargo test -p omnideliv-mesh conflict::`
Expected: FAIL to compile — `cannot find type 'ItemFacts' in this scope`.

- [ ] **Step 3: Implement**

```rust
// services/omnideliv/crates/mesh/src/conflict.rs
//! Reconcile-phase conflict detection.
//!
//! Every check runs against facts the **runner resolved from the catalog**, not
//! against what a specialist reported. That distinction is the whole point: a
//! model asked not to propose allergens might still do so, and verifying its
//! output against its own claims would verify nothing. It is the same rule the
//! RBAC gate applies to tool calls, applied to tool results.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::transition::ProposedLine;

/// Catalog truth about one item, resolved server-side.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemFacts {
    pub item_id:           Uuid,
    pub allergens:         Vec<String>,
    pub vertical:          String,
    pub prep_time_minutes: i32,
    pub price_cents:       i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConflictKind {
    /// A proposed item carries an allergen the customer must avoid.
    AllergenViolation { item_id: Uuid, allergen: String },
    /// The runner could not resolve the item in the catalog.
    UnverifiableItem  { item_id: Uuid },
    BudgetExceeded    { limit_cents: i64, actual_cents: i64 },
    TemperatureMix    { classes: Vec<String> },
    ReadinessSpread   { earliest_minutes: i32, latest_minutes: i32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Conflict {
    pub kind: ConflictKind,
    /// Blocking conflicts have already had their line removed. Advisory ones
    /// are shown to the customer, who decides.
    pub blocking: bool,
    /// Customer-facing. Rendered directly on Screen C, so no ids and no jargon.
    pub description: String,
}

pub struct ReconcileContext {
    pub budget_cents:    Option<i64>,
    pub avoid_allergens: Vec<String>,
    /// Catalog facts, keyed by item. Resolved by the runner before reconcile.
    pub facts: HashMap<Uuid, ItemFacts>,
}

/// A readiness gap this wide means something sits waiting.
const READINESS_SPREAD_MINUTES: i32 = 15;

fn temperature_class(vertical: &str) -> &'static str {
    match vertical {
        "restaurant" => "hot",
        "grocery" | "florist" => "chilled",
        _ => "ambient",
    }
}

/// Merge-time verification.
///
/// Returns the lines that survive and the conflicts found. Blocking conflicts
/// remove their line: a customer who must avoid peanuts should never see a
/// peanut item in their basket at all, and an item the runner could not resolve
/// cannot be verified, so keeping it would reintroduce exactly the risk this
/// check removes.
///
/// Advisory conflicts drop nothing. Trimming a basket to fit a budget would
/// mean choosing for the customer which part of their order to lose.
pub fn detect(
    lines: Vec<ProposedLine>,
    ctx: &ReconcileContext,
) -> (Vec<ProposedLine>, Vec<Conflict>) {
    let avoid: HashSet<String> = ctx.avoid_allergens.iter().map(|a| a.to_lowercase()).collect();

    let mut kept = Vec::with_capacity(lines.len());
    let mut conflicts = Vec::new();

    for line in lines {
        let Some(facts) = ctx.facts.get(&line.item_id) else {
            conflicts.push(Conflict {
                kind: ConflictKind::UnverifiableItem { item_id: line.item_id },
                blocking: true,
                description: "We couldn't confirm one of the items, so we've left it out.".into(),
            });
            continue;
        };

        if let Some(hit) = facts
            .allergens
            .iter()
            .find(|a| avoid.contains(&a.to_lowercase()))
        {
            conflicts.push(Conflict {
                kind: ConflictKind::AllergenViolation {
                    item_id: line.item_id,
                    allergen: hit.clone(),
                },
                blocking: true,
                description: format!("We left out an item because it contains {hit}."),
            });
            continue;
        }

        kept.push(line);
    }

    // Everything below is computed from the surviving lines, so a removed
    // allergen line does not inflate the budget or skew the readiness spread.
    let surviving: Vec<&ItemFacts> = kept.iter().filter_map(|l| ctx.facts.get(&l.item_id)).collect();

    if let Some(limit) = ctx.budget_cents {
        let actual: i64 = kept
            .iter()
            .map(|l| l.unit_price_cents * l.qty as i64)
            .sum();
        if actual > limit {
            conflicts.push(Conflict {
                kind: ConflictKind::BudgetExceeded { limit_cents: limit, actual_cents: actual },
                blocking: false,
                description: format!(
                    "This comes to ₱{:.2}, which is over the ₱{:.2} you mentioned.",
                    actual as f64 / 100.0,
                    limit as f64 / 100.0
                ),
            });
        }
    }

    let classes: Vec<String> = {
        let mut c: Vec<&str> = surviving.iter().map(|f| temperature_class(&f.vertical)).collect();
        c.sort_unstable();
        c.dedup();
        c.into_iter().map(str::to_owned).collect()
    };
    if classes.len() > 1 {
        conflicts.push(Conflict {
            kind: ConflictKind::TemperatureMix { classes: classes.clone() },
            blocking: false,
            description: "Your order mixes hot and cold items — we'll collect the hot food last."
                .into(),
        });
    }

    if let (Some(min), Some(max)) = (
        surviving.iter().map(|f| f.prep_time_minutes).min(),
        surviving.iter().map(|f| f.prep_time_minutes).max(),
    ) {
        if max - min >= READINESS_SPREAD_MINUTES {
            conflicts.push(Conflict {
                kind: ConflictKind::ReadinessSpread { earliest_minutes: min, latest_minutes: max },
                blocking: false,
                description: format!(
                    "One shop needs about {max} minutes, so your whole order arrives together at that point."
                ),
            });
        }
    }

    (kept, conflicts)
}
```

Add `pub mod conflict;` to `lib.rs` and re-export `Conflict`, `ConflictKind`, `ItemFacts`, `ReconcileContext`.

- [ ] **Step 4: Run the tests**

Run: `CARGO_INCREMENTAL=0 cargo test -p omnideliv-mesh conflict::`
Expected: PASS — 11 passed.

- [ ] **Step 5: Commit**

```bash
git add services/omnideliv/crates/mesh/src/conflict.rs services/omnideliv/crates/mesh/src/lib.rs
git commit -m "feat(mesh): reconcile-phase conflict detection

Every check runs against catalog facts the runner resolved, never against what
the specialist reported — verifying a model's output against its own claims
verifies nothing. Blocking conflicts remove their line: an unverifiable or
allergen-carrying item must not reach the basket. Advisory conflicts drop
nothing, because trimming to fit a budget means choosing for the customer
which part of their order to lose."
```

---

## Task 2: Wire it into reconcile

**Files:**
- Modify: `services/omnideliv/crates/mesh/src/runner.rs`

- [ ] **Step 1: Write the failing test**

```rust
    #[tokio::test]
    async fn reconcile_reports_conflicts_and_drops_blocked_lines() {
        let si = Uuid::new_v4();
        let safe   = Uuid::new_v4();
        let peanut = Uuid::new_v4();

        let mut facts = std::collections::HashMap::new();
        facts.insert(safe, ItemFacts {
            item_id: safe, allergens: vec![], vertical: "restaurant".into(),
            prep_time_minutes: 20, price_cents: 25_000,
        });
        facts.insert(peanut, ItemFacts {
            item_id: peanut, allergens: vec!["peanuts".into()], vertical: "restaurant".into(),
            prep_time_minutes: 20, price_cents: 30_000,
        });

        let ctx = ReconcileContext {
            budget_cents: None,
            avoid_allergens: vec!["peanuts".into()],
            facts,
        };

        let outcome = reconcile_results(
            vec![SpecialistResult {
                sub_intent_id: si,
                lines: vec![
                    ProposedLine { vendor_id: Uuid::new_v4(), item_id: safe,   qty: 1, unit_price_cents: 25_000, substitutes: None },
                    ProposedLine { vendor_id: Uuid::new_v4(), item_id: peanut, qty: 1, unit_price_cents: 30_000, substitutes: None },
                ],
                degraded: false,
                note: None,
            }],
            &ctx,
        );

        let lines: Vec<_> = outcome.lines.iter().flat_map(|(_, l)| l).collect();
        assert_eq!(lines.len(), 1, "the peanut line is removed before the basket");
        assert_eq!(lines[0].item_id, safe);
        assert_eq!(outcome.conflicts.len(), 1);
        assert!(outcome.conflicts[0].blocking);
    }

    /// A degraded specialist's lines were never trusted; its absence must not
    /// also produce phantom conflicts.
    #[tokio::test]
    async fn a_degraded_specialist_contributes_no_conflicts() {
        let ctx = ReconcileContext {
            budget_cents: Some(1), avoid_allergens: vec![],
            facts: std::collections::HashMap::new(),
        };
        let outcome = reconcile_results(
            vec![SpecialistResult {
                sub_intent_id: Uuid::new_v4(), lines: vec![], degraded: true, note: None,
            }],
            &ctx,
        );
        assert!(outcome.conflicts.is_empty());
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `CARGO_INCREMENTAL=0 cargo test -p omnideliv-mesh runner::reconcile`
Expected: FAIL to compile — `reconcile_results` takes 1 argument, 2 supplied.

- [ ] **Step 3: Implement**

Add `conflicts: Vec<Conflict>` to `MeshOutcome`, then:

```rust
/// Phase 3. The single writer: merges results, verifies them against catalog
/// facts, and decides whether the run produced anything usable.
pub fn reconcile_results(
    results: Vec<SpecialistResult>,
    ctx: &ReconcileContext,
) -> MeshOutcome {
    let degraded_count = results.iter().filter(|r| r.degraded).count();
    let total_failure = !results.is_empty() && degraded_count == results.len();

    let mut lines = Vec::new();
    let mut conflicts = Vec::new();

    // Only non-degraded results are verified. A degraded specialist's lines were
    // never trusted, so they must not produce conflicts either.
    for r in results.into_iter().filter(|r| !r.degraded) {
        let (kept, mut found) = crate::conflict::detect(r.lines, ctx);
        lines.push((r.sub_intent_id, kept));
        conflicts.append(&mut found);
    }

    MeshOutcome { lines, conflicts, degraded_count, total_failure }
}
```

- [ ] **Step 4: Fix the three call sites Plan 4 already wrote**

The second parameter breaks `a_specialist_that_times_out_degrades_only_its_own_vertical`, `every_specialist_failing_is_a_total_failure` and `an_honest_empty_result_is_not_a_failure` in Plan 4 Task 6. None of them cares about conflicts, so give each an empty context via a helper:

```rust
    /// No budget, no allergens, no facts — for the tests that assert
    /// degradation behaviour rather than verification.
    fn no_constraints() -> ReconcileContext {
        ReconcileContext {
            budget_cents: None,
            avoid_allergens: vec![],
            facts: std::collections::HashMap::new(),
        }
    }
```

then pass `&no_constraints()` as the second argument at each site.

> **One behavioural change to notice.** With an empty `facts` map, any line in those tests now resolves to `UnverifiableItem` and is dropped. `a_specialist_that_times_out_degrades_only_its_own_vertical` asserts `outcome.lines.len() == 1`, which will now be 0. That is correct new behaviour, not a regression — fail-closed is the point. Update the assertion to check the *result count* rather than the line count:
>
> ```rust
>         assert_eq!(outcome.lines.len(), 1, "one healthy result survives");
>         assert_eq!(outcome.degraded_count, 1);
> ```
>
> `outcome.lines` is `Vec<(Uuid, Vec<ProposedLine>)>` — one entry per non-degraded specialist — so this assertion already means what the test intended. Verify that reading before changing anything.

- [ ] **Step 5: Run the tests and commit**

Run: `CARGO_INCREMENTAL=0 cargo test -p omnideliv-mesh`
Expected: PASS — every Plan 4 test still green.

```bash
git add services/omnideliv/crates/mesh/src/runner.rs
git commit -m "feat(mesh): reconcile returns the conflict list the spec promised

Only non-degraded results are verified — a degraded specialist's lines were
never trusted, so they must not produce phantom conflicts either."
```

---

## Task 3: Resolve facts and emit real constraints

Replaces Plan 9's hardcoded vertical-membership heuristic with a computation over what the specialists actually returned.

**Files:**
- Modify: `services/omnideliv/crates/mesh/src/tools.rs`, `src/runner.rs`

- [ ] **Step 1: Extend the catalog port**

```rust
// MeshCatalog — add:
    /// Resolve catalog truth for the items a specialist proposed.
    ///
    /// The runner calls this between fan-out and reconcile. Facts must come
    /// from here rather than from the specialist's own output, or the
    /// verification step verifies the model against itself.
    async fn resolve_facts(
        &self,
        tenant_id: Uuid,
        item_ids: &[Uuid],
    ) -> anyhow::Result<Vec<crate::conflict::ItemFacts>>;
```

- [ ] **Step 2: Use it in `run()`**

Replace the hardcoded constraint block in `MeshRunner::run` with:

```rust
        // Resolve catalog truth for everything proposed, then verify against it.
        let proposed_ids: Vec<Uuid> = results
            .iter()
            .filter(|r| !r.degraded)
            .flat_map(|r| r.lines.iter().map(|l| l.item_id))
            .collect();

        let facts = self
            .catalog
            .resolve_facts(tenant_id.inner(), &proposed_ids)
            .await
            .unwrap_or_else(|e| {
                // Resolving nothing means every line becomes UnverifiableItem
                // and is dropped. Failing closed is correct here: the check
                // exists to keep allergens out of baskets, so a lookup failure
                // must not become a bypass.
                tracing::error!(err = %e, "catalog fact resolution failed; failing closed");
                Vec::new()
            });

        let ctx = ReconcileContext {
            // `specs`, not the planned `workers`: a vertical with no slice-one
            // specialist still contributes its constraints.
            budget_cents: constraints_budget(&specs),
            avoid_allergens: constraints_allergens(&specs),
            facts: facts.into_iter().map(|f| (f.item_id, f)).collect(),
        };

        let outcome = reconcile_results(results, &ctx);

        // ... existing total_failure check and basket writes ...

        // One event per conflict, in the customer's words. This replaces the
        // vertical-membership guess: a grocery-only basket of ambient tins no
        // longer claims a temperature constraint it does not have.
        for c in &outcome.conflicts {
            let _ = events.send(MeshEvent::ConstraintDetected {
                description: c.description.clone(),
            }).await;
        }
```

with two small helpers reading the constraints the Concierge attached to each sub-intent:

```rust
/// Tightest stated budget across sub-intents. The customer said one number for
/// the whole order, so the strictest reading is the safe one.
fn constraints_budget(specs: &[SubIntentSpec]) -> Option<i64> {
    specs
        .iter()
        .filter_map(|s| s.constraints.get("budget_cents").and_then(|v| v.as_i64()))
        .min()
}

/// Union of every allergen mentioned anywhere. An allergen stated for one
/// vertical applies to the person, not the vertical.
fn constraints_allergens(specs: &[SubIntentSpec]) -> Vec<String> {
    let mut out: Vec<String> = specs
        .iter()
        .filter_map(|s| s.constraints.get("avoid_allergens").and_then(|v| v.as_array()))
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    out.sort();
    out.dedup();
    out
}
```

- [ ] **Step 3: Write the helper tests**

```rust
    #[test]
    fn the_tightest_budget_wins_across_sub_intents() {
        let specs = vec![
            SubIntentSpec { vertical: "restaurant".into(), vendor_hint: None, raw_text: "".into(),
                            constraints: serde_json::json!({ "budget_cents": 50_000 }) },
            SubIntentSpec { vertical: "grocery".into(), vendor_hint: None, raw_text: "".into(),
                            constraints: serde_json::json!({ "budget_cents": 30_000 }) },
        ];
        assert_eq!(constraints_budget(&specs), Some(30_000));
    }

    /// An allergen stated about dinner applies to the groceries too — it is a
    /// fact about the person, not the sub-intent.
    #[test]
    fn allergens_are_unioned_across_sub_intents() {
        let specs = vec![
            SubIntentSpec { vertical: "restaurant".into(), vendor_hint: None, raw_text: "".into(),
                            constraints: serde_json::json!({ "avoid_allergens": ["peanuts"] }) },
            SubIntentSpec { vertical: "grocery".into(), vendor_hint: None, raw_text: "".into(),
                            constraints: serde_json::json!({ "avoid_allergens": ["dairy"] }) },
        ];
        assert_eq!(constraints_allergens(&specs), vec!["dairy", "peanuts"]);
    }

    #[test]
    fn absent_constraints_yield_none_and_empty() {
        let specs = vec![SubIntentSpec {
            vertical: "grocery".into(), vendor_hint: None, raw_text: "".into(),
            constraints: serde_json::json!({}),
        }];
        assert_eq!(constraints_budget(&specs), None);
        assert!(constraints_allergens(&specs).is_empty());
    }
```

- [ ] **Step 4: Run and commit**

Run: `CARGO_INCREMENTAL=0 cargo test -p omnideliv-mesh`
Expected: PASS.

```bash
git add services/omnideliv/crates/mesh/
git commit -m "feat(mesh): resolve catalog facts and emit computed constraints

Replaces the vertical-membership heuristic with a computation over what the
specialists actually returned — a grocery-only basket of ambient tins no
longer claims a temperature constraint it does not have. A fact-resolution
failure fails closed: every line becomes unverifiable and is dropped, because
a lookup failure must not become an allergen bypass."
```

---

## Task 4: Delete `NeedsUser` — ALREADY SATISFIED

> **Nothing to do.** Plan 4 never wrote `NeedsUser` or `UserPrompt`: writing a
> variant in one plan so a later plan can delete it means shipping dead code
> that a reader may build against in between. `transition.rs` carries a test
> (`there_is_no_needs_user_transition`) asserting the variant does not parse,
> so reinstating it fails loudly. Verify with the check below and move on.

**Files:**
- Modify: `services/omnideliv/crates/mesh/src/transition.rs`, `docs/superpowers/specs/2026-08-06-omnideliv-ai-design.md`

- [ ] **Step 1: Confirm it is unused**

```bash
rg -n "NeedsUser|UserPrompt" services/omnideliv apps/omnideliv-app
```

Expected: hits only in the enum definition and the `UserPrompt` struct. Any other hit means something started using it — stop and read that first.

- [ ] **Step 2: Delete it**

Remove the `NeedsUser` variant and the `UserPrompt` struct from `transition.rs`, and update the spec's §4.3 transition table with a note:

```markdown
> **`NeedsUser` was removed.** The human gate is Screen C, reached via
> `Completed { needs_review }` at the end of a run — there is no mid-run
> question the mesh needs to ask, so a pause/resume transition had no caller.
> Reintroduce it only if a slice genuinely needs to block mid-run, which would
> also be the point at which the pipeline needs to become a graph.
```

- [ ] **Step 3: Verify and commit**

Run: `CARGO_INCREMENTAL=0 cargo test -p omnideliv-mesh`
Expected: PASS.

```bash
git add services/omnideliv/crates/mesh/src/transition.rs docs/superpowers/specs/
git commit -m "refactor(mesh): remove the unused NeedsUser transition

The human gate is Screen C via Completed { needs_review }; there is no mid-run
question the mesh asks, so the variant had no caller. Deleting beats inventing
a use for it."
```

---

## Task 5: Show conflicts to the customer

**Files:**
- Modify: `apps/omnideliv-app/app/review.tsx`, `src/api/orders.ts`

- [ ] **Step 1: Carry conflicts through to Screen C**

`ConstraintDetected` events already arrive on Screen B, but a customer who taps through quickly will miss them. `GET /v1/omnideliv/baskets/:id` should also return the conflicts recorded for the run so Screen C can restate them at the point of decision.

Persist them on the basket (a `conflicts JSONB` column on `omnideliv.baskets`, written by the mesh alongside the deltas), and extend `BasketView`:

```ts
export interface BasketConflict {
  blocking: boolean;
  description: string;
}

export interface BasketView {
  id: string;
  status: string;
  goods_total_cents: number;
  lines_awaiting_review: number;
  conflicts: BasketConflict[];
}
```

- [ ] **Step 2: Render them**

In `app/review.tsx`, above the totals — the same placement rule as the substitution card, because these are things the customer needs before deciding, not after:

```tsx
        {basket.conflicts.length > 0 && (
          <View style={{ gap: 8 }}>
            {basket.conflicts.map((c, i) => (
              <View
                key={i}
                style={{
                  borderLeftWidth: 2,
                  borderLeftColor: c.blocking ? theme.red : theme.amber,
                  backgroundColor: c.blocking ? "rgba(255,59,92,0.07)" : "rgba(255,171,0,0.07)",
                  borderRadius: theme.radius.sm,
                  padding: 11,
                }}
              >
                <Text style={{ color: c.blocking ? theme.red : theme.amber, fontSize: 9.5, letterSpacing: 1, marginBottom: 4 }}>
                  {/* A blocking conflict already changed the basket, so it is
                      stated as done rather than as something to decide. */}
                  {c.blocking ? "WE CHANGED SOMETHING" : "WORTH KNOWING"}
                </Text>
                <Text style={{ color: "rgba(255,255,255,0.82)", fontSize: 12 }}>
                  {c.description}
                </Text>
              </View>
            ))}
          </View>
        )}
```

- [ ] **Step 3: Verify and commit**

```bash
cd apps/omnideliv-app && npx tsc --noEmit && npx jest
```

```bash
git add apps/omnideliv-app/ services/omnideliv/
git commit -m "feat(omnideliv-app): surface reconcile conflicts on Screen C

Restated at the point of decision rather than only on Screen B, which a
customer tapping through quickly never reads. A blocking conflict is phrased
as something already done, because the line is gone by the time they see it."
```

---

## Definition of done

- [ ] `cargo test -p omnideliv-mesh` — 32 tests pass (19 from Plan 9, plus 11 conflict and 2 reconcile, plus 3 helper)
- [ ] `rg -n "NeedsUser" services/ apps/` returns nothing
- [ ] `rg -n "restaurant.*verticals.len\(\) > 1" services/omnideliv/crates/mesh/` returns nothing — the heuristic is gone
- [ ] A run proposing an item carrying a stated allergen drops that line and reports it
- [ ] A catalog resolution failure drops every line rather than passing them through

## What this deliberately does not do

- **No conflict→replan cycle.** A budget overrun is reported, not resolved by re-running specialists with a tighter constraint. That is the cycle a pipeline cannot express, and building it is the point at which the topology should become an explicit state machine rather than an `async fn`. Deferred until a slice needs it.
- **No per-item temperature class.** `temperature_class` maps from vertical, so a grocery basket of ambient tins still reads as chilled. Fixing it is a `temperature_class` column on `catalog_items` — a catalog change, tracked in Plan 5's follow-ups.
