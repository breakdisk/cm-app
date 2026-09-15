//! Hours of service: how long a driver has been on duty — go-online to
//! go-offline — over a rolling window, against a configured limit.
//!
//! Display and record only. Nothing here refuses work: the limit is a figure the
//! driver sees, not a gate, until enforcement is decided (roadmap
//! `docs/superpowers/plans/2026-09-15-mobile-design-remaining-gaps.md`, default 2).
//!
//! The clock mirrors the platform's own duty state. A driver who closes the app
//! without going off duty is still on duty to dispatch, so they are on duty here
//! too — the fix for that is the status, not the clock.

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

/// Longest window the policy accepts, in hours. Keeps a mistyped config value
/// from overflowing the date arithmetic or reading months of history.
const MAX_WINDOW_HOURS: i64 = 168;

/// One stretch on duty. `ended_at` is `None` while the driver is still on duty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DutySession {
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HosPolicy {
    /// On-duty minutes allowed inside the window (660 = 11 h).
    pub max_on_duty_minutes: i64,
    /// Rolling window length in hours (14).
    pub window_hours: i64,
}

impl Default for HosPolicy {
    fn default() -> Self {
        Self { max_on_duty_minutes: 660, window_hours: 14 }
    }
}

impl HosPolicy {
    /// The window in hours, clamped to 1..=168.
    pub fn window_hours(&self) -> i64 {
        self.window_hours.clamp(1, MAX_WINDOW_HOURS)
    }

    pub fn window(&self) -> Duration {
        Duration::hours(self.window_hours())
    }

    pub fn limit_minutes(&self) -> i64 {
        self.max_on_duty_minutes.max(0)
    }
}

/// What the driver app shows. Serialised as the `GET /v1/drivers/me/hos` body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct HosClock {
    pub window_hours: i64,
    pub limit_minutes: i64,
    pub on_duty_minutes: i64,
    pub remaining_minutes: i64,
    pub over_limit: bool,
    /// Start of the open session — present only while the driver is on duty.
    pub on_duty_since: Option<DateTime<Utc>>,
    /// Minutes since `on_duty_since`; 0 when off duty.
    pub current_stretch_minutes: i64,
    /// Always false today: the clock is shown and recorded, never enforced.
    pub enforced: bool,
}

/// The clock at `now` from a driver's sessions. Only the part of each session
/// inside `[now - window, now]` counts; an open session counts up to `now`, and
/// an end stamped after `now` (clock skew between hosts) is clamped to it.
pub fn hos_clock(sessions: &[DutySession], policy: HosPolicy, now: DateTime<Utc>) -> HosClock {
    let window_start = now - policy.window();
    let mut on_duty_secs: i64 = 0;
    let mut on_duty_since: Option<DateTime<Utc>> = None;

    for session in sessions {
        let start = session.started_at.max(window_start);
        let end = session.ended_at.unwrap_or(now).min(now);
        if end > start {
            on_duty_secs += (end - start).num_seconds();
        }
        if session.ended_at.is_none() && session.started_at <= now {
            on_duty_since = Some(match on_duty_since {
                Some(earlier) => earlier.min(session.started_at),
                None => session.started_at,
            });
        }
    }

    let limit_minutes = policy.limit_minutes();
    let on_duty_minutes = on_duty_secs / 60;
    HosClock {
        window_hours: policy.window_hours(),
        limit_minutes,
        on_duty_minutes,
        remaining_minutes: (limit_minutes - on_duty_minutes).max(0),
        over_limit: on_duty_minutes > limit_minutes,
        on_duty_since,
        current_stretch_minutes: on_duty_since.map_or(0, |since| (now - since).num_minutes().max(0)),
        enforced: false,
    }
}
