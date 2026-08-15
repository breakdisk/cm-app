//! Catalog items and their availability.
//!
//! Availability is vendor-declared, not POS-synced. The age of a declaration is
//! therefore part of its meaning: `confidence` turns "what the vendor said" plus
//! "how long ago they said it" into "how much an agent should trust it".

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogItem {
    pub id:             Uuid,
    pub tenant_id:      Uuid,
    pub vendor_id:      Uuid,
    pub sku:            String,
    pub name:           String,
    pub description:    Option<String>,
    pub price_cents:    i64,
    /// Choices offered against this item — "Size", "Add-ons". Empty for most.
    ///
    /// Typed rather than raw JSON since migration 0018. It spent its whole life
    /// before that as an untyped column the API round-tripped and nothing read,
    /// which is precisely why a merchant form alone would not have made
    /// modifiers work: there was nothing downstream to receive a selection.
    pub modifiers:      Vec<ModifierGroup>,
    pub allergens:      Vec<String>,
    /// When a vendor last asserted the contents. `None` means never — which is
    /// not the same as "contains none". See migration 0014.
    pub allergens_declared_at: Option<DateTime<Utc>>,
    pub dietary_tags:   Vec<String>,
    /// "Mains", "Beverages"… `None` = uncategorised, which an import legitimately
    /// produces. A real column, not a `vertical_attrs` key: it means the same
    /// thing in every vertical and both browse surfaces group by it.
    pub category:       Option<String>,
    pub vertical_attrs: serde_json::Value,
    pub is_listed:      bool,
    /// Which ingest wrote this row last.
    pub source:         CatalogSource,
    /// The id in the source system, for idempotent re-sync. `None` for manual.
    pub external_id:    Option<String>,
    /// When an ingest last touched this item. `None` for hand-entered rows.
    pub synced_at:      Option<DateTime<Utc>>,
    /// Object key of the product photo, or `None` for no photo.
    ///
    /// A key rather than a URL: the bucket is cluster-internal, so a stored URL
    /// would be unreachable from a browser. Only the photo endpoint writes it —
    /// the catalog upsert leaves it alone, so a re-sync cannot wipe a picture
    /// the vendor uploaded.
    pub image_key:      Option<String>,
    pub created_at:     DateTime<Utc>,
    pub updated_at:     DateTime<Utc>,
}

impl CatalogItem {
    /// Does this item contain any allergen the customer must avoid?
    /// Case-insensitive — vendors type these by hand.
    pub fn conflicts_with_allergens(&self, avoid: &[String]) -> bool {
        self.allergens.iter().any(|a| {
            avoid.iter().any(|x| x.eq_ignore_ascii_case(a))
        })
    }

    /// Fold an ingested record into this item.
    ///
    /// Commercial facts — name, price, listing — are the source system's to own;
    /// a vendor who wired up Shopify expects a price change there to land here.
    /// Allergens are not, and the two rules below are the reason this function
    /// exists instead of a plain UPDATE in each adapter.
    pub fn merge_ingested(&mut self, incoming: &IngestedItem, source: CatalogSource, now: DateTime<Utc>) {
        self.name         = incoming.name.clone();
        self.description  = incoming.description.clone();
        self.price_cents  = incoming.price_cents;
        self.dietary_tags = incoming.dietary_tags.clone();
        self.is_listed    = incoming.is_listed;

        // Rule 1 — a human declaration outranks any machine field, including an
        // empty one. A Shopify store with no allergen mapping syncs `[]`; taking
        // it would erase "contains dairy" and send the dish to someone who asked
        // us to avoid it. Silence from a source system is not a correction.
        //
        // Rule 2 — where no human has declared, machine allergens are still
        // worth storing (we can exclude on them), but `allergens_declared_at`
        // stays NULL. The item keeps reading "contents not stated", which is
        // what withholds it from allergy-avoiding customers. An ingest can add
        // information; it cannot add an attestation.
        if self.allergens_declared_at.is_none() {
            self.allergens = incoming.allergens.clone();
        }

        self.source       = source;
        self.external_id  = incoming.external_id.clone();
        self.synced_at    = Some(now);
        self.updated_at   = now;
    }
}

/// One choice inside a group — "Large", "Extra shot", "No onions".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModifierOption {
    pub id:   Uuid,
    pub name: String,
    /// Added to the item's base price when chosen. Signed, because "no cheese"
    /// is a legitimate discount and not every modifier costs money.
    #[serde(default)]
    pub price_delta_cents: i64,
}

/// A set of choices offered against an item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModifierGroup {
    pub id:   Uuid,
    pub name: String,
    /// How many options must be chosen. 0 leaves the group optional.
    #[serde(default)]
    pub min_select: usize,
    /// How many options may be chosen. 1 is a radio group, more is checkboxes.
    #[serde(default = "one")]
    pub max_select: usize,
    pub options: Vec<ModifierOption>,
}

const fn one() -> usize { 1 }

impl ModifierGroup {
    /// A group nobody can satisfy is a catalog bug, not a customer error — it
    /// would make every add-to-basket for the item fail with a message about
    /// the customer's choices. Checked when a vendor saves, not at order time.
    pub fn is_coherent(&self) -> bool {
        self.max_select >= 1
            && self.min_select <= self.max_select
            && self.min_select <= self.options.len()
            && !self.options.is_empty()
    }
}

/// What the customer chose, resolved against the catalog and frozen.
///
/// Names are copied rather than referenced so a later rename or delete cannot
/// change what an existing line says — the same reasoning that makes
/// `BasketLine::unit_price_cents` a snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectedModifier {
    pub group_id:          Uuid,
    pub group_name:        String,
    pub option_id:         Uuid,
    pub option_name:       String,
    pub price_delta_cents: i64,
}

/// Why a set of chosen option ids was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModifierError {
    /// Not an option of any group on this item. Covers both a typo and an
    /// attempt to attach a cheaper item's option to a dearer one.
    UnknownOption(Uuid),
    /// The same option id sent twice. Rejected rather than deduplicated: it is
    /// ambiguous whether the customer meant "two of these" (which is `qty`, and
    /// priced differently) or fat-fingered the request.
    DuplicateOption(Uuid),
    TooFew  { group: String, min: usize, got: usize },
    TooMany { group: String, max: usize, got: usize },
}

impl std::fmt::Display for ModifierError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownOption(id)   => write!(f, "option {id} is not offered on this item"),
            Self::DuplicateOption(id) => write!(f, "option {id} was selected more than once"),
            Self::TooFew { group, min, got } => {
                write!(f, "\"{group}\" needs at least {min} selection(s), got {got}")
            }
            Self::TooMany { group, max, got } => {
                write!(f, "\"{group}\" allows at most {max} selection(s), got {got}")
            }
        }
    }
}

impl std::error::Error for ModifierError {}

impl CatalogItem {
    /// Resolve chosen option ids into a frozen selection and an effective unit
    /// price.
    ///
    /// The caller passes ids only. Prices are read here, from the catalog, for
    /// the same reason `BasketService::add_item` refuses a caller-supplied
    /// price: anything the client can name, the client can lower. A modifier
    /// delta is as much money as the base price is.
    ///
    /// Returns the price *including* deltas, so callers keep multiplying by qty
    /// exactly as before and no total anywhere needs to learn about modifiers.
    pub fn resolve_modifiers(
        &self,
        chosen: &[Uuid],
    ) -> Result<(i64, Vec<SelectedModifier>), ModifierError> {
        let mut seen: Vec<Uuid> = Vec::with_capacity(chosen.len());
        for id in chosen {
            if seen.contains(id) {
                return Err(ModifierError::DuplicateOption(*id));
            }
            seen.push(*id);
        }

        let mut selected = Vec::with_capacity(chosen.len());
        for id in chosen {
            let found = self.modifiers.iter().find_map(|g| {
                g.options.iter().find(|o| o.id == *id).map(|o| (g, o))
            });
            let (group, option) = found.ok_or(ModifierError::UnknownOption(*id))?;
            selected.push(SelectedModifier {
                group_id:          group.id,
                group_name:        group.name.clone(),
                option_id:         option.id,
                option_name:       option.name.clone(),
                price_delta_cents: option.price_delta_cents,
            });
        }

        // Cardinality is checked per group, including groups with no selection
        // at all — that is the case that catches a required group the client
        // simply omitted, which is the one a UI bug produces most often.
        for group in &self.modifiers {
            let got = selected.iter().filter(|s| s.group_id == group.id).count();
            if got < group.min_select {
                return Err(ModifierError::TooFew {
                    group: group.name.clone(),
                    min:   group.min_select,
                    got,
                });
            }
            if got > group.max_select {
                return Err(ModifierError::TooMany {
                    group: group.name.clone(),
                    max:   group.max_select,
                    got,
                });
            }
        }

        let unit = self.price_cents + selected.iter().map(|s| s.price_delta_cents).sum::<i64>();
        Ok((unit, selected))
    }
}

/// Where an item's facts came from. Every adapter that can write a catalog
/// declares one, so a vendor (and an auditor) can see which rows a human typed
/// and which a machine pushed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogSource {
    Manual,
    Shopify,
    WooCommerce,
    Csv,
    Pos,
}

impl CatalogSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            CatalogSource::Manual      => "manual",
            CatalogSource::Shopify     => "shopify",
            CatalogSource::WooCommerce => "woocommerce",
            CatalogSource::Csv         => "csv",
            CatalogSource::Pos         => "pos",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "manual"      => CatalogSource::Manual,
            "shopify"     => CatalogSource::Shopify,
            "woocommerce" => CatalogSource::WooCommerce,
            "csv"         => CatalogSource::Csv,
            "pos"         => CatalogSource::Pos,
            _ => return None,
        })
    }

    /// Did a person type this, or did a machine push it?
    ///
    /// The distinction the whole ingest design turns on — not "which vendor
    /// system", but "was there a human in the loop".
    pub fn is_human(&self) -> bool {
        matches!(self, CatalogSource::Manual)
    }
}

/// One item as an ingest adapter yields it, before any merge rules apply.
///
/// Deliberately dumb: an adapter's job is to translate a foreign payload into
/// this shape, not to decide what may overwrite what. Those rules live in
/// `merge_ingested` so that adding a fourth adapter cannot add a fourth
/// interpretation of them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestedItem {
    /// The id in the source system, for idempotent re-sync. `None` for sources
    /// with no stable id (a hand-rolled CSV), which then match on `sku`.
    pub external_id:  Option<String>,
    pub sku:          String,
    pub name:         String,
    pub description:  Option<String>,
    pub price_cents:  i64,
    /// Whatever the source system knows. Never an attestation — see
    /// `merge_ingested`.
    #[serde(default)]
    pub allergens:    Vec<String>,
    #[serde(default)]
    pub dietary_tags: Vec<String>,
    #[serde(default = "default_true")]
    pub is_listed:    bool,
}

fn default_true() -> bool { true }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AvailabilityState {
    Available,
    Limited,
    OutOfStock,
}

impl AvailabilityState {
    pub fn as_str(&self) -> &'static str {
        match self {
            AvailabilityState::Available  => "available",
            AvailabilityState::Limited    => "limited",
            AvailabilityState::OutOfStock => "out_of_stock",
        }
    }
}

/// How much an agent should trust an availability declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Trusted,
    Uncertain,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Availability {
    pub item_id:    Uuid,
    pub tenant_id:  Uuid,
    pub state:      AvailabilityState,
    /// When this row last changed **by any means**, human or machine. Useful for
    /// ops ("did the Shopify sync run?") and deliberately NOT the trust input —
    /// see `confirmed_at`.
    pub updated_at: DateTime<Utc>,
    /// When a *human* last attested this state. `None` means nobody ever has.
    ///
    /// Separate from `updated_at` because a catalog sync is not a declaration.
    /// A POS reconciled overnight can set `state` to whatever its stock count
    /// says, and that number is precisely the old evidence this model exists to
    /// distrust — so an ingest updates `updated_at` and leaves this alone.
    pub confirmed_at: Option<DateTime<Utc>>,
    pub updated_by: Option<Uuid>,
}

impl Availability {
    /// Staleness only ever reduces confidence that an item is *present*.
    ///
    /// An "in stock" flag from four hours ago is a guess; the item may have sold
    /// out since. An "out of stock" flag from four hours ago is still believed —
    /// a vendor who marks something gone rarely has it back within the window,
    /// and being wrong in that direction merely offers a substitute the customer
    /// can decline. Being wrong the other way means a courier arrives to nothing.
    /// Reads `confirmed_at`, never `updated_at`. A catalog sync moves the latter
    /// and not the former precisely so that it cannot manufacture trust: an
    /// overnight POS reconciliation is old evidence wearing a new timestamp.
    pub fn confidence(&self, fresh_window_mins: i64) -> Confidence {
        match self.state {
            AvailabilityState::OutOfStock | AvailabilityState::Limited => Confidence::Trusted,
            AvailabilityState::Available => match self.confirmed_at {
                // Nobody has ever confirmed this. Machine-populated, or created
                // and not yet attested — either way it has earned no trust.
                None => Confidence::Uncertain,
                Some(at) if Utc::now() - at <= Duration::minutes(fresh_window_mins) => {
                    Confidence::Trusted
                }
                Some(_) => Confidence::Uncertain,
            },
        }
    }

    /// Should the agent line up a substitute before the courier sets off?
    ///
    /// True when the item is gone, nearly gone, or claimed present on evidence
    /// too old to rely on. This is what makes Screen C's substitution review
    /// meaningful rather than decorative.
    pub fn warrants_substitute(&self, fresh_window_mins: i64) -> bool {
        match self.state {
            AvailabilityState::OutOfStock | AvailabilityState::Limited => true,
            AvailabilityState::Available => self.confidence(fresh_window_mins) == Confidence::Uncertain,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use uuid::Uuid;

    /// A state a human confirmed `age_mins` ago.
    fn avail(state: AvailabilityState, age_mins: i64) -> Availability {
        let at = Utc::now() - Duration::minutes(age_mins);
        Availability {
            item_id:    Uuid::new_v4(),
            tenant_id:  Uuid::new_v4(),
            state,
            updated_at: at,
            confirmed_at: Some(at),
            updated_by: Some(Uuid::new_v4()),
        }
    }

    /// The same state, arrived at by a machine sync — nobody has confirmed it.
    fn synced(state: AvailabilityState, age_mins: i64) -> Availability {
        Availability {
            updated_at:   Utc::now() - Duration::minutes(age_mins),
            confirmed_at: None,
            updated_by:   None,
            ..avail(state, age_mins)
        }
    }

    const FRESH_WINDOW: i64 = 30;

    #[test]
    fn a_recently_confirmed_in_stock_item_is_trusted() {
        let a = avail(AvailabilityState::Available, 2);
        assert_eq!(a.confidence(FRESH_WINDOW), Confidence::Trusted);
        assert!(!a.warrants_substitute(FRESH_WINDOW));
    }

    /// The whole point of the freshness stamp: a stale "in stock" flag is not
    /// a promise. The agent should line up a substitute rather than assume.
    #[test]
    fn a_stale_in_stock_flag_is_only_uncertain() {
        let a = avail(AvailabilityState::Available, 240);
        assert_eq!(a.confidence(FRESH_WINDOW), Confidence::Uncertain);
        assert!(a.warrants_substitute(FRESH_WINDOW),
                "a 4-hour-old in-stock flag should trigger defensive substitution");
    }

    /// Out-of-stock is believed regardless of age. Staleness can only ever make
    /// us *less* confident an item is present, never more.
    #[test]
    fn out_of_stock_is_trusted_even_when_stale() {
        let a = avail(AvailabilityState::OutOfStock, 5_000);
        assert_eq!(a.confidence(FRESH_WINDOW), Confidence::Trusted);
        assert!(a.warrants_substitute(FRESH_WINDOW),
                "out of stock always needs a substitute — that is the point");
    }

    #[test]
    fn limited_stock_always_warrants_a_backup() {
        let a = avail(AvailabilityState::Limited, 1);
        assert!(a.warrants_substitute(FRESH_WINDOW));
    }

    #[test]
    fn the_freshness_boundary_is_inclusive_of_the_window() {
        assert_eq!(avail(AvailabilityState::Available, 29).confidence(30), Confidence::Trusted);
        assert_eq!(avail(AvailabilityState::Available, 31).confidence(30), Confidence::Uncertain);
    }

    /// The rule that makes an ingest port safe to plug in.
    ///
    /// A POS or Shopify sync can set state to whatever its stock count says, and
    /// that count is exactly the kind of evidence this model distrusts: nobody
    /// looked at the shelf. Machine-set availability is therefore never trusted,
    /// however recently the sync ran.
    #[test]
    fn a_machine_synced_in_stock_flag_is_never_trusted() {
        let a = synced(AvailabilityState::Available, 1);
        assert_eq!(a.confidence(FRESH_WINDOW), Confidence::Uncertain);
        assert!(a.warrants_substitute(FRESH_WINDOW),
                "an unconfirmed sync must line up a substitute, however fresh the sync");
    }

    /// A sync must not be able to launder a stale human confirmation into a
    /// fresh one by touching the row. This is the whole reason the two
    /// timestamps are separate columns rather than one.
    #[test]
    fn a_sync_cannot_launder_a_stale_confirmation() {
        let a = Availability {
            updated_at:   Utc::now(),                                  // sync just ran
            confirmed_at: Some(Utc::now() - Duration::minutes(240)),   // human, 4h ago
            ..avail(AvailabilityState::Available, 240)
        };
        assert_eq!(a.confidence(FRESH_WINDOW), Confidence::Uncertain);
    }

    /// The converse: a sync touching the row does not *destroy* a recent human
    /// confirmation either. The human's clock is the only one that counts.
    #[test]
    fn a_sync_does_not_invalidate_a_recent_confirmation() {
        let a = Availability {
            updated_at:   Utc::now(),
            confirmed_at: Some(Utc::now() - Duration::minutes(5)),
            ..avail(AvailabilityState::Available, 5)
        };
        assert_eq!(a.confidence(FRESH_WINDOW), Confidence::Trusted);
    }

    /// Asymmetry holds regardless of who said it. A machine reporting zero stock
    /// is believed — being wrong that way costs a declinable substitute, being
    /// wrong the other way costs a courier arriving to nothing.
    #[test]
    fn a_machine_synced_out_of_stock_is_still_trusted() {
        let a = synced(AvailabilityState::OutOfStock, 5_000);
        assert_eq!(a.confidence(FRESH_WINDOW), Confidence::Trusted);
    }

    // ---- ingest merge rules -------------------------------------------------

    fn item(allergens: &[&str], declared: Option<DateTime<Utc>>) -> CatalogItem {
        CatalogItem {
            id: Uuid::new_v4(), tenant_id: Uuid::new_v4(), vendor_id: Uuid::new_v4(),
            sku: "SKU-1".into(), name: "Adobo".into(), description: None,
            price_cents: 18000,
            modifiers: Vec::new(),
            allergens: allergens.iter().map(|s| s.to_string()).collect(),
            allergens_declared_at: declared,
            dietary_tags: vec![],
            category: None,
            vertical_attrs: serde_json::json!({}),
            is_listed: true,
            source: CatalogSource::Manual,
            external_id: None,
            synced_at: None,
            image_key: None,
            created_at: Utc::now(), updated_at: Utc::now(),
        }
    }

    fn incoming(allergens: &[&str]) -> IngestedItem {
        IngestedItem {
            external_id: Some("gid://shopify/Product/42".into()),
            sku: "SKU-1".into(),
            name: "Chicken Adobo".into(),
            description: Some("with rice".into()),
            price_cents: 19500,
            allergens: allergens.iter().map(|s| s.to_string()).collect(),
            dietary_tags: vec!["halal".into()],
            is_listed: true,
        }
    }

    #[test]
    fn an_ingest_updates_the_commercial_facts() {
        let mut i = item(&[], None);
        i.merge_ingested(&incoming(&[]), CatalogSource::Shopify, Utc::now());

        assert_eq!(i.name, "Chicken Adobo");
        assert_eq!(i.price_cents, 19500);
        assert_eq!(i.source, CatalogSource::Shopify);
        assert!(i.synced_at.is_some(), "a sync must record that it ran");
    }

    /// The liability rule. A Shopify tag list is data, not a statement that
    /// someone checked what is in the dish — so an ingest may populate
    /// `allergens` (useful: we can still exclude on it) but must never stamp
    /// `allergens_declared_at`. The item stays "contents not stated", which is
    /// what withholds it from customers who asked us to avoid something.
    #[test]
    fn an_ingest_never_counts_as_an_allergen_declaration() {
        let mut i = item(&[], None);
        i.merge_ingested(&incoming(&["peanuts"]), CatalogSource::Shopify, Utc::now());

        assert_eq!(i.allergens, vec!["peanuts".to_string()],
                   "machine allergen data is still worth having — we can exclude on it");
        assert!(i.allergens_declared_at.is_none(),
                "a sync must not manufacture an attestation no human made");
    }

    /// The converse, and the more dangerous direction: a vendor stated this dish
    /// contains dairy. A Shopify store with no allergen field mapped syncs an
    /// empty list. Erasing the human's declaration would send a dairy dish to
    /// someone who asked us to avoid it.
    #[test]
    fn an_ingest_cannot_erase_a_human_allergen_declaration() {
        let declared_at = Utc::now() - Duration::minutes(60);
        let mut i = item(&["dairy"], Some(declared_at));
        i.merge_ingested(&incoming(&[]), CatalogSource::Shopify, Utc::now());

        assert_eq!(i.allergens, vec!["dairy".to_string()],
                   "a human declaration outranks an empty machine field");
        assert_eq!(i.allergens_declared_at, Some(declared_at),
                   "and its timestamp must not move");
    }

    #[test]
    fn a_source_round_trips_through_its_wire_name() {
        for s in [CatalogSource::Manual, CatalogSource::Shopify,
                  CatalogSource::WooCommerce, CatalogSource::Csv, CatalogSource::Pos] {
            assert_eq!(CatalogSource::parse(s.as_str()), Some(s));
        }
        assert_eq!(CatalogSource::parse("magento"), None);
    }

    #[test]
    fn only_manual_entry_counts_as_a_human_in_the_loop() {
        assert!(CatalogSource::Manual.is_human());
        assert!(!CatalogSource::Shopify.is_human());
        assert!(!CatalogSource::Pos.is_human());
    }

    // ---- modifiers ----------------------------------------------------------
    //
    // Every assertion here is about money or about what a customer is allowed to
    // send. The pricing path has no database, so these are the only place the
    // arithmetic is checked before it reaches a real basket.

    fn opt(name: &str, delta: i64) -> ModifierOption {
        ModifierOption { id: Uuid::new_v4(), name: name.into(), price_delta_cents: delta }
    }

    fn group(name: &str, min: usize, max: usize, options: Vec<ModifierOption>) -> ModifierGroup {
        ModifierGroup { id: Uuid::new_v4(), name: name.into(), min_select: min, max_select: max, options }
    }

    fn with_groups(groups: Vec<ModifierGroup>) -> CatalogItem {
        let mut i = item(&[], None);
        i.price_cents = 10_000;
        i.modifiers = groups;
        i
    }

    #[test]
    fn no_modifiers_leaves_the_price_alone() {
        let i = with_groups(vec![]);
        let (price, sel) = i.resolve_modifiers(&[]).unwrap();
        assert_eq!(price, 10_000);
        assert!(sel.is_empty());
    }

    #[test]
    fn deltas_are_added_to_the_base_price() {
        let large = opt("Large", 2_500);
        let id = large.id;
        let i = with_groups(vec![group("Size", 1, 1, vec![large])]);
        let (price, sel) = i.resolve_modifiers(&[id]).unwrap();
        assert_eq!(price, 12_500);
        assert_eq!(sel.len(), 1);
        assert_eq!(sel[0].option_name, "Large");
        assert_eq!(sel[0].group_name, "Size");
    }

    #[test]
    fn a_negative_delta_takes_money_off() {
        let no_cheese = opt("No cheese", -1_000);
        let id = no_cheese.id;
        let i = with_groups(vec![group("Cheese", 0, 1, vec![no_cheese])]);
        assert_eq!(i.resolve_modifiers(&[id]).unwrap().0, 9_000);
    }

    #[test]
    fn several_options_across_groups_all_count() {
        let large = opt("Large", 2_500);
        let bacon = opt("Bacon", 3_000);
        let egg   = opt("Egg", 1_500);
        let (l, b, e) = (large.id, bacon.id, egg.id);
        let i = with_groups(vec![
            group("Size", 1, 1, vec![large]),
            group("Extras", 0, 2, vec![bacon, egg]),
        ]);
        assert_eq!(i.resolve_modifiers(&[l, b, e]).unwrap().0, 17_000);
    }

    /// The one that matters: an option id lifted from a *different*, cheaper
    /// item must not attach here. Without this a caller could pick any option
    /// in the catalog and pay its delta against this item's price.
    #[test]
    fn an_option_from_another_item_is_refused() {
        let i = with_groups(vec![group("Size", 1, 1, vec![opt("Large", 2_500)])]);
        let foreign = Uuid::new_v4();
        assert_eq!(i.resolve_modifiers(&[foreign]), Err(ModifierError::UnknownOption(foreign)));
    }

    #[test]
    fn a_required_group_left_empty_is_refused() {
        let i = with_groups(vec![group("Size", 1, 1, vec![opt("Large", 2_500)])]);
        match i.resolve_modifiers(&[]) {
            Err(ModifierError::TooFew { min, got, .. }) => { assert_eq!((min, got), (1, 0)); }
            other => panic!("expected TooFew, got {other:?}"),
        }
    }

    #[test]
    fn exceeding_a_groups_maximum_is_refused() {
        let a = opt("Bacon", 3_000);
        let b = opt("Egg", 1_500);
        let c = opt("Avocado", 2_000);
        let (x, y, z) = (a.id, b.id, c.id);
        let i = with_groups(vec![group("Extras", 0, 2, vec![a, b, c])]);
        match i.resolve_modifiers(&[x, y, z]) {
            Err(ModifierError::TooMany { max, got, .. }) => { assert_eq!((max, got), (2, 3)); }
            other => panic!("expected TooMany, got {other:?}"),
        }
    }

    /// Sending the same option twice is rejected rather than silently counted
    /// once — quietly deduplicating would charge one delta for what the client
    /// believes it asked for twice, and the difference is money.
    #[test]
    fn the_same_option_twice_is_refused() {
        let bacon = opt("Bacon", 3_000);
        let id = bacon.id;
        let i = with_groups(vec![group("Extras", 0, 2, vec![bacon])]);
        assert_eq!(i.resolve_modifiers(&[id, id]), Err(ModifierError::DuplicateOption(id)));
    }

    #[test]
    fn an_optional_group_may_be_skipped() {
        let i = with_groups(vec![group("Extras", 0, 3, vec![opt("Bacon", 3_000)])]);
        assert_eq!(i.resolve_modifiers(&[]).unwrap().0, 10_000);
    }

    #[test]
    fn a_group_nobody_could_satisfy_is_incoherent() {
        // min above the number of options on offer
        assert!(!group("Size", 2, 2, vec![opt("Large", 0)]).is_coherent());
        // max of zero — the group can never be chosen from
        assert!(!group("Size", 0, 0, vec![opt("Large", 0)]).is_coherent());
        // min above max
        assert!(!group("Size", 2, 1, vec![opt("A", 0), opt("B", 0)]).is_coherent());
        // no options at all
        assert!(!group("Size", 0, 1, vec![]).is_coherent());
        // and a normal one
        assert!(group("Size", 1, 1, vec![opt("Large", 0)]).is_coherent());
    }

    /// The selection is a snapshot. A vendor repricing or renaming an option
    /// afterwards must not change what an existing line says it owes.
    #[test]
    fn the_selection_captures_names_and_deltas_not_references() {
        let large = opt("Large", 2_500);
        let id = large.id;
        let mut i = with_groups(vec![group("Size", 1, 1, vec![large])]);
        let (_, sel) = i.resolve_modifiers(&[id]).unwrap();

        i.modifiers[0].options[0].price_delta_cents = 9_999;
        i.modifiers[0].options[0].name = "Enormous".into();

        assert_eq!(sel[0].price_delta_cents, 2_500);
        assert_eq!(sel[0].option_name, "Large");
    }

}
