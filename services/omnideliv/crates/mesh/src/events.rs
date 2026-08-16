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

impl MeshEvent {
    /// Is this the last event of a run? The SSE endpoint closes the stream
    /// after one, and the client stops waiting.
    ///
    /// A `match` rather than a boolean field so adding a terminal variant
    /// forces this decision to be made rather than defaulted.
    pub fn is_terminal(&self) -> bool {
        match self {
            MeshEvent::Completed { .. } | MeshEvent::Failed { .. } => true,
            MeshEvent::IntentParsed { .. }
            | MeshEvent::SpecialistStarted { .. }
            | MeshEvent::SpecialistProgress { .. }
            | MeshEvent::SpecialistFinished { .. }
            | MeshEvent::ConstraintDetected { .. }
            | MeshEvent::RoutePlanned { .. } => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The client keys its cards on the tagged `event` field, and the mobile
    /// app decodes these by name. Renaming a variant is a wire break, so the
    /// names are pinned here rather than left to inference.
    #[test]
    fn events_serialise_with_a_snake_case_tag() {
        let e = MeshEvent::SpecialistFinished {
            sub_intent_id: Uuid::nil(),
            lines_added: 3,
            degraded: false,
            note: None,
        };
        let v = serde_json::to_value(&e).expect("serialise");
        assert_eq!(v["event"], "specialist_finished");
        assert_eq!(v["lines_added"], 3);
    }

    #[test]
    fn only_completed_and_failed_end_the_stream() {
        assert!(MeshEvent::Completed { basket_id: Uuid::nil(), needs_review: 0 }.is_terminal());
        assert!(MeshEvent::Failed { reason: "no vendors".into() }.is_terminal());

        assert!(!MeshEvent::IntentParsed { sub_intent_count: 2 }.is_terminal());
        assert!(!MeshEvent::SpecialistFinished {
            sub_intent_id: Uuid::nil(), lines_added: 0, degraded: true, note: None,
        }.is_terminal(), "a degraded specialist must not end the run — that is the whole point");
    }

    /// A degraded specialist is reported, not hidden. Screen B shows the card
    /// greyed with its note; the order continues without that vertical.
    #[test]
    fn a_degraded_finish_carries_its_reason_to_the_client() {
        let e = MeshEvent::SpecialistFinished {
            sub_intent_id: Uuid::nil(),
            lines_added: 0,
            degraded: true,
            note: Some("grocery specialist timed out".into()),
        };
        let v = serde_json::to_value(&e).expect("serialise");
        assert_eq!(v["degraded"], true);
        assert_eq!(v["note"], "grocery specialist timed out");
    }
}
