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
}

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
    /// When the vendor last declared this. Load-bearing — see `confidence`.
    pub updated_at: DateTime<Utc>,
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
    pub fn confidence(&self, fresh_window_mins: i64) -> Confidence {
        match self.state {
            AvailabilityState::OutOfStock | AvailabilityState::Limited => Confidence::Trusted,
            AvailabilityState::Available => {
                if Utc::now() - self.updated_at <= Duration::minutes(fresh_window_mins) {
                    Confidence::Trusted
                } else {
                    Confidence::Uncertain
                }
            }
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

    fn avail(state: AvailabilityState, age_mins: i64) -> Availability {
        Availability {
            item_id:    Uuid::new_v4(),
            tenant_id:  Uuid::new_v4(),
            state,
            updated_at: Utc::now() - Duration::minutes(age_mins),
            updated_by: None,
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
}
