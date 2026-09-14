//! Cancellation pricing: which tier a cancellation falls in, and at what rate.
//!
//! Decided 2026-09-14:
//! - The policy prices **scheduled** jobs only. A shipment with no
//!   `scheduled_pickup_at`, which is every shipment booked without a slot, keeps
//!   the old rule: free, and only before pickup assignment.
//! - 24 to 48 hours out is charged the late rate.
//! - Every rate defaults to 0 bps until finance confirms the numbers, so
//!   deploying this changes nobody's money. Turning fees on is config.
//! - Fees round down, in the customer's favour.
//!
//! order-intake states only the **rate** (`retention_bps` on
//! `shipment.cancelled`). Payments applies it to what it actually captured. The
//! amounts here are an estimate for the preview screen, from the quoted total.

use chrono::{DateTime, Utc};
use serde::Deserialize;

/// 100%, in basis points.
pub const FULL_RETENTION_BPS: i64 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelTier {
    /// No scheduled time. The policy does not price it.
    Unscheduled,
    Early,
    Late,
    SameDay,
}

impl CancelTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unscheduled => "unscheduled",
            Self::Early => "early",
            Self::Late => "late",
            Self::SameDay => "same_day",
        }
    }
}

/// Env: `CANCELLATION_POLICY__LATE_BPS`, `CANCELLATION_POLICY__SAME_DAY_BPS`,
/// `CANCELLATION_POLICY__FREE_UNTIL_HOURS`, `CANCELLATION_POLICY__LATE_FROM_HOURS`,
/// `CANCELLATION_POLICY__VERSION`.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CancellationPolicy {
    pub version: String,
    /// More than this many hours before the scheduled start is free.
    pub free_until_hours: i64,
    /// Documents where the under-24-hour band starts. Under 24 hours and 24 to
    /// 48 hours are both charged `late_bps`, by decision.
    pub late_from_hours: i64,
    pub late_bps: i64,
    /// At or after the scheduled start.
    pub same_day_bps: i64,
}

impl Default for CancellationPolicy {
    fn default() -> Self {
        Self {
            version: "2026-09".into(),
            free_until_hours: 48,
            late_from_hours: 24,
            late_bps: 0,
            same_day_bps: 0,
        }
    }
}

impl CancellationPolicy {
    /// Refuse a rate outside 0 to 100%, or windows that do not nest. Called at
    /// startup so a bad policy stops the deploy instead of every cancellation.
    pub fn validate(&self) -> Result<(), String> {
        let rate_ok = |bps: i64| (0..=FULL_RETENTION_BPS).contains(&bps);
        if !rate_ok(self.late_bps) || !rate_ok(self.same_day_bps) {
            return Err(format!(
                "cancellation rates must be 0..={FULL_RETENTION_BPS} bps (late {}, same day {})",
                self.late_bps, self.same_day_bps
            ));
        }
        if self.late_from_hours < 0 || self.free_until_hours < self.late_from_hours {
            return Err(format!(
                "cancellation windows must nest: late_from_hours {} <= free_until_hours {}",
                self.late_from_hours, self.free_until_hours
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancellationQuote {
    pub tier: CancelTier,
    /// What payments retains, as a share of what it captured.
    pub retention_bps: i64,
    pub minutes_to_pickup: Option<i64>,
    /// Estimate from the quoted booking total. `None` without one.
    pub fee_cents: Option<i64>,
    pub refund_cents: Option<i64>,
}

pub fn quote_cancellation(
    policy: &CancellationPolicy,
    scheduled_pickup_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    booking_amount_cents: Option<i64>,
) -> CancellationQuote {
    let Some(start) = scheduled_pickup_at else {
        return CancellationQuote {
            tier: CancelTier::Unscheduled,
            retention_bps: 0,
            minutes_to_pickup: None,
            fee_cents: None,
            refund_cents: None,
        };
    };

    // Minutes, not hours: num_hours() truncates, and 23h59m must not read as 23h.
    let minutes_out = (start - now).num_minutes();

    let (tier, bps) = if minutes_out <= 0 {
        (CancelTier::SameDay, policy.same_day_bps)
    } else if minutes_out < policy.free_until_hours * 60 {
        // Under 24 hours and 24 to 48 hours are both the late rate.
        (CancelTier::Late, policy.late_bps)
    } else {
        (CancelTier::Early, 0)
    };
    let retention_bps = bps.clamp(0, FULL_RETENTION_BPS);

    // Round down, in the customer's favour. i128 so booking x bps cannot overflow.
    let fee_cents = booking_amount_cents
        .map(|b| (b.max(0) as i128 * retention_bps as i128 / FULL_RETENTION_BPS as i128) as i64);
    let refund_cents = booking_amount_cents.zip(fee_cents).map(|(b, fee)| b.max(0) - fee);

    CancellationQuote { tier, retention_bps, minutes_to_pickup: Some(minutes_out), fee_cents, refund_cents }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    /// Placeholder rates, only for exercising the tiers. The shipped default is
    /// 0% everywhere until finance confirms the numbers.
    fn live() -> CancellationPolicy {
        CancellationPolicy {
            version: "2026-09".into(),
            free_until_hours: 48,
            late_from_hours: 24,
            late_bps: 1_500,
            same_day_bps: 10_000,
        }
    }

    /// Decided 2026-09-14: the policy prices scheduled jobs only. Every parcel
    /// booked today has no scheduled time and keeps the old rule.
    #[test]
    fn an_unscheduled_shipment_is_not_priced() {
        let q = quote_cancellation(&live(), None, Utc::now(), Some(20_000));
        assert_eq!(q.tier, CancelTier::Unscheduled);
        assert_eq!(q.retention_bps, 0);
        assert_eq!(q.minutes_to_pickup, None);
    }

    #[test]
    fn more_than_48_hours_out_is_free() {
        let now = Utc::now();
        let q = quote_cancellation(&live(), Some(now + Duration::hours(60)), now, Some(20_000));
        assert_eq!(
            (q.tier, q.retention_bps, q.fee_cents, q.refund_cents),
            (CancelTier::Early, 0, Some(0), Some(20_000))
        );
    }

    #[test]
    fn inside_24_hours_is_the_late_rate() {
        let now = Utc::now();
        let q = quote_cancellation(&live(), Some(now + Duration::hours(19)), now, Some(20_000));
        assert_eq!(
            (q.tier, q.retention_bps, q.fee_cents, q.refund_cents),
            (CancelTier::Late, 1_500, Some(3_000), Some(17_000))
        );
    }

    /// Decided 2026-09-14: 24–48 hours is charged the late rate, matching the
    /// booking screen's "Free until 48 hours before move day".
    #[test]
    fn the_24_to_48_hour_band_is_the_late_rate_too() {
        let now = Utc::now();
        let q = quote_cancellation(&live(), Some(now + Duration::hours(30)), now, Some(20_000));
        assert_eq!((q.tier, q.retention_bps), (CancelTier::Late, 1_500));
    }

    #[test]
    fn at_or_after_the_scheduled_start_is_same_day() {
        let now = Utc::now();
        for start in [now, now - Duration::minutes(1)] {
            let q = quote_cancellation(&live(), Some(start), now, Some(20_000));
            assert_eq!(
                (q.tier, q.retention_bps, q.fee_cents, q.refund_cents),
                (CancelTier::SameDay, 10_000, Some(20_000), Some(0))
            );
        }
    }

    /// Hours truncate; 23h59m must not read as outside the late window, and
    /// exactly 48h out is the first free minute.
    #[test]
    fn the_boundaries_are_measured_in_minutes() {
        let now = Utc::now();
        let inside = quote_cancellation(&live(), Some(now + Duration::minutes(24 * 60 - 1)), now, Some(20_000));
        assert_eq!(inside.tier, CancelTier::Late);
        let edge = quote_cancellation(&live(), Some(now + Duration::minutes(48 * 60)), now, Some(20_000));
        assert_eq!(edge.tier, CancelTier::Early);
    }

    /// Decided 2026-09-14: round down, in the customer's favour.
    #[test]
    fn fees_round_down_in_the_customers_favour() {
        let now = Utc::now();
        let q = quote_cancellation(&live(), Some(now + Duration::hours(19)), now, Some(101));
        assert_eq!((q.fee_cents, q.refund_cents), (Some(15), Some(86)));
    }

    /// Decided 2026-09-14: ship at 0% until rates are confirmed, so a deploy
    /// changes nobody's money.
    #[test]
    fn the_default_policy_charges_nothing() {
        let now = Utc::now();
        let q = quote_cancellation(&CancellationPolicy::default(), Some(now + Duration::hours(19)), now, Some(20_000));
        assert_eq!((q.tier, q.retention_bps, q.fee_cents), (CancelTier::Late, 0, Some(0)));
    }

    /// The rate is what payments acts on. The amount is display only, and a
    /// cash booking has none to show.
    #[test]
    fn without_a_booking_amount_there_is_a_rate_but_no_amount() {
        let now = Utc::now();
        let q = quote_cancellation(&live(), Some(now + Duration::hours(19)), now, None);
        assert_eq!((q.retention_bps, q.fee_cents, q.refund_cents), (1_500, None, None));
    }

    /// A bad policy must not boot.
    #[test]
    fn a_rate_above_100_percent_or_crossed_windows_are_refused() {
        assert!(CancellationPolicy::default().validate().is_ok());
        let too_high = CancellationPolicy { late_bps: FULL_RETENTION_BPS + 1, ..Default::default() };
        assert!(too_high.validate().is_err());
        let crossed = CancellationPolicy { free_until_hours: 12, ..Default::default() };
        assert!(crossed.validate().is_err(), "free window inside the late window");
    }
}
