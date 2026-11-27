//! The delivery promise per service type: the label the customer is shown and
//! the deadline stored for it come from this one table, so they cannot drift.
//!
//! Business days are Monday to Friday. There is no holiday calendar: a public
//! holiday counts as a business day.

use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, TimeZone, Utc, Weekday};

/// What a service type promises.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServicePromise {
    /// Shown to the customer as the estimated delivery.
    pub label: &'static str,
    /// The deadline is the end of the Nth business day after the base day.
    /// 0 means the base day itself (same day).
    pub business_days: u32,
}

/// Keyed on `ServiceType::as_str()`. Anything not listed (standard, home_move)
/// gets the standard promise.
pub fn promise_for(service_type: &str) -> ServicePromise {
    match service_type {
        "same_day"      => ServicePromise { label: "Today",               business_days: 0 },
        "express"       => ServicePromise { label: "Next business day",   business_days: 1 },
        "international" => ServicePromise { label: "7–14 business days",  business_days: 14 },
        "balikbayan"    => ServicePromise { label: "21–45 business days", business_days: 45 },
        _               => ServicePromise { label: "2–3 business days",   business_days: 3 },
    }
}

fn is_business_day(d: NaiveDate) -> bool {
    !matches!(d.weekday(), Weekday::Sat | Weekday::Sun)
}

/// The deadline for a shipment: 23:59:59 (in `offset`) of the Nth business day
/// strictly after the local date of the base, where the base is
/// `max(created_at, scheduled_pickup_at)`. A weekend base day counts forward
/// from its own date. `None` only if the date arithmetic leaves the calendar.
pub fn sla_due_at(
    service_type: &str,
    created_at: DateTime<Utc>,
    scheduled_pickup_at: Option<DateTime<Utc>>,
    offset: FixedOffset,
) -> Option<DateTime<Utc>> {
    let base = scheduled_pickup_at.map_or(created_at, |p| p.max(created_at));
    let mut day = base.with_timezone(&offset).date_naive();
    let mut remaining = promise_for(service_type).business_days;
    while remaining > 0 {
        day = day.checked_add_signed(Duration::days(1))?;
        if is_business_day(day) {
            remaining -= 1;
        }
    }
    let local = day.and_hms_opt(23, 59, 59)?;
    offset.from_local_datetime(&local).single().map(|d| d.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pht() -> FixedOffset {
        FixedOffset::east_opt(8 * 3600).unwrap()
    }
    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn friday_evening_express_is_monday_end_of_day() {
        // Fri 2026-11-27 18:00 +08
        let due = sla_due_at("express", at("2026-11-27T18:00:00+08:00"), None, pht()).unwrap();
        assert_eq!(due, at("2026-11-30T23:59:59+08:00"));
    }

    #[test]
    fn saturday_standard_is_wednesday_end_of_day() {
        // Sat 2026-11-28 10:00 +08 -> Mon, Tue, Wed
        let due = sla_due_at("standard", at("2026-11-28T10:00:00+08:00"), None, pht()).unwrap();
        assert_eq!(due, at("2026-12-02T23:59:59+08:00"));
    }

    #[test]
    fn same_day_is_the_same_local_day() {
        let due = sla_due_at("same_day", at("2026-11-25T09:00:00+08:00"), None, pht()).unwrap();
        assert_eq!(due, at("2026-11-25T23:59:59+08:00"));
    }

    #[test]
    fn base_day_is_the_local_day_not_the_utc_day() {
        // 2026-11-25 20:00 UTC is already Thu 2026-11-26 04:00 +08.
        let due = sla_due_at("same_day", at("2026-11-25T20:00:00Z"), None, pht()).unwrap();
        assert_eq!(due, at("2026-11-26T23:59:59+08:00"));
    }

    #[test]
    fn a_later_scheduled_pickup_moves_the_base() {
        // Created Wed, pickup booked for the following Monday; standard = Thu.
        let due = sla_due_at(
            "standard",
            at("2026-11-25T10:00:00+08:00"),
            Some(at("2026-11-30T09:00:00+08:00")),
            pht(),
        )
        .unwrap();
        assert_eq!(due, at("2026-12-03T23:59:59+08:00"));
    }

    #[test]
    fn an_earlier_scheduled_pickup_does_not_move_the_base() {
        let created = at("2026-11-25T10:00:00+08:00");
        let with = sla_due_at("express", created, Some(at("2026-11-20T09:00:00+08:00")), pht());
        assert_eq!(with, sla_due_at("express", created, None, pht()));
    }

    #[test]
    fn long_promises_skip_weekends() {
        // Mon 2026-11-23 + 14 business days = Fri 2026-12-11.
        let due = sla_due_at("international", at("2026-11-23T10:00:00+08:00"), None, pht()).unwrap();
        assert_eq!(due, at("2026-12-11T23:59:59+08:00"));
    }

    #[test]
    fn labels_are_unchanged() {
        assert_eq!(promise_for("express").label, "Next business day");
        assert_eq!(promise_for("same_day").label, "Today");
        assert_eq!(promise_for("balikbayan").label, "21–45 business days");
        assert_eq!(promise_for("international").label, "7–14 business days");
        assert_eq!(promise_for("standard").label, "2–3 business days");
        assert_eq!(promise_for("home_move").label, "2–3 business days");
    }
}
