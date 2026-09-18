//! A promo code a tenant offers, and whether one account may use it now.
//!
//! Eligibility is decided here, per account, and never by the app: an app that
//! decides who sees an offer is an app that can be told to see all of them.

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use uuid::Uuid;

use super::window::{WindowRefusal, WindowRule};

/// What a code takes off. Percentages apply to carriage only — the design's
/// `MOVE20` is "20% off carriage" — so a code never discounts the helpers or
/// packing a customer adds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Discount {
    /// Basis points of the carriage line: 2000 = 20%.
    PercentCarriage { bps: i64 },
    /// A flat amount, in the move's currency.
    Flat { cents: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Offer {
    pub id: Uuid,
    pub tenant_id: Uuid,
    /// Stored and matched upper-case.
    pub code: String,
    pub title: String,
    pub body: String,
    pub discount: Discount,
    /// The most this code takes off one move, before the ceiling. None = only
    /// the ceiling limits it.
    pub cap_cents: Option<i64>,
    /// Subject to the monthly window and the one-code-a-month rule.
    pub windowed: bool,
    /// Usable once per account, ever (a welcome code).
    pub once_per_account: bool,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub active: bool,
    /// Whose budget pays: "marketing", "acquisition" or "loyalty". Finance
    /// attributes discount spend by it.
    pub budget_tag: String,
}

impl Offer {
    /// What the code takes off before the ceiling: its own rule and its own cap.
    pub fn raw_cents(&self, carriage_cents: i64) -> i64 {
        let raw = match self.discount {
            Discount::PercentCarriage { bps } => carriage_cents.max(0) * bps.clamp(0, 10_000) / 10_000,
            Discount::Flat { cents } => cents.max(0),
        };
        match self.cap_cents {
            Some(cap) => raw.min(cap.max(0)),
            None => raw,
        }
    }

    pub fn live_at(&self, now: DateTime<Utc>) -> bool {
        self.active
            && self.starts_at.is_none_or(|s| now >= s)
            && self.ends_at.is_none_or(|e| now < e)
    }
}

pub fn normalize_code(code: &str) -> String {
    code.trim().to_ascii_uppercase()
}

/// What an account has already used.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccountHistory {
    /// A windowed code was redeemed this calendar month (and not released).
    pub windowed_used_this_month: bool,
    /// Offers this account has redeemed and not had released.
    pub offers_used: Vec<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeRefusal {
    Unknown,
    NotLive,
    Window(WindowRefusal),
    UsedThisMonth,
    AlreadyUsed,
}

impl CodeRefusal {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unknown => "UNKNOWN_CODE",
            Self::NotLive => "CODE_NOT_LIVE",
            Self::Window(WindowRefusal::Weekend) => "OUTSIDE_WINDOW_WEEKEND",
            Self::Window(WindowRefusal::OutsideDays { .. }) => "OUTSIDE_WINDOW_DAYS",
            Self::UsedThisMonth => "ALREADY_REDEEMED_THIS_MONTH",
            Self::AlreadyUsed => "NOT_ELIGIBLE_FOR_ACCOUNT",
        }
    }

    /// Which rule it failed, in the words the app shows.
    pub fn message(&self) -> String {
        match self {
            Self::Unknown => "That code isn't one we recognise.".into(),
            Self::NotLive => "That code isn't running right now.".into(),
            Self::Window(w) => w.message(),
            Self::UsedThisMonth => "You've already used a code this month. The next window opens next month.".into(),
            Self::AlreadyUsed => "This code has already been used on your account.".into(),
        }
    }
}

/// May this account use this code today?
pub fn eligibility(
    offer: Option<&Offer>,
    now: DateTime<Utc>,
    local_day: NaiveDate,
    window: &WindowRule,
    history: &AccountHistory,
) -> Result<(), CodeRefusal> {
    let offer = offer.ok_or(CodeRefusal::Unknown)?;
    if !offer.live_at(now) {
        return Err(CodeRefusal::NotLive);
    }
    if offer.once_per_account && history.offers_used.contains(&offer.id) {
        return Err(CodeRefusal::AlreadyUsed);
    }
    if offer.windowed {
        window.check(local_day).map_err(CodeRefusal::Window)?;
        if history.windowed_used_this_month {
            return Err(CodeRefusal::UsedThisMonth);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn offer(discount: Discount, cap: Option<i64>) -> Offer {
        Offer {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            code: "MOVE20".into(),
            title: "Autumn move-in".into(),
            body: String::new(),
            discount,
            cap_cents: cap,
            windowed: true,
            once_per_account: false,
            starts_at: None,
            ends_at: None,
            active: true,
            budget_tag: "marketing".into(),
        }
    }

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-15T03:00:00Z").unwrap().with_timezone(&Utc)
    }

    const WINDOW: WindowRule = WindowRule { from_day: 11, to_day: 24 };

    fn tuesday() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 15).unwrap()
    }

    /// The design's MOVE20: 20% of carriage, capped at 25.
    #[test]
    fn a_percentage_is_of_carriage_and_held_to_the_codes_cap() {
        let o = offer(Discount::PercentCarriage { bps: 2_000 }, Some(2_500));
        assert_eq!(o.raw_cents(10_000), 2_000);
        assert_eq!(o.raw_cents(30_000), 2_500);
    }

    #[test]
    fn a_flat_code_is_its_amount() {
        let o = offer(Discount::Flat { cents: 1_400 }, None);
        assert_eq!(o.raw_cents(0), 1_400);
    }

    #[test]
    fn inside_the_window_and_unused_it_applies() {
        let o = offer(Discount::Flat { cents: 1_400 }, None);
        assert_eq!(eligibility(Some(&o), now(), tuesday(), &WINDOW, &AccountHistory::default()), Ok(()));
    }

    #[test]
    fn one_windowed_code_a_month() {
        let o = offer(Discount::Flat { cents: 1_400 }, None);
        let used = AccountHistory { windowed_used_this_month: true, ..Default::default() };
        assert_eq!(eligibility(Some(&o), now(), tuesday(), &WINDOW, &used), Err(CodeRefusal::UsedThisMonth));
    }

    #[test]
    fn a_code_outside_the_window_says_which_rule() {
        let o = offer(Discount::Flat { cents: 1_400 }, None);
        let saturday = NaiveDate::from_ymd_opt(2026, 9, 19).unwrap();
        assert_eq!(
            eligibility(Some(&o), now(), saturday, &WINDOW, &AccountHistory::default()),
            Err(CodeRefusal::Window(WindowRefusal::Weekend))
        );
    }

    /// A welcome code is not windowed, but is once per account.
    #[test]
    fn a_once_per_account_code_is_used_once() {
        let mut o = offer(Discount::Flat { cents: 1_500 }, None);
        o.windowed = false;
        o.once_per_account = true;
        let used = AccountHistory { offers_used: vec![o.id], ..Default::default() };
        assert_eq!(eligibility(Some(&o), now(), tuesday(), &WINDOW, &used), Err(CodeRefusal::AlreadyUsed));
        let saturday = NaiveDate::from_ymd_opt(2026, 9, 19).unwrap();
        assert_eq!(eligibility(Some(&o), now(), saturday, &WINDOW, &AccountHistory::default()), Ok(()));
    }

    #[test]
    fn an_expired_or_unknown_code_is_refused() {
        let mut o = offer(Discount::Flat { cents: 1_400 }, None);
        o.ends_at = Some(now() - Duration::hours(1));
        assert_eq!(eligibility(Some(&o), now(), tuesday(), &WINDOW, &AccountHistory::default()), Err(CodeRefusal::NotLive));
        assert_eq!(eligibility(None, now(), tuesday(), &WINDOW, &AccountHistory::default()), Err(CodeRefusal::Unknown));
    }

    #[test]
    fn codes_match_whatever_case_they_are_typed_in() {
        assert_eq!(normalize_code("  move20 "), "MOVE20");
    }
}
