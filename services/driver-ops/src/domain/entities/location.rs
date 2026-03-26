use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single GPS location record from a driver device.
/// Stored in TimescaleDB for time-series queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverLocation {
    pub driver_id: uuid::Uuid,
    pub tenant_id: uuid::Uuid,
    pub lat: f64,
    pub lng: f64,
    pub accuracy_m: Option<f32>,
    pub speed_kmh: Option<f32>,
    pub heading: Option<f32>,     // degrees 0-360
    pub battery_pct: Option<u8>,
    pub recorded_at: DateTime<Utc>,
    pub received_at: DateTime<Utc>,
}

impl DriverLocation {
    /// Business rule: reject location updates older than 5 minutes (stale GPS from offline sync).
    pub fn is_stale(&self) -> bool {
        let age = Utc::now() - self.recorded_at;
        age.num_minutes() > 5
    }

    /// Business rule: reject suspiciously fast movements (> 200 km/h = likely GPS noise).
    pub fn is_plausible_speed(&self) -> bool {
        self.speed_kmh.map_or(true, |s| s <= 200.0)
    }
}
