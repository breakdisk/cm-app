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

/// The handoffs the runner understands.
///
/// There is deliberately no `NeedsUser` variant. The spec and this plan both
/// described one, but nothing ever emits it and nothing ever handles it: the
/// human gate is Screen C, reached from `Completed { needs_review }` at the end
/// of a run, and the mesh has no mid-run question to ask. Writing it here so
/// that Plan 12 could delete it would mean shipping dead code that a reader
/// might reasonably build against in between.
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
    /// Fleet → Concierge.
    Plan { plan: RoutePlan },
    /// Concierge → the commit path. Not an agent action — checkout is a plain
    /// user-initiated transaction.
    Settle { basket_id: Uuid },
}

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

    /// `NeedsUser` is not a variant. If someone reinstates it, this fails and
    /// they have to justify a mid-run human gate that Screen C already covers.
    #[test]
    fn there_is_no_needs_user_transition() {
        let raw = serde_json::json!({
            "type": "needs_user",
            "prompt": { "sub_intent_id": Uuid::nil(), "question": "which?", "options": [] }
        });
        assert!(serde_json::from_value::<MeshTransition>(raw).is_err());
    }

    /// A propose with no `note` is still valid — the note is only required in
    /// spirit when the lines are empty, and serde defaults cover the rest.
    #[test]
    fn a_propose_without_a_note_parses() {
        let raw = serde_json::json!({
            "type": "propose",
            "sub_intent_id": Uuid::nil(),
            "lines": [{
                "vendor_id": Uuid::nil(), "item_id": Uuid::nil(),
                "qty": 2, "unit_price_cents": 12_000
            }]
        });
        let t: MeshTransition = serde_json::from_value(raw).expect("must parse");
        match t {
            MeshTransition::Propose { lines, note, .. } => {
                assert_eq!(lines.len(), 1);
                assert_eq!(lines[0].qty, 2);
                assert!(lines[0].substitutes.is_none());
                assert!(note.is_none());
            }
            other => panic!("expected Propose, got {other:?}"),
        }
    }
}
