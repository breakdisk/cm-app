//! Itemised rate-card pricing.
//!
//! The consumer Review screen renders base / distance / weight as three rows and
//! states a total. Those four numbers must agree, so they come from one function.
//! `quote_price_cents` delegates here rather than repeating the arithmetic.

use serde::{Deserialize, Serialize};

use super::VehicleListing;

/// One priced move, itemised. Every field is a row on the Review screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuoteBreakdown {
    /// The listing's callout fee.
    pub base_cents: i64,
    /// `per_km_cents × distance_km`, rounded.
    pub distance_cents: i64,
    /// `per_kg_cents × billable_kg`, rounded. Zero when the listing has no
    /// per-kg rate — a zero row, never an absent one.
    pub weight_cents: i64,
    /// The sum of the three above. Never computed independently of them.
    pub total_cents: i64,
}

/// Price `listing` for `distance_km` and `weight_kg`, itemised.
///
/// Negative inputs clamp to zero: a bad distance must never subtract from the
/// base fee.
pub fn quote_breakdown_cents(
    listing: &VehicleListing,
    distance_km: f32,
    weight_kg: f32,
) -> QuoteBreakdown {
    let distance = distance_km.max(0.0) as f64;
    let weight = weight_kg.max(0.0) as f64;

    let base_cents = listing.base_price_cents;
    let distance_cents = (listing.per_km_cents as f64 * distance).round() as i64;
    let weight_cents = listing
        .per_kg_cents
        .map(|c| (c as f64 * weight).round() as i64)
        .unwrap_or(0);

    let total_cents = base_cents
        .saturating_add(distance_cents)
        .saturating_add(weight_cents);

    QuoteBreakdown { base_cents, distance_cents, weight_cents, total_cents }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::{ListingStatus, SizeClass, VehicleListing};
    use chrono::Utc;
    use uuid::Uuid;

    fn listing(base: i64, per_km: i64, per_kg: Option<i64>) -> VehicleListing {
        VehicleListing {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            carrier_id: Uuid::new_v4(),
            vehicle_plate: "TEST-1".into(),
            size_class: SizeClass::Van,
            max_weight_kg: 1000.0,
            max_volume_m3: Some(8.0),
            base_price_cents: base,
            per_km_cents: per_km,
            per_kg_cents: per_kg,
            service_area_label: "Metro".into(),
            idle_from: Utc::now(),
            idle_until: Utc::now(),
            status: ListingStatus::Active,
            carrier_response_window_mins: 15,
            bookings_today: 0,
            revenue_today_cents: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    /// The invariant the Review screen renders: the three rows sum to the total,
    /// with no residue. If this can fail, the screen can show a total that does
    /// not match its own line items.
    #[test]
    fn the_rows_sum_to_the_total_exactly() {
        let b = quote_breakdown_cents(&listing(45_000, 240, Some(22)), 7.4, 320.0);
        assert_eq!(b.base_cents, 45_000);
        assert_eq!(b.distance_cents, 1_776); // 240 * 7.4 = 1776
        assert_eq!(b.weight_cents, 7_040); // 22 * 320
        assert_eq!(
            b.base_cents + b.distance_cents + b.weight_cents,
            b.total_cents,
            "a row exists that is not in the total, or vice versa"
        );
    }

    /// A listing with no per-kg rate must emit a zero row, not omit the row --
    /// the screen renders from the struct, and a missing field renders nothing.
    #[test]
    fn a_listing_without_a_per_kg_rate_still_emits_the_row() {
        let b = quote_breakdown_cents(&listing(45_000, 240, None), 7.4, 320.0);
        assert_eq!(b.weight_cents, 0);
        assert_eq!(b.total_cents, 46_776);
    }

    /// Negative inputs are clamped, not propagated -- a negative distance must
    /// never subtract from the base fee.
    #[test]
    fn negative_inputs_clamp_to_zero() {
        let b = quote_breakdown_cents(&listing(45_000, 240, Some(22)), -5.0, -10.0);
        assert_eq!(b.distance_cents, 0);
        assert_eq!(b.weight_cents, 0);
        assert_eq!(b.total_cents, 45_000);
    }

    /// The old entry point must return the same number as the new one, or the
    /// marketplace booking path and the consumer quote path will disagree.
    #[test]
    fn quote_price_cents_agrees_with_the_breakdown_total() {
        let l = listing(45_000, 240, Some(22));
        assert_eq!(
            crate::domain::entities::quote_price_cents(&l, 7.4, 320.0),
            quote_breakdown_cents(&l, 7.4, 320.0).total_cents
        );
    }
}
