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
    pub modifiers:      serde_json::Value,
    pub allergens:      Vec<String>,
    /// When a vendor last asserted the contents. `None` means never — which is
    /// not the same as "contains none". See migration 0014.
    pub allergens_declared_at: Option<DateTime<Utc>>,
    pub dietary_tags:   Vec<String>,
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
            modifiers: serde_json::json!([]),
            allergens: allergens.iter().map(|s| s.to_string()).collect(),
            allergens_declared_at: declared,
            dietary_tags: vec![],
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
}
