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
