//! The basket — the mesh's shared state.
//!
//! SINGLE WRITER: concurrent specialists never mutate a basket. Each returns a
//! `BasketDelta` scoped to its own sub-intent, and only `Basket::apply` writes.
//! That is what makes budget, timing and temperature conflicts surface
//! deterministically in the reconcile phase instead of as a race.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::catalog::SelectedModifier;
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
    ///
    /// Modifier deltas are already folded in, so this stays the one number every
    /// total multiplies by `qty`. Nothing downstream had to learn about
    /// modifiers for them to be charged correctly.
    pub unit_price_cents:  i64,
    /// The chosen options behind `unit_price_cents`, frozen at proposal time.
    ///
    /// Kept alongside the price so a line can be *explained* — on the customer's
    /// receipt, and to the vendor who has to make the thing. `serde(default)`
    /// because baskets persisted before migration 0018 have no such field.
    #[serde(default)]
    pub modifiers:         Vec<SelectedModifier>,
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
            modifiers: Vec::new(),
            created_at: Utc::now(),
        }
    }

    /// Attach resolved modifier selections.
    ///
    /// A builder rather than a ninth parameter on `propose`: the price passed to
    /// `propose` is already the effective one, so every existing caller that has
    /// no modifiers stays correct and unchanged.
    #[must_use]
    pub fn with_modifiers(mut self, modifiers: Vec<SelectedModifier>) -> Self {
        self.modifiers = modifiers;
        self
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

/// One thing reconcile found while verifying a mesh run.
///
/// Deliberately not the mesh's own `Conflict` type: this crosses the HTTP
/// boundary to the app, and `kind` is carried as opaque JSON so a new variant
/// in the mesh does not become a breaking change here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BasketConflict {
    pub kind:        serde_json::Value,
    /// The line is already gone from this basket. Phrased to the customer as
    /// something done, not something to decide.
    pub blocking:    bool,
    pub description: String,
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
    /// What the last mesh run's verification found. Empty for a manually built
    /// basket — nothing proposed it, so there was nothing to verify.
    pub conflicts:       Vec<BasketConflict>,
    /// Optimistic lock. Bumped by every mutation via `touch`, and compared on
    /// write so a concurrent update is a detected conflict, not a lost one.
    pub version:         i64,
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
            conflicts: Vec::new(),
            version: 0,
            created_at: now,
            updated_at: now,
        }
    }

    /// Every mutation goes through here, so a new one cannot silently skip the
    /// version bump and reopen the lost-update window.
    fn touch(&mut self) {
        self.version += 1;
        self.updated_at = Utc::now();
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
        self.touch();
        id
    }

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
        self.touch();
    }

    /// Remove a line. Returns whether anything was removed, so the API can
    /// answer 404 rather than reporting success for a line that never existed.
    pub fn remove_line(&mut self, line_id: Uuid) -> bool {
        let before = self.lines.len();
        self.lines.retain(|l| l.id != line_id);
        let removed = self.lines.len() != before;
        if removed {
            self.touch();
        }
        removed
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
        self.touch();
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

    /// A rejected line is not a merge target: re-adding an item the customer
    /// (or a specialist) rejected must produce a fresh live line, not silently
    /// resurrect the rejected one by bumping its quantity.
    #[test]
    fn a_rejected_line_is_not_merged_into() {
        let mut b = basket();
        let si = b.browse_sub_intent(Vertical::Grocery);
        let (bid, tid) = (b.id, b.tenant_id);
        let (vendor, item) = (Uuid::new_v4(), Uuid::new_v4());

        let mut rejected = BasketLine::propose(bid, si, tid, vendor, item, 1, 12_000, "browse");
        rejected.state = LineState::Rejected;
        b.add_line(rejected);

        b.add_line(BasketLine::propose(bid, si, tid, vendor, item, 1, 12_000, "browse"));

        assert_eq!(b.lines.len(), 2, "the rejected line stays rejected and separate");
        assert_eq!(b.goods_total_cents(), 12_000, "only the live line is charged");
    }

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

    /// A no-op remove must not bump the version: it would invalidate another
    /// writer's in-flight update for a change that did not happen.
    #[test]
    fn a_removal_that_changes_nothing_does_not_bump_the_version() {
        let mut b = basket();
        let before = b.version;
        assert!(!b.remove_line(Uuid::new_v4()));
        assert_eq!(b.version, before);
    }
}
