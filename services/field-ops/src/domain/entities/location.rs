use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CourierLocation {
    pub id:               Uuid,
    pub tenant_id:        Uuid,
    pub courier_id:       Uuid,
    pub lat:              f64,
    pub lng:              f64,
    pub accuracy_m:       Option<f32>,
    pub speed_kph:        Option<f32>,
    pub heading_deg:      Option<f32>,
    /// Hardware clock at the physical moment of capture. Primary time basis for
    /// SLA maths; `None` only for server-generated points.
    pub device_timestamp: Option<DateTime<Utc>>,
    pub recorded_at:      DateTime<Utc>,
}

impl CourierLocation {
    pub fn new(
        tenant_id: Uuid,
        courier_id: Uuid,
        lat: f64,
        lng: f64,
        device_timestamp: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            courier_id,
            lat,
            lng,
            accuracy_m: None,
            speed_kph: None,
            heading_deg: None,
            device_timestamp,
            recorded_at: Utc::now(),
        }
    }

    /// The timestamp SLA and velocity calculations should use: the device clock
    /// where we have it, backend receipt time only as a fallback.
    pub fn sla_timestamp(&self) -> DateTime<Utc> {
        self.device_timestamp.unwrap_or(self.recorded_at)
    }
}

/// How old a fix may be before it stops being "live".
///
/// Two minutes is chosen against the app's own poll cadence: the tracking
/// screen refreshes every 5s while delivering, so a fix this old means many
/// consecutive missed reports, not one dropped packet.
pub const FIX_STALE_AFTER_SECS: i64 = 120;

/// Weight given to the newest reading in the speed EWMA.
const SPEED_SMOOTHING_ALPHA: f64 = 0.5;

impl CourierLocation {
    /// Seconds since this fix was taken, floored at zero.
    ///
    /// Floored deliberately: a device clock running fast would otherwise
    /// produce a negative age, and a negative age compares as fresher than a
    /// live fix in every staleness check.
    pub fn age_seconds(&self, now: DateTime<Utc>) -> i64 {
        (now - self.sla_timestamp()).num_seconds().max(0)
    }

    pub fn is_stale(&self, now: DateTime<Utc>) -> bool {
        self.age_seconds(now) > FIX_STALE_AFTER_SECS
    }
}

/// Exponentially weighted mean speed over recent fixes, newest first.
///
/// Returns `None` when no reading carries a speed — which is the common case
/// while the ingest path does not populate it. `None` means "unknown" and the
/// caller falls back to a default; returning `0.0` would instead assert the
/// courier is stopped and drive every ETA to its slowest clamp.
pub fn smoothed_speed_kph(fixes: &[CourierLocation]) -> Option<f64> {
    let mut acc: Option<f64> = None;
    // Oldest first, so each newer reading gets the heavier weight.
    for f in fixes.iter().rev() {
        let Some(s) = f.speed_kph else { continue };
        let s = s as f64;
        acc = Some(match acc {
            None => s,
            Some(prev) => SPEED_SMOOTHING_ALPHA * s + (1.0 - SPEED_SMOOTHING_ALPHA) * prev,
        });
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sla_timestamp_prefers_the_device_clock() {
        let device = Utc::now() - chrono::Duration::seconds(45);
        let l = CourierLocation::new(Uuid::new_v4(), Uuid::new_v4(), 14.6, 120.98, Some(device));
        assert_eq!(l.sla_timestamp(), device, "device_timestamp must win when present");
        assert_ne!(l.sla_timestamp(), l.recorded_at);
    }

    #[test]
    fn sla_timestamp_falls_back_to_receipt_time_for_server_points() {
        let l = CourierLocation::new(Uuid::new_v4(), Uuid::new_v4(), 14.6, 120.98, None);
        assert_eq!(l.sla_timestamp(), l.recorded_at);
    }

    fn at(secs_ago: i64, speed: Option<f32>) -> CourierLocation {
        let mut l = CourierLocation::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            14.5995,
            120.9842,
            Some(Utc::now() - chrono::Duration::seconds(secs_ago)),
        );
        l.speed_kph = speed;
        l
    }

    #[test]
    fn age_is_measured_from_the_device_clock() {
        let now = Utc::now();
        let l = at(45, None);
        assert!((l.age_seconds(now) - 45).abs() <= 1);
    }

    /// A clock skewed into the future must not read as a negative age, which
    /// would make a stale fix look fresher than a live one.
    #[test]
    fn a_future_fix_reads_as_zero_age_not_negative() {
        let now = Utc::now();
        let l = at(-30, None);
        assert_eq!(l.age_seconds(now), 0);
    }

    #[test]
    fn a_fix_older_than_the_window_is_stale() {
        let now = Utc::now();
        assert!(!at(FIX_STALE_AFTER_SECS - 10, None).is_stale(now));
        assert!(at(FIX_STALE_AFTER_SECS + 10, None).is_stale(now));
    }

    /// Weighted towards the newest reading, so pulling away from a kerb shows
    /// up faster than it would in a flat mean.
    #[test]
    fn smoothing_weights_the_newest_reading_hardest() {
        // newest first, as the repository returns them
        let fixes = vec![at(0, Some(30.0)), at(10, Some(10.0)), at(20, Some(10.0))];
        let s = smoothed_speed_kph(&fixes).expect("some speed");
        let flat_mean = (30.0 + 10.0 + 10.0) / 3.0;
        assert!(s > flat_mean, "EWMA {s} should exceed the flat mean {flat_mean}");
        assert!(s < 30.0, "and still be pulled down by the older readings");
    }

    /// The common case today: the ingest route may carry no speed at all, so
    /// this must answer "unknown" rather than zero. Zero would read as a
    /// stopped courier and push every ETA to its clamp.
    #[test]
    fn no_speed_readings_at_all_is_unknown_not_zero() {
        assert_eq!(smoothed_speed_kph(&[at(0, None), at(10, None)]), None);
        assert_eq!(smoothed_speed_kph(&[]), None);
    }

    #[test]
    fn readings_without_speed_are_skipped_not_counted_as_zero() {
        let fixes = vec![at(0, Some(20.0)), at(10, None), at(20, Some(20.0))];
        assert_eq!(smoothed_speed_kph(&fixes), Some(20.0));
    }
}
