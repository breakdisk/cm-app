//! The redemption window: one promo code per calendar month, on weekdays
//! between the 11th and the 24th.
//!
//! Weekends are out because van supply is tightest on Saturday and Sunday: a
//! discount there buys demand that cannot be served. The days are the tenant's
//! local days, so the rule works on a local date, never on UTC.

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc, Weekday};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowRule {
    /// First eligible day of the month, inclusive.
    pub from_day: u32,
    /// Last eligible day of the month, inclusive.
    pub to_day: u32,
}

impl WindowRule {
    /// Config out of range is pulled back into range rather than trusted.
    pub fn new(from_day: u32, to_day: u32) -> Self {
        let from_day = from_day.clamp(1, 31);
        let to_day = to_day.clamp(from_day, 31);
        Self { from_day, to_day }
    }

    pub fn check(&self, day: NaiveDate) -> Result<(), WindowRefusal> {
        if matches!(day.weekday(), Weekday::Sat | Weekday::Sun) {
            return Err(WindowRefusal::Weekend);
        }
        if day.day() < self.from_day || day.day() > self.to_day {
            return Err(WindowRefusal::OutsideDays { from_day: self.from_day, to_day: self.to_day });
        }
        Ok(())
    }

    /// Every day of `year`-`month` and whether a code can be redeemed on it —
    /// what the app draws as the month grid, so it never works the rule out.
    pub fn month(&self, year: i32, month: u32) -> Vec<WindowDay> {
        let Some(first) = NaiveDate::from_ymd_opt(year, month, 1) else {
            return Vec::new();
        };
        let mut days = Vec::with_capacity(31);
        let mut d = first;
        while d.month() == month {
            days.push(WindowDay {
                day: d.day(),
                weekday: d.weekday().num_days_from_monday(),
                eligible: self.check(d).is_ok(),
            });
            d += Duration::days(1);
        }
        days
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct WindowDay {
    pub day: u32,
    /// 0 = Monday.
    pub weekday: u32,
    pub eligible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowRefusal {
    Weekend,
    OutsideDays { from_day: u32, to_day: u32 },
}

impl WindowRefusal {
    pub fn message(&self) -> String {
        match self {
            Self::Weekend => "Codes can't be used at weekends — van supply is tightest then.".into(),
            Self::OutsideDays { from_day, to_day } => {
                format!("Codes can be used on weekdays from the {} to the {} of the month.", ordinal(*from_day), ordinal(*to_day))
            }
        }
    }
}

/// The tenant's local date. A fixed offset, because the platform has no tenant
/// time zone yet; a deployment serves one region.
pub fn local_date(now: DateTime<Utc>, utc_offset_minutes: i32) -> NaiveDate {
    (now + Duration::minutes(i64::from(utc_offset_minutes))).date_naive()
}

/// "2026-09" — the calendar month a redemption counts against.
pub fn month_key(day: NaiveDate) -> String {
    format!("{:04}-{:02}", day.year(), day.month())
}

fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    const RULE: WindowRule = WindowRule { from_day: 11, to_day: 24 };

    #[test]
    fn a_weekday_inside_the_days_is_open() {
        // Friday 2026-09-11.
        assert_eq!(RULE.check(d(2026, 9, 11)), Ok(()));
        // Thursday 2026-09-24.
        assert_eq!(RULE.check(d(2026, 9, 24)), Ok(()));
    }

    #[test]
    fn weekends_are_closed_even_inside_the_days() {
        // Saturday 2026-09-12.
        assert_eq!(RULE.check(d(2026, 9, 12)), Err(WindowRefusal::Weekend));
    }

    #[test]
    fn days_outside_the_range_are_closed() {
        // Thursday 2026-09-10 and Friday 2026-09-25.
        assert!(matches!(RULE.check(d(2026, 9, 10)), Err(WindowRefusal::OutsideDays { .. })));
        assert!(matches!(RULE.check(d(2026, 9, 25)), Err(WindowRefusal::OutsideDays { .. })));
    }

    #[test]
    fn the_month_grid_lights_weekdays_in_range() {
        let grid = RULE.month(2026, 9);
        assert_eq!(grid.len(), 30);
        let lit: Vec<u32> = grid.iter().filter(|x| x.eligible).map(|x| x.day).collect();
        assert_eq!(lit, vec![11, 14, 15, 16, 17, 18, 21, 22, 23, 24]);
        // 2026-09-01 is a Tuesday.
        assert_eq!(grid[0].weekday, 1);
    }

    /// Manila is UTC+8: 17:00 UTC on the 10th is already the 11th there.
    #[test]
    fn the_local_day_is_the_tenants() {
        let now = DateTime::parse_from_rfc3339("2026-09-10T17:00:00Z").unwrap().with_timezone(&Utc);
        assert_eq!(local_date(now, 480), d(2026, 9, 11));
        assert_eq!(local_date(now, 0), d(2026, 9, 10));
    }

    #[test]
    fn config_out_of_range_is_pulled_back() {
        assert_eq!(WindowRule::new(0, 99), WindowRule { from_day: 1, to_day: 31 });
        assert_eq!(WindowRule::new(20, 5), WindowRule { from_day: 20, to_day: 20 });
    }

    #[test]
    fn the_refusal_reads_as_the_design_says() {
        assert_eq!(
            WindowRefusal::OutsideDays { from_day: 11, to_day: 24 }.message(),
            "Codes can be used on weekdays from the 11th to the 24th of the month."
        );
        assert_eq!(month_key(d(2026, 9, 11)), "2026-09");
    }
}
