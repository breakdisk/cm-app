//! The basket — the mesh's shared state.
//!
//! SINGLE WRITER: concurrent specialists never mutate a basket. Each returns a
//! `BasketDelta` scoped to its own sub-intent, and only `Basket::apply` writes.
//! That is what makes budget, timing and temperature conflicts surface
//! deterministically in the reconcile phase instead of as a race.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::Vertical;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BasketStatus {
    Draft,
    Proposed,
    AwaitingReview,
    Confirmed,
    Abandoned,
}

impl BasketStatus {
    /// The wire and database representation. One definition, so the API and the
    /// repository can never disagree — `format!("{:?}").to_lowercase()` would
    /// render `AwaitingReview` as `awaitingreview` and silently drift from the
    /// `awaiting_review` the CHECK constraint expects.
    pub fn as_str(&self) -> &'static str {
        match self {
            BasketStatus::Draft          => "draft",
            BasketStatus::Proposed       => "proposed",
            BasketStatus::AwaitingReview => "awaiting_review",
            BasketStatus::Confirmed      => "confirmed",
            BasketStatus::Abandoned      => "abandoned",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubIntentStatus {
    Pending,
    Satisfied,
    /// The specialist failed or timed out; this vertical falls back to manual
    /// browse. One degraded sub-intent must not fail the whole basket.
    Degraded,
    Failed,
}

/// Where a sub-intent came from.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineState {
    Proposed,
    Accepted,
    Substituted,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubIntent {
    pub id:          Uuid,
    pub basket_id:   Uuid,
    pub tenant_id:   Uuid,
    pub vertical:    Vertical,
    pub vendor_hint: Option<String>,
    pub raw_text:    String,
    pub constraints: serde_json::Value,
    pub status:      SubIntentStatus,
    pub source:      SubIntentSource,
    pub created_at:  DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BasketLine {
    pub id:                Uuid,
    pub basket_id:         Uuid,
    pub sub_intent_id:     Uuid,
    pub tenant_id:         Uuid,
    pub vendor_id:         Uuid,
    pub item_id:           Uuid,
    pub qty:               i32,
    /// Captured at proposal time — the customer pays what they were shown, even
    /// if the catalog price moves before checkout.
    pub unit_price_cents:  i64,
    pub state:             LineState,
    pub substitution_for:  Option<Uuid>,
    pub proposed_by_agent: Option<String>,
    pub created_at:        DateTime<Utc>,
}

impl BasketLine {
    #[allow(clippy::too_many_arguments)]
    pub fn propose(
        basket_id: Uuid,
        sub_intent_id: Uuid,
        tenant_id: Uuid,
        vendor_id: Uuid,
        item_id: Uuid,
        qty: i32,
        unit_price_cents: i64,
        agent: &str,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            basket_id,
            sub_intent_id,
            tenant_id,
            vendor_id,
            item_id,
            qty,
            unit_price_cents,
            state: LineState::Proposed,
            substitution_for: None,
            proposed_by_agent: Some(agent.to_string()),
            created_at: Utc::now(),
        }
    }

    pub fn subtotal_cents(&self) -> i64 {
        self.unit_price_cents * self.qty as i64
    }

    /// Does this line contribute to what the customer pays?
    pub fn is_chargeable(&self) -> bool {
        self.state != LineState::Rejected
    }
}

/// A specialist's contribution, scoped to one sub-intent.
///
/// Deltas are the only way lines enter a basket. A specialist that cannot
/// satisfy its sub-intent returns an empty delta with a `note` — it never
/// writes a partial basket.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BasketDelta {
    pub sub_intent_id: Uuid,
    pub lines:         Vec<BasketLine>,
    pub note:          Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Basket {
    pub id:              Uuid,
    pub tenant_id:       Uuid,
    pub customer_id:     Uuid,
    pub status:          BasketStatus,
    pub mesh_session_id: Option<Uuid>,
    pub sub_intents:     Vec<SubIntent>,
    pub lines:           Vec<BasketLine>,
    pub created_at:      DateTime<Utc>,
    pub updated_at:      DateTime<Utc>,
}

impl Basket {
    pub fn new(tenant_id: Uuid, customer_id: Uuid) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            customer_id,
            status: BasketStatus::Draft,
            mesh_session_id: None,
            sub_intents: Vec::new(),
            lines: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

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

    /// **The single writer.** Replaces this sub-intent's lines wholesale and
    /// leaves every other sub-intent untouched.
    ///
    /// Replace rather than append so a specialist that retries — or that the
    /// runner re-drives after a transient failure — cannot double the basket.
    /// Scoping by `sub_intent_id` is what lets concurrent specialists write
    /// without coordinating: their deltas are disjoint by construction.
    pub fn apply(&mut self, delta: BasketDelta) {
        self.lines.retain(|l| l.sub_intent_id != delta.sub_intent_id);
        self.lines.extend(delta.lines);
        self.updated_at = Utc::now();
    }

    /// What the customer pays for goods, before delivery fee and tip.
    pub fn goods_total_cents(&self) -> i64 {
        self.lines.iter().filter(|l| l.is_chargeable()).map(|l| l.subtotal_cents()).sum()
    }

    /// The lines Screen C must surface — the only ones blocking checkout.
    pub fn lines_awaiting_review(&self) -> Vec<&BasketLine> {
        self.lines.iter().filter(|l| l.state == LineState::Substituted).collect()
    }

    /// Per-vendor goods subtotals, for the vendor payout legs in Plan 5.
    pub fn subtotals_by_vendor(&self) -> std::collections::HashMap<Uuid, i64> {
        let mut out = std::collections::HashMap::new();
        for l in self.lines.iter().filter(|l| l.is_chargeable()) {
            *out.entry(l.vendor_id).or_insert(0) += l.subtotal_cents();
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn basket() -> Basket {
        Basket::new(Uuid::new_v4(), Uuid::new_v4())
    }

    fn line(basket_id: Uuid, sub_intent_id: Uuid, price: i64, qty: i32) -> BasketLine {
        BasketLine::propose(
            basket_id, sub_intent_id, Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(),
            qty, price, "nutritionist",
        )
    }

    #[test]
    fn a_new_basket_is_draft_and_empty() {
        let b = basket();
        assert_eq!(b.status, BasketStatus::Draft);
        assert!(b.lines.is_empty());
        assert_eq!(b.goods_total_cents(), 0);
    }

    /// The single-writer property. Specialists return deltas; only this method
    /// mutates the basket. Applying two deltas from two concurrent specialists
    /// must produce a deterministic union, not a lost update.
    #[test]
    fn applying_two_deltas_merges_both_without_loss() {
        let mut b = basket();
        let si_food = Uuid::new_v4();
        let si_grocery = Uuid::new_v4();

        b.apply(BasketDelta {
            sub_intent_id: si_food,
            lines: vec![line(b.id, si_food, 34_000, 1)],
            note: None,
        });
        b.apply(BasketDelta {
            sub_intent_id: si_grocery,
            lines: vec![line(b.id, si_grocery, 12_000, 2)],
            note: None,
        });

        assert_eq!(b.lines.len(), 2, "both specialists' lines must survive");
        assert_eq!(b.goods_total_cents(), 34_000 + 24_000);
    }

    /// Re-applying a delta for the same sub-intent replaces that sub-intent's
    /// lines rather than duplicating them — a specialist that retries must not
    /// double the basket.
    #[test]
    fn reapplying_a_delta_replaces_that_sub_intents_lines() {
        let mut b = basket();
        let si = Uuid::new_v4();

        b.apply(BasketDelta { sub_intent_id: si, lines: vec![line(b.id, si, 10_000, 1)], note: None });
        b.apply(BasketDelta { sub_intent_id: si, lines: vec![line(b.id, si, 15_000, 1)], note: None });

        assert_eq!(b.lines.len(), 1, "a retry must replace, not append");
        assert_eq!(b.goods_total_cents(), 15_000);
    }

    /// A delta for one sub-intent must never disturb another's lines.
    #[test]
    fn reapplying_one_sub_intent_leaves_the_others_alone() {
        let mut b = basket();
        let si_a = Uuid::new_v4();
        let si_b = Uuid::new_v4();

        b.apply(BasketDelta { sub_intent_id: si_a, lines: vec![line(b.id, si_a, 10_000, 1)], note: None });
        b.apply(BasketDelta { sub_intent_id: si_b, lines: vec![line(b.id, si_b, 20_000, 1)], note: None });
        b.apply(BasketDelta { sub_intent_id: si_a, lines: vec![line(b.id, si_a, 11_000, 1)], note: None });

        assert_eq!(b.lines.len(), 2);
        assert_eq!(b.goods_total_cents(), 11_000 + 20_000);
    }

    #[test]
    fn rejected_lines_do_not_count_toward_the_total() {
        let mut b = basket();
        let si = Uuid::new_v4();
        let mut l = line(b.id, si, 9_000, 1);
        l.state = LineState::Rejected;
        b.apply(BasketDelta { sub_intent_id: si, lines: vec![l], note: None });

        assert_eq!(b.goods_total_cents(), 0, "a rejected line is not charged for");
    }

    /// Screen C surfaces exactly the lines that block checkout.
    #[test]
    fn lines_awaiting_review_are_the_ones_needing_a_decision() {
        let mut b = basket();
        let si = Uuid::new_v4();

        let accepted = { let mut l = line(b.id, si, 1_000, 1); l.state = LineState::Accepted; l };
        let swapped  = { let mut l = line(b.id, si, 2_000, 1); l.state = LineState::Substituted; l };

        b.apply(BasketDelta { sub_intent_id: si, lines: vec![accepted, swapped], note: None });

        let pending = b.lines_awaiting_review();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].state, LineState::Substituted);
    }

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
}
