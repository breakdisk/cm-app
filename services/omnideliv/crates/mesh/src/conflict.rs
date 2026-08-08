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
    UnverifiableItem { item_id: Uuid },
    BudgetExceeded { limit_cents: i64, actual_cents: i64 },
    /// Raised once whenever a run filtered on allergens at all.
    ///
    /// Not decoration. An allergen filter that silently succeeds teaches a
    /// customer to trust it, and the data behind it is vendor-typed and
    /// unverified. This says so at the point of decision, every time, rather
    /// than once in terms nobody read.
    AllergenDataUnverified { avoided: Vec<String> },
    TemperatureMix { classes: Vec<String> },
    ReadinessSpread { earliest_minutes: i32, latest_minutes: i32 },
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

impl ReconcileContext {
    /// No budget, no allergens, no facts. For callers that only need the merge.
    ///
    /// Note this is *not* a permissive context: with an empty `facts` map every
    /// line is unverifiable and is dropped. Fail-closed is deliberate.
    pub fn empty() -> Self {
        Self { budget_cents: None, avoid_allergens: Vec::new(), facts: HashMap::new() }
    }
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

        if let Some(hit) = facts.allergens.iter().find(|a| avoid.contains(&a.to_lowercase())) {
            conflicts.push(Conflict {
                kind: ConflictKind::AllergenViolation {
                    item_id:  line.item_id,
                    allergen: hit.clone(),
                },
                blocking: true,
                // Says who knows what, and who does not.
                //
                // The old wording — "we left out an item because it contains
                // peanuts" — reads as a statement of fact the platform has
                // checked. It has not: `allergens` is free text a vendor typed,
                // never verified, and absent for any item they did not fill in.
                // A customer with a serious allergy could reasonably take the
                // old sentence as clearance to eat the rest of the basket.
                //
                // Removing the item is still right. Claiming to have vetted the
                // remainder is not.
                description: format!(
                    "We left out an item the shop lists as containing {hit}. Allergen information comes from the shop, not from us — if this matters medically, please confirm with them directly."
                ),
            });
            continue;
        }

        kept.push(line);
    }

    // Everything below is computed from the surviving lines, so a removed
    // allergen line does not inflate the budget or skew the readiness spread.
    let surviving: Vec<&ItemFacts> = kept.iter().filter_map(|l| ctx.facts.get(&l.item_id)).collect();

    if let Some(limit) = ctx.budget_cents {
        let actual: i64 = kept.iter().map(|l| l.unit_price_cents * i64::from(l.qty)).sum();
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
            kind:        ConflictKind::TemperatureMix { classes },
            blocking:    false,
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
                kind:        ConflictKind::ReadinessSpread {
                    earliest_minutes: min,
                    latest_minutes:   max,
                },
                blocking:    false,
                description: format!(
                    "One shop needs about {max} minutes, so your whole order arrives together at that point."
                ),
            });
        }
    }

    // Raised whenever the customer stated an allergen, whether or not anything
    // was removed. "We found nothing" is exactly the case where a customer is
    // most likely to assume the basket was vetted.
    if !avoid.is_empty() {
        let mut avoided: Vec<String> = ctx.avoid_allergens.clone();
        avoided.sort();
        conflicts.push(Conflict {
            kind:        ConflictKind::AllergenDataUnverified { avoided },
            blocking:    false,
            description: "Allergen details come from each shop and are not checked by us. Please confirm anything medically important with the shop before eating."
                .into(),
        });
    }

    (kept, conflicts)
}

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
            item_id:           Uuid::new_v4(),
            allergens:         allergens.iter().map(|s| (*s).to_string()).collect(),
            vertical:          vertical.into(),
            prep_time_minutes: prep,
            price_cents:       price,
        }
    }

    fn line(item_id: Uuid, qty: i32, price: i64) -> ProposedLine {
        ProposedLine {
            vendor_id: Uuid::new_v4(),
            item_id,
            qty,
            unit_price_cents: price,
            substitutes: None,
        }
    }

    fn ctx(budget: Option<i64>, avoid: &[&str], f: HashMap<Uuid, ItemFacts>) -> ReconcileContext {
        ReconcileContext {
            budget_cents:    budget,
            avoid_allergens: avoid.iter().map(|s| (*s).to_string()).collect(),
            facts:           f,
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
        let bad = item(&["peanuts"], "restaurant", 20, 30_000);
        let good = item(&[], "restaurant", 20, 25_000);
        let c = ctx(None, &["peanuts"], facts(vec![bad.clone(), good.clone()]));

        let (kept, conflicts) =
            detect(vec![line(bad.item_id, 1, 30_000), line(good.item_id, 1, 25_000)], &c);

        assert_eq!(kept.len(), 1, "the offending line is removed");
        assert_eq!(kept[0].item_id, good.item_id);

        // Filter by kind rather than index: stating an allergen also raises the
        // unverified-data disclaimer, and a positional assertion would break
        // every time another advisory is added.
        let blocking: Vec<_> = conflicts.iter().filter(|c| c.blocking).collect();
        assert_eq!(blocking.len(), 1);
        assert!(matches!(blocking[0].kind, ConflictKind::AllergenViolation { .. }));
    }

    /// Allergen matching is case-insensitive: vendors type these by hand, and
    /// "Peanuts" must not slip past a filter for "peanuts".
    #[test]
    fn allergen_matching_ignores_case() {
        let bad = item(&["Peanuts"], "restaurant", 20, 30_000);
        let c = ctx(None, &["peanuts"], facts(vec![bad.clone()]));
        let (kept, conflicts) = detect(vec![line(bad.item_id, 1, 30_000)], &c);

        assert!(kept.is_empty());
        assert_eq!(conflicts.iter().filter(|c| c.blocking).count(), 1);
    }

    /// An item the runner could not resolve is dropped, not trusted. Keeping an
    /// unverifiable line is exactly the allergen risk this check exists to
    /// remove — a specialist could name any item id.
    #[test]
    fn an_unresolvable_item_is_dropped() {
        let c = ctx(None, &["peanuts"], facts(vec![]));
        let (kept, conflicts) = detect(vec![line(Uuid::new_v4(), 1, 10_000)], &c);

        assert!(kept.is_empty());
        let blocking: Vec<_> = conflicts.iter().filter(|c| c.blocking).collect();
        assert_eq!(blocking.len(), 1);
        assert!(matches!(blocking[0].kind, ConflictKind::UnverifiableItem { .. }));
    }

    /// The case that matters most. A customer said "no peanuts", nothing was
    /// removed, and they are now most likely to assume the basket was vetted.
    /// It was not — every allergen string came from a shop, unverified.
    #[test]
    fn stating_an_allergen_always_gets_the_disclaimer_even_when_nothing_is_removed() {
        let clean = item(&[], "restaurant", 20, 25_000);
        let c = ctx(None, &["peanuts"], facts(vec![clean.clone()]));
        let (kept, conflicts) = detect(vec![line(clean.item_id, 1, 25_000)], &c);

        assert_eq!(kept.len(), 1, "nothing to remove");
        let d: Vec<_> = conflicts.iter()
            .filter(|c| matches!(c.kind, ConflictKind::AllergenDataUnverified { .. }))
            .collect();
        assert_eq!(d.len(), 1, "silence here reads as 'we checked'");
        assert!(!d[0].blocking, "it informs, it does not remove anything");
    }

    /// A customer who stated no allergens is not shown an allergen disclaimer.
    #[test]
    fn no_stated_allergen_means_no_disclaimer() {
        let a = item(&[], "grocery", 5, 8_000);
        let c = ctx(None, &[], facts(vec![a.clone()]));
        let (_, conflicts) = detect(vec![line(a.item_id, 1, 8_000)], &c);

        assert!(!conflicts.iter()
            .any(|c| matches!(c.kind, ConflictKind::AllergenDataUnverified { .. })));
    }

    /// It names what was asked for, so the message is specific rather than a
    /// generic banner a customer learns to skip.
    #[test]
    fn the_disclaimer_names_the_allergens_the_customer_asked_about() {
        let a = item(&[], "grocery", 5, 8_000);
        let c = ctx(None, &["peanuts", "dairy"], facts(vec![a.clone()]));
        let (_, conflicts) = detect(vec![line(a.item_id, 1, 8_000)], &c);

        let kinds: Vec<_> = conflicts.iter().map(|c| &c.kind).collect();
        assert!(kinds.iter().any(|k| matches!(k,
            ConflictKind::AllergenDataUnverified { avoided } if avoided == &vec!["dairy".to_string(), "peanuts".to_string()])),
            "expected both allergens, sorted; got {kinds:?}");
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
        assert!(matches!(
            conflicts[0].kind,
            ConflictKind::BudgetExceeded { limit_cents: 30_000, actual_cents: 40_000 }
        ));
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
        // Prep times 14 and 5: a 9-minute spread, deliberately under the
        // readiness threshold so this asserts the temperature axis alone. With
        // the obvious 20/5 pair the spread is exactly 15, which also trips
        // ReadinessSpread and makes the count assertion below meaningless.
        let hot = item(&[], "restaurant", 14, 30_000);
        let chilled = item(&[], "grocery", 5, 8_000);
        let c = ctx(None, &[], facts(vec![hot.clone(), chilled.clone()]));
        let (_, conflicts) =
            detect(vec![line(hot.item_id, 1, 30_000), line(chilled.item_id, 1, 8_000)], &c);

        assert_eq!(conflicts.len(), 1);
        assert!(!conflicts[0].blocking);
        assert!(matches!(conflicts[0].kind, ConflictKind::TemperatureMix { .. }));
    }

    /// The threshold is inclusive. Pinned because the obvious test data sits
    /// exactly on it, so an off-by-one here changes behaviour silently.
    #[test]
    fn the_readiness_threshold_is_inclusive() {
        let a = item(&[], "grocery", 5, 8_000);
        let on_boundary = item(&[], "grocery", 5 + READINESS_SPREAD_MINUTES, 6_000);
        let below = item(&[], "grocery", 5 + READINESS_SPREAD_MINUTES - 1, 6_000);

        let c = ctx(None, &[], facts(vec![a.clone(), on_boundary.clone()]));
        let (_, hit) = detect(vec![line(a.item_id, 1, 8_000), line(on_boundary.item_id, 1, 6_000)], &c);
        assert!(hit.iter().any(|c| matches!(c.kind, ConflictKind::ReadinessSpread { .. })),
                "a spread of exactly {READINESS_SPREAD_MINUTES} must report");

        let c = ctx(None, &[], facts(vec![a.clone(), below.clone()]));
        let (_, miss) = detect(vec![line(a.item_id, 1, 8_000), line(below.item_id, 1, 6_000)], &c);
        assert!(!miss.iter().any(|c| matches!(c.kind, ConflictKind::ReadinessSpread { .. })),
                "one minute under must not report");
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
        let (_, conflicts) =
            detect(vec![line(slow.item_id, 1, 30_000), line(fast.item_id, 1, 8_000)], &c);

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
        let ok = item(&[], "grocery", 5, 8_000);
        let c = ctx(Some(10_000), &["peanuts"], facts(vec![bad.clone(), ok.clone()]));
        let (_, conflicts) =
            detect(vec![line(bad.item_id, 1, 40_000), line(ok.item_id, 1, 8_000)], &c);

        assert!(!conflicts.is_empty());
        for k in &conflicts {
            assert!(!k.description.trim().is_empty(), "{:?} has no description", k.kind);
            assert!(!k.description.contains("item_id"), "descriptions must not leak ids");
        }
    }

    /// `ReconcileContext::empty()` is not a bypass. A caller that skips fact
    /// resolution drops every line rather than passing unverified ones through.
    #[test]
    fn an_empty_context_drops_everything_rather_than_trusting_it() {
        let (kept, conflicts) = detect(vec![line(Uuid::new_v4(), 1, 10_000)], &ReconcileContext::empty());
        assert!(kept.is_empty());
        assert!(conflicts[0].blocking);
    }
}
