//! Leaving an accepted job.
//!
//! Before the grace clock runs out, leaving is a **drop**: the driver pays a
//! share of the job's payout and the job goes back to dispatch. Once the
//! customer has kept them waiting past it, leaving is a **release**: nothing
//! on the driver, and they are paid for the wait.
//!
//! The server decides which, from a deadline it stamped when the driver
//! arrived. The app shows the answer and never works it out: a clock the
//! client keeps is a clock the client can wind forward.

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

/// Every money figure is deployment config and defaults to off. The grace
/// window is the exception: it protects the driver rather than charging anyone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PenaltyPolicy {
    /// Minutes a driver waits at a stop before they may leave for free.
    /// 0 switches the clock off, and leaving is then always a drop.
    pub grace_minutes: i64,
    /// Share of the job's payout charged to the driver on a drop, in percent.
    pub drop_fee_pct: i64,
    /// Paid to the driver per hour waited past grace, in cents. Nothing
    /// charges the customer for it yet, so until that exists it is the
    /// tenant's cost — keep it 0 unless that is intended.
    pub waiting_fee_cents_per_hour: i64,
    /// How many unanswered offers one drop counts as in the acceptance rate.
    /// 1 means the claim is simply taken back.
    pub drop_counts_as_declines: i64,
}

impl PenaltyPolicy {
    /// Out-of-range config is pulled back into range rather than trusted.
    pub fn clamped(self) -> Self {
        Self {
            grace_minutes:              self.grace_minutes.clamp(0, 240),
            drop_fee_pct:               self.drop_fee_pct.clamp(0, 100),
            waiting_fee_cents_per_hour: self.waiting_fee_cents_per_hour.max(0),
            drop_counts_as_declines:    self.drop_counts_as_declines.clamp(1, 10),
        }
    }

    /// When the clock runs out for a driver who arrived at `arrived_at`.
    pub fn grace_deadline(&self, arrived_at: DateTime<Utc>) -> Option<DateTime<Utc>> {
        (self.grace_minutes > 0).then(|| arrived_at + Duration::minutes(self.grace_minutes))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LeaveMode {
    /// Before grace ran out. Fee to the driver; the job goes back to dispatch.
    Drop,
    /// After grace ran out. Free; the stop fails as customer-absent.
    Release,
}

/// Why a driver cannot leave through this path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaveRefusal {
    /// The stop is already done, failed or cancelled.
    TaskClosed,
    /// The load is aboard and the customer is not late. Walking away now
    /// strands their goods; this is a failed delivery, not a drop.
    GoodsAboard,
}

impl LeaveRefusal {
    pub fn code(self) -> &'static str {
        match self {
            Self::TaskClosed  => "TASK_CLOSED",
            Self::GoodsAboard => "GOODS_ABOARD",
        }
    }
}

/// The stop the driver is asking to leave.
#[derive(Debug, Clone, Copy)]
pub struct StopState {
    pub open: bool,
    pub grace_expires_at: Option<DateTime<Utc>>,
    /// The load is on the vehicle: the pickup is done, or there never was one
    /// (a delivery loaded at the hub).
    pub goods_aboard: bool,
}

pub fn leave_mode(stop: &StopState, now: DateTime<Utc>) -> Result<LeaveMode, LeaveRefusal> {
    if !stop.open {
        return Err(LeaveRefusal::TaskClosed);
    }
    // Past grace wins over goods aboard: a driver kept waiting at the door
    // with the load aboard is released, and the stop fails as customer-absent.
    if stop.grace_expires_at.is_some_and(|deadline| now >= deadline) {
        return Ok(LeaveMode::Release);
    }
    if stop.goods_aboard {
        return Err(LeaveRefusal::GoodsAboard);
    }
    Ok(LeaveMode::Drop)
}

/// Rounded down, in the driver's favour.
pub fn drop_fee_cents(payout_cents: i64, pct: i64) -> i64 {
    payout_cents.max(0) * pct.clamp(0, 100) / 100
}

/// Whole seconds past grace at the hourly rate, rounded down (in the
/// customer's favour, the rounding the cancellation fees use).
pub fn waiting_fee_cents(grace_expires_at: DateTime<Utc>, now: DateTime<Utc>, cents_per_hour: i64) -> i64 {
    let seconds = (now - grace_expires_at).num_seconds().max(0);
    seconds * cents_per_hour.max(0) / 3600
}

pub fn minutes_past(deadline: DateTime<Utc>, now: DateTime<Utc>) -> i64 {
    (now - deadline).num_minutes().max(0)
}

/// Acceptance over offers the driver actually saw. A drop takes its claim back
/// and counts as `weight` unanswered offers in all. `None` before any offer
/// has been seen, so a new driver is not shown 0%.
pub fn acceptance_pct(seen: i64, claimed: i64, drops: i64, weight: i64) -> Option<i64> {
    let drops = drops.max(0);
    let denominator = seen.max(0) + (weight.max(1) - 1) * drops;
    if denominator <= 0 {
        return None;
    }
    let accepted = (claimed - drops).max(0);
    Some((accepted * 100 / denominator).clamp(0, 100))
}

/// What leaving costs, stated before the driver commits.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LeaveQuote {
    pub mode: LeaveMode,
    /// Charged to the driver (drop only).
    pub fee_cents: i64,
    /// The payout the fee is a share of.
    pub payout_cents: i64,
    pub fee_pct: i64,
    /// Paid to the driver (release only).
    pub waiting_fee_cents: i64,
    pub waiting_fee_cents_per_hour: i64,
    pub minutes_past_grace: i64,
    pub grace_expires_at: Option<DateTime<Utc>>,
    /// Before and after this leave, when the job came from an offer the driver
    /// claimed. A release never moves it.
    pub acceptance_before: Option<i64>,
    pub acceptance_after: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(minute: i64) -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp(1_800_000_000, 0).unwrap() + Duration::minutes(minute)
    }

    fn stop(grace_at: Option<i64>, goods_aboard: bool) -> StopState {
        StopState { open: true, grace_expires_at: grace_at.map(at), goods_aboard }
    }

    #[test]
    fn leaving_before_arrival_is_a_drop() {
        assert_eq!(leave_mode(&stop(None, false), at(0)), Ok(LeaveMode::Drop));
    }

    #[test]
    fn leaving_inside_grace_is_a_drop() {
        assert_eq!(leave_mode(&stop(Some(45), false), at(44)), Ok(LeaveMode::Drop));
    }

    /// The deadline itself is past grace, not inside it.
    #[test]
    fn leaving_at_or_after_the_deadline_is_a_release() {
        assert_eq!(leave_mode(&stop(Some(45), false), at(45)), Ok(LeaveMode::Release));
        assert_eq!(leave_mode(&stop(Some(45), false), at(90)), Ok(LeaveMode::Release));
    }

    #[test]
    fn a_waiting_driver_with_the_load_aboard_is_released_not_trapped() {
        assert_eq!(leave_mode(&stop(Some(45), true), at(50)), Ok(LeaveMode::Release));
    }

    #[test]
    fn the_load_aboard_and_the_customer_not_late_is_refused() {
        assert_eq!(leave_mode(&stop(Some(45), true), at(10)), Err(LeaveRefusal::GoodsAboard));
        assert_eq!(leave_mode(&stop(None, true), at(10)), Err(LeaveRefusal::GoodsAboard));
    }

    #[test]
    fn a_closed_stop_cannot_be_left() {
        let closed = StopState { open: false, grace_expires_at: Some(at(0)), goods_aboard: false };
        assert_eq!(leave_mode(&closed, at(90)), Err(LeaveRefusal::TaskClosed));
    }

    #[test]
    fn no_clock_when_grace_is_off() {
        let off = PenaltyPolicy { grace_minutes: 0, drop_fee_pct: 20, waiting_fee_cents_per_hour: 0, drop_counts_as_declines: 1 };
        assert_eq!(off.grace_deadline(at(0)), None);
        let on = PenaltyPolicy { grace_minutes: 45, ..off };
        assert_eq!(on.grace_deadline(at(0)), Some(at(45)));
    }

    /// The design's mock: 20% of $214 is $42.80, shown as $43. Rounded down.
    #[test]
    fn the_drop_fee_is_a_share_of_the_payout_rounded_down() {
        assert_eq!(drop_fee_cents(21_400, 20), 4_280);
        assert_eq!(drop_fee_cents(999, 20), 199);
        assert_eq!(drop_fee_cents(21_400, 0), 0);
        assert_eq!(drop_fee_cents(-5, 20), 0);
    }

    /// The design's mock: 3 minutes past grace at $38/h is $1.90.
    #[test]
    fn the_waiting_fee_accrues_by_the_second_past_grace() {
        assert_eq!(waiting_fee_cents(at(45), at(48), 3_800), 190);
        assert_eq!(waiting_fee_cents(at(45), at(40), 3_800), 0, "not before grace");
        assert_eq!(waiting_fee_cents(at(45), at(105), 0), 0, "off");
    }

    #[test]
    fn acceptance_takes_the_claim_back_and_weights_the_drop() {
        // 50 seen, 47 claimed: 94%.
        assert_eq!(acceptance_pct(50, 47, 0, 3), Some(94));
        // One drop at weight 3: 46 of 52.
        assert_eq!(acceptance_pct(50, 47, 1, 3), Some(88));
        // At weight 1 the claim is only taken back: 46 of 50.
        assert_eq!(acceptance_pct(50, 47, 1, 1), Some(92));
    }

    #[test]
    fn no_acceptance_rate_before_any_offer_is_seen() {
        assert_eq!(acceptance_pct(0, 0, 0, 3), None);
    }

    #[test]
    fn config_out_of_range_is_pulled_back() {
        let wild = PenaltyPolicy { grace_minutes: -5, drop_fee_pct: 150, waiting_fee_cents_per_hour: -1, drop_counts_as_declines: 0 };
        assert_eq!(
            wild.clamped(),
            PenaltyPolicy { grace_minutes: 0, drop_fee_pct: 100, waiting_fee_cents_per_hour: 0, drop_counts_as_declines: 1 }
        );
    }
}
