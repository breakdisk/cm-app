//! Loyalty tiers: derived from completed moves in the last 12 months, against
//! a ladder the tenant sets.
//!
//! A tier takes a share off accessorials only and never more than its cap on
//! one move. No tier touches distance or weight: a long, heavy move costs what
//! the rate card says, so the perk cannot run away with it.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Tier {
    pub id: Uuid,
    pub name: String,
    pub min_moves: i32,
    pub perk: String,
    pub accessorial_bps: i64,
    pub cap_cents: i64,
    pub referral_multiplier: i32,
}

impl Tier {
    /// Before its cap and the ceiling: a share of the accessorials.
    pub fn raw_cents(&self, accessorial_cents: i64) -> i64 {
        accessorial_cents.max(0) * self.accessorial_bps.clamp(0, 10_000) / 10_000
    }
}

/// The highest rung reached. None below the first rung, or with no ladder.
pub fn tier_for(ladder: &[Tier], moves: i64) -> Option<&Tier> {
    ladder.iter().filter(|t| i64::from(t.min_moves) <= moves).max_by_key(|t| t.min_moves)
}

/// The next rung up, if there is one.
pub fn next_tier(ladder: &[Tier], moves: i64) -> Option<&Tier> {
    ladder.iter().filter(|t| i64::from(t.min_moves) > moves).min_by_key(|t| t.min_moves)
}

/// A rung as an admin states it.
#[derive(Debug, Clone, Deserialize)]
pub struct NewTier {
    pub name: String,
    pub min_moves: i32,
    #[serde(default)]
    pub perk: String,
    #[serde(default)]
    pub accessorial_bps: i64,
    #[serde(default)]
    pub cap_cents: i64,
    #[serde(default = "one")]
    pub referral_multiplier: i32,
}

fn one() -> i32 { 1 }

/// A ladder is replaced whole, so it is checked whole.
pub fn validate_ladder(tiers: &[NewTier]) -> Result<(), String> {
    if tiers.len() > 10 {
        return Err("A ladder has at most 10 tiers".into());
    }
    let mut thresholds = std::collections::HashSet::new();
    let mut names = std::collections::HashSet::new();
    for t in tiers {
        let name = t.name.trim();
        if name.is_empty() || name.len() > 32 {
            return Err("A tier name is 1–32 characters".into());
        }
        if !names.insert(name.to_ascii_lowercase()) {
            return Err(format!("Two tiers are called {name}"));
        }
        if t.min_moves < 0 || !thresholds.insert(t.min_moves) {
            return Err(format!("{name}: each tier needs its own non-negative move count"));
        }
        if !(0..=10_000).contains(&t.accessorial_bps) {
            return Err(format!("{name}: accessorial_bps is 0–10000"));
        }
        if t.cap_cents < 0 {
            return Err(format!("{name}: cap_cents cannot be negative"));
        }
        if t.accessorial_bps > 0 && t.cap_cents == 0 {
            return Err(format!("{name}: a tier that discounts needs a cap per move"));
        }
        if !(1..=5).contains(&t.referral_multiplier) {
            return Err(format!("{name}: referral_multiplier is 1–5"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tier(name: &str, min_moves: i32, bps: i64, cap: i64) -> Tier {
        Tier {
            id: Uuid::new_v4(),
            name: name.into(),
            min_moves,
            perk: String::new(),
            accessorial_bps: bps,
            cap_cents: cap,
            referral_multiplier: 1,
        }
    }

    /// The design's ladder.
    fn ladder() -> Vec<Tier> {
        vec![tier("Bronze", 1, 0, 0), tier("Silver", 5, 1_500, 2_000), tier("Gold", 15, 1_500, 4_000)]
    }

    #[test]
    fn the_highest_rung_reached_is_the_tier() {
        let l = ladder();
        assert_eq!(tier_for(&l, 0), None);
        assert_eq!(tier_for(&l, 1).map(|t| t.name.as_str()), Some("Bronze"));
        assert_eq!(tier_for(&l, 7).map(|t| t.name.as_str()), Some("Silver"));
        assert_eq!(tier_for(&l, 40).map(|t| t.name.as_str()), Some("Gold"));
    }

    #[test]
    fn the_next_rung_is_the_nearest_above() {
        let l = ladder();
        assert_eq!(next_tier(&l, 7).map(|t| t.name.as_str()), Some("Gold"));
        assert_eq!(next_tier(&l, 15), None);
    }

    #[test]
    fn a_tier_takes_its_share_of_accessorials_only() {
        assert_eq!(tier("Silver", 5, 1_500, 2_000).raw_cents(10_000), 1_500);
    }

    #[test]
    fn a_ladder_is_checked_whole() {
        let ok = vec![NewTier { name: "Silver".into(), min_moves: 5, perk: String::new(), accessorial_bps: 1_500, cap_cents: 2_000, referral_multiplier: 1 }];
        assert!(validate_ladder(&ok).is_ok());

        let mut dup = ok.clone();
        dup.push(NewTier { name: "Gold".into(), ..ok[0].clone() });
        assert!(validate_ladder(&dup).is_err(), "two rungs at one threshold");

        let uncapped = vec![NewTier { cap_cents: 0, ..ok[0].clone() }];
        assert!(validate_ladder(&uncapped).is_err(), "a discount with no cap per move");
    }
}
