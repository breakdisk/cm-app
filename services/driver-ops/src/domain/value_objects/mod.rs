/// Geofence radius around a delivery address — driver must be within this to mark arrival.
pub const ARRIVAL_GEOFENCE_METERS: f64 = 200.0;

/// Location update interval expected from the driver app (seconds).
pub const LOCATION_UPDATE_INTERVAL_SECS: u64 = 10;

/// If no location update received within this window, driver is considered "lost" for dispatch.
pub const STALE_LOCATION_THRESHOLD_MINUTES: i64 = 5;

/// Maximum speed threshold for GPS plausibility check (km/h).
pub const MAX_PLAUSIBLE_SPEED_KMH: f32 = 200.0;

/// Check if a driver is within the geofence of a target location.
pub fn within_geofence(
    driver_lat: f64, driver_lng: f64,
    target_lat: f64, target_lng: f64,
) -> bool {
    let driver = logisticos_types::Coordinates { lat: driver_lat, lng: driver_lng };
    let target = logisticos_types::Coordinates { lat: target_lat, lng: target_lng };
    let distance_m = driver.distance_km(&target) * 1000.0;
    distance_m <= ARRIVAL_GEOFENCE_METERS
}
