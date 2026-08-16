//! Turning a courier position into what a waiting customer is told.
//!
//! Pure by design: no database, no broker, no clock beyond what is passed in.
//! The database-backed tests in this service have never executed against a real
//! Postgres, so the arithmetic a customer reads must be provable without one.
//!
//! Every constant here is a reasoned starting value, not a measured one. They
//! live together in this module so calibrating against real deliveries is a
//! one-file change.

use serde::{Deserialize, Serialize};

use crate::application::services::CourierFix;

/// Straight-line distance flatters a road network. 1.3 is the common urban
/// detour ratio and errs towards over-estimating the wait, which is the right
/// direction to be wrong in.
pub const ROAD_FACTOR: f64 = 1.3;

/// A courier is never usefully slower than this in traffic and never legally
/// faster than this on a city scooter. The floor is what stops a red light
/// reading as an infinite wait; the ceiling stops a bad GPS reading promising
/// a delivery that cannot happen.
pub const MIN_SPEED_KPH: f64 = 8.0;
pub const MAX_SPEED_KPH: f64 = 40.0;

/// Used when no fix carries a speed — the common case today.
pub const DEFAULT_SPEED_KPH: f64 = 18.0;

/// Parking, queueing, and waiting for a bag at each shop still to visit.
pub const DWELL_PER_STOP_MINS: f64 = 4.0;

/// Must match `field_ops::domain::entities::FIX_STALE_AFTER_SECS`. Duplicated
/// rather than shared because the two services do not depend on each other, and
/// a shared crate for one integer would couple them for no gain.
pub const FIX_STALE_AFTER_SECS: i64 = 120;

/// Half-width of the estimate as a fraction of the midpoint, so the range
/// narrows on its own as the distance falls out of the arithmetic.
const SPREAD: f64 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LatLng {
    pub lat: f64,
    pub lng: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct EtaEstimate {
    pub low_minutes:  i64,
    pub high_minutes: i64,
}

/// Great-circle distance in kilometres.
pub fn haversine_km(a: LatLng, b: LatLng) -> f64 {
    const EARTH_RADIUS_KM: f64 = 6371.0088;

    let (lat1, lat2) = (a.lat.to_radians(), b.lat.to_radians());
    let dlat = (b.lat - a.lat).to_radians();
    let dlng = (b.lng - a.lng).to_radians();

    let h = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlng / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_KM * h.sqrt().asin()
}

/// How long until this order reaches the door.
///
/// Whether a fix is recent enough to be shown as live.
///
/// The single definition of "fresh", because there are two consumers and they
/// must not disagree. The ETA refusing a stale fix while the map keeps drawing
/// the dot is the incoherent middle: the courier's phone died, the arrival time
/// quietly vanished, and the screen is still animating a point that has not
/// moved in twenty minutes. Either both go or neither does.
pub fn is_fresh(fix: &CourierFix) -> bool {
    fix.age_seconds <= FIX_STALE_AFTER_SECS
}

/// `remaining_stops` are the pickups not yet collected, in visit order.
/// Returns `None` when the fix is too old to reason from — the caller shows no
/// number at all rather than a stale one, which is the same rule that makes
/// `courier_supply` return null instead of a fabricated count.
pub fn estimate_eta(
    fix: &CourierFix,
    remaining_stops: &[LatLng],
    destination: LatLng,
) -> Option<EtaEstimate> {
    if !is_fresh(fix) {
        return None;
    }

    // Along the route, not straight to the door: a detour to a shop is time.
    let mut km = 0.0;
    let mut from = LatLng { lat: fix.lat, lng: fix.lng };
    for stop in remaining_stops {
        km += haversine_km(from, *stop);
        from = *stop;
    }
    km += haversine_km(from, destination);
    km *= ROAD_FACTOR;

    let speed = fix
        .smoothed_speed_kph
        .filter(|s| s.is_finite() && *s > 0.0)
        .unwrap_or(DEFAULT_SPEED_KPH)
        .clamp(MIN_SPEED_KPH, MAX_SPEED_KPH);

    let travel_mins = (km / speed) * 60.0;
    let dwell_mins  = DWELL_PER_STOP_MINS * remaining_stops.len() as f64;
    let mid         = travel_mins + dwell_mins;

    // The spread is a fraction of the travel component only. Dwell is the part
    // we are most confident about, so padding it would widen the range exactly
    // as the courier arrives — the opposite of narrowing.
    let half = travel_mins * SPREAD;

    let low  = ((mid - half).round() as i64).max(1);
    let high = ((mid + half).round() as i64).max(low);

    Some(EtaEstimate { low_minutes: low, high_minutes: high })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::services::CourierFix;

    const MANILA: LatLng = LatLng { lat: 14.5995, lng: 120.9842 };

    fn fix(lat: f64, lng: f64, speed: Option<f64>, age: i64) -> CourierFix {
        CourierFix { lat, lng, heading_deg: None, smoothed_speed_kph: speed, age_seconds: age }
    }

    #[test]
    fn haversine_matches_a_known_distance() {
        // Manila to Quezon City city hall, ~12 km apart.
        let d = haversine_km(MANILA, LatLng { lat: 14.6760, lng: 121.0437 });
        assert!((d - 11.5).abs() < 1.5, "expected ~11.5 km, got {d}");
    }

    #[test]
    fn a_stale_fix_yields_no_estimate() {
        let f = fix(14.60, 120.99, Some(20.0), FIX_STALE_AFTER_SECS + 1);
        assert!(estimate_eta(&f, &[], MANILA).is_none());
    }

    #[test]
    fn a_fresh_fix_yields_an_estimate() {
        let f = fix(14.60, 120.99, Some(20.0), 10);
        assert!(estimate_eta(&f, &[], MANILA).is_some());
    }

    /// A stopped courier must not produce an unbounded ETA. The clamp is what
    /// turns a red light into a slightly longer wait rather than "4 hours".
    #[test]
    fn a_stopped_courier_clamps_rather_than_diverging() {
        let f = fix(14.6760, 121.0437, Some(0.0), 5);
        let e = estimate_eta(&f, &[], MANILA).expect("an estimate");
        assert!(e.high_minutes < 24 * 60, "clamped, not divergent");
        // 11.5 km x 1.3 at the 8 km/h floor is under two hours.
        assert!(e.high_minutes <= 120, "got {} minutes", e.high_minutes);
    }

    /// The common case while the ingest path carries no speed.
    #[test]
    fn an_unknown_speed_still_estimates() {
        let f = fix(14.60, 120.99, None, 5);
        assert!(estimate_eta(&f, &[], MANILA).is_some());
    }

    /// A courier flagged at an implausible speed must not produce a
    /// one-minute ETA from across the city.
    #[test]
    fn an_implausible_speed_is_clamped_down() {
        let far = fix(14.6760, 121.0437, Some(900.0), 5);
        let e = estimate_eta(&far, &[], MANILA).expect("an estimate");
        assert!(e.low_minutes >= 15, "11.5 km cannot take {} min", e.low_minutes);
    }

    #[test]
    fn the_range_narrows_as_the_courier_closes() {
        let far  = estimate_eta(&fix(14.6760, 121.0437, Some(20.0), 5), &[], MANILA).unwrap();
        let near = estimate_eta(&fix(14.6050, 120.9890, Some(20.0), 5), &[], MANILA).unwrap();

        let far_width  = far.high_minutes - far.low_minutes;
        let near_width = near.high_minutes - near.low_minutes;
        assert!(near_width < far_width, "near {near_width} should be tighter than far {far_width}");
        assert!(near.high_minutes < far.high_minutes);
    }

    /// Uncollected stops are time, not just distance — a shop pickup is not
    /// instantaneous, and an ETA that ignores it is always early.
    #[test]
    fn each_remaining_stop_adds_dwell_time() {
        let f = fix(14.60, 120.99, Some(20.0), 5);
        let none = estimate_eta(&f, &[], MANILA).unwrap();
        let two  = estimate_eta(&f, &[LatLng { lat: 14.601, lng: 120.991 },
                                      LatLng { lat: 14.602, lng: 120.992 }], MANILA).unwrap();
        assert!(two.low_minutes >= none.low_minutes + (2.0 * DWELL_PER_STOP_MINS) as i64);
    }

    /// Distance is measured along the route through the stops, not as one hop
    /// to the door — otherwise a detour to a shop is free.
    #[test]
    fn distance_follows_the_stop_sequence() {
        let f = fix(14.60, 120.99, Some(20.0), 5);
        let direct  = estimate_eta(&f, &[], MANILA).unwrap();
        let detour  = estimate_eta(&f, &[LatLng { lat: 14.70, lng: 121.10 }], MANILA).unwrap();
        assert!(detour.low_minutes > direct.low_minutes + (DWELL_PER_STOP_MINS) as i64,
                "a 20 km detour must cost more than its dwell alone");
    }

    #[test]
    fn an_estimate_is_never_negative_or_zero_width() {
        let f = fix(14.5995, 120.9842, Some(20.0), 0); // already at the door
        let e = estimate_eta(&f, &[], MANILA).unwrap();
        assert!(e.low_minutes >= 1);
        assert!(e.high_minutes >= e.low_minutes);
    }
}
