/// Maximum stops per route — configurable per subscription tier in future.
/// Starter: 20, Growth: 50, Business/Enterprise: 100.
pub const MAX_STOPS_STARTER: usize = 20;
pub const MAX_STOPS_GROWTH: usize = 50;
pub const MAX_STOPS_BUSINESS: usize = 100;

/// Maximum search radius when auto-assigning the nearest driver.
pub const DEFAULT_DRIVER_SEARCH_RADIUS_KM: f64 = 25.0;

/// Speed estimate for ETA calculations (urban Philippine roads).
pub const AVERAGE_SPEED_KMH: f64 = 30.0;

/// Buffer added per stop for load/unload time (minutes).
pub const STOP_SERVICE_MINUTES: f64 = 5.0;

/// Compute estimated route duration from total distance and stop count.
pub fn estimate_duration_minutes(distance_km: f64, stop_count: usize) -> u32 {
    let drive_minutes = (distance_km / AVERAGE_SPEED_KMH) * 60.0;
    let service_minutes = stop_count as f64 * STOP_SERVICE_MINUTES;
    (drive_minutes + service_minutes).ceil() as u32
}

// ── Vehicle capacity matching ────────────────────────────────────────────────
// Auto-dispatch must not offer a 40 kg balikbayan box to a motorcycle rider.
// Class limits are deliberately coarse — the goal is to stop obvious
// mismatches, not to model payload manifests.

/// Payload ceiling per vehicle class, in grams.
const MOTORCYCLE_MAX_GRAMS: i64 = 20_000;      // 20 kg
const CAR_MAX_GRAMS: i64        = 200_000;     // 200 kg
const VAN_MAX_GRAMS: i64        = 1_000_000;   // 1 t

/// Returns `true` when a vehicle of `vehicle_type` can carry a shipment of
/// `weight_grams` / `delivery_category`.
///
/// Rules:
/// - motorcycle/bike-class: ≤ 20 kg and never "heavy" or "large" categories
/// - sedan/car-class:       ≤ 200 kg and never "large" (big shipment)
/// - van/pickup-class:      ≤ 1 t
/// - truck-class:           unlimited
/// - unknown / NULL:        treated as car-class (conservative middle ground —
///   most field vehicles without a recorded type are cars or small MPVs)
pub fn vehicle_can_carry(
    vehicle_type: Option<&str>,
    weight_grams: i64,
    delivery_category: &str,
) -> bool {
    let class = vehicle_type.unwrap_or("").trim().to_lowercase();
    match class.as_str() {
        "motorcycle" | "bike" | "bicycle" | "scooter" =>
            weight_grams <= MOTORCYCLE_MAX_GRAMS
                && delivery_category != "heavy"
                && delivery_category != "large",
        "van" | "pickup" | "l300" =>
            weight_grams <= VAN_MAX_GRAMS,
        "truck" | "lorry" | "6wheeler" | "10wheeler" =>
            true,
        // sedan / car / mpv / suv / unknown
        _ =>
            weight_grams <= CAR_MAX_GRAMS && delivery_category != "large",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motorcycle_rejects_heavy_and_large() {
        assert!(vehicle_can_carry(Some("motorcycle"), 5_000, "food"));
        assert!(!vehicle_can_carry(Some("motorcycle"), 25_000, "parcel"));
        assert!(!vehicle_can_carry(Some("motorcycle"), 5_000, "heavy"));
        assert!(!vehicle_can_carry(Some("motorcycle"), 5_000, "large"));
    }

    #[test]
    fn car_rejects_large_category_and_over_200kg() {
        assert!(vehicle_can_carry(Some("sedan"), 150_000, "parcel"));
        assert!(!vehicle_can_carry(Some("sedan"), 250_000, "parcel"));
        assert!(!vehicle_can_carry(Some("sedan"), 10_000, "large"));
    }

    #[test]
    fn truck_carries_anything_and_unknown_is_car_class() {
        assert!(vehicle_can_carry(Some("truck"), 5_000_000, "large"));
        assert!(vehicle_can_carry(None, 100_000, "parcel"));
        assert!(!vehicle_can_carry(None, 10_000, "large"));
    }
}
