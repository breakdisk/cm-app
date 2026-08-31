//! A place with tables, and the codes printed on them.
//!
//! A venue is deliberately not a vendor. A vendor is a business that sells; a
//! venue is a place with tables. A mall foodcourt is one venue with many
//! vendors, a standalone restaurant is one venue with one, and collapsing the
//! two would make the foodcourt unrepresentable.
//!
//! ## What the printed code is worth
//!
//! Nothing, on its own. It is on adhesive vinyl in a public room and is
//! photographable from three metres by anyone walking past. Scanning it mints a
//! session; holding it grants nothing. The controls that actually bound the
//! threat are here in `orderable_now` — a table that is closed, or a venue
//! outside its hours, refuses every scan however valid the token.

use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VenueKind {
    Standalone,
    Foodcourt,
}

impl VenueKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            VenueKind::Standalone => "standalone",
            VenueKind::Foodcourt => "foodcourt",
        }
    }

    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "standalone" => Some(VenueKind::Standalone),
            "foodcourt" => Some(VenueKind::Foodcourt),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VenueStatus {
    Active,
    Paused,
    Closed,
}

impl VenueStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            VenueStatus::Active => "active",
            VenueStatus::Paused => "paused",
            VenueStatus::Closed => "closed",
        }
    }

    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "active" => Some(VenueStatus::Active),
            "paused" => Some(VenueStatus::Paused),
            "closed" => Some(VenueStatus::Closed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TableStatus {
    Open,
    Closed,
}

impl TableStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TableStatus::Open => "open",
            TableStatus::Closed => "closed",
        }
    }

    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "open" => Some(TableStatus::Open),
            "closed" => Some(TableStatus::Closed),
            _ => None,
        }
    }
}

/// One opening window, in the venue's own local time.
///
/// `dow` is 1 = Monday .. 7 = Sunday, matching ISO-8601 and `chrono`'s
/// `number_from_monday`, so nothing has to remember an off-by-one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpeningWindow {
    pub dow: u32,
    /// Minutes past local midnight. `540` is 09:00.
    pub open_minute: u32,
    /// Minutes past local midnight. May exceed 1440 for a window that runs past
    /// midnight — a kitchen open until 01:00 is `close_minute: 1500`, not a
    /// second window on the following day, because splitting it would make
    /// "are we open" a two-row question.
    pub close_minute: u32,
}

impl OpeningWindow {
    /// Whether `local_dow` / `local_minute` fall inside this window.
    ///
    /// A window that runs past midnight also matches the early hours of the
    /// FOLLOWING day, which is the whole reason `close_minute` may exceed 1440.
    fn covers(&self, local_dow: u32, local_minute: u32) -> bool {
        if self.dow == local_dow && local_minute >= self.open_minute && local_minute < self.close_minute
        {
            return true;
        }
        // Spill-over into the next day: 01:00 Tuesday is covered by Monday's
        // 18:00–01:00 window, read as minute 1500.
        if self.close_minute > 1440 {
            let prev_dow = if local_dow == 1 { 7 } else { local_dow - 1 };
            if self.dow == prev_dow && local_minute + 1440 < self.close_minute {
                return true;
            }
        }
        false
    }
}

/// Why a scan was refused.
///
/// The HTTP surface deliberately collapses every one of these into one
/// indistinguishable response — a probing scanner must not be able to tell a
/// closed table from an unknown token. This exists so operators and logs can
/// still tell them apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotOrderable {
    VenueNotActive,
    TableClosed,
    OutsideOpeningHours,
}

#[derive(Debug, Clone)]
pub struct Venue {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub kind: VenueKind,
    pub hours: Vec<OpeningWindow>,
    /// See the migration: a fixed offset, because both current markets are
    /// DST-free. A DST venue needs an IANA zone here first.
    pub utc_offset_minutes: i32,
    pub status: VenueStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Venue {
    /// Whether this venue is inside its opening hours at `now`.
    ///
    /// `now` is a parameter rather than an internal `Utc::now()` so the
    /// boundaries can be tested exactly — the same reason
    /// `leg_recovery::decide` takes one. A function that reads the clock can
    /// only be tested relative to the real clock, which cannot pin a boundary.
    ///
    /// **A venue with no hours declared is treated as closed, not as open.**
    /// An empty schedule is far more likely to be an unfinished onboarding than
    /// a deliberate always-open, and defaulting to open would let a half-set-up
    /// venue take orders at 4am.
    pub fn is_open_at(&self, now: DateTime<Utc>) -> bool {
        let local = now + Duration::minutes(self.utc_offset_minutes as i64);
        let dow = local.date_naive().weekday().number_from_monday();
        let minute = local.hour() * 60 + local.minute();
        self.hours.iter().any(|w| w.covers(dow, minute))
    }
}

#[derive(Debug, Clone)]
pub struct Table {
    pub id: Uuid,
    pub venue_id: Uuid,
    pub tenant_id: Uuid,
    pub label: String,
    /// The printed secret. Never logged, never returned to a scanner — only to
    /// the operator printing it.
    pub token: String,
    pub status: TableStatus,
    pub printed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// An open party at a table.
///
/// `id` doubles as the synthetic `user_id` on the minted token, which is what
/// lets `orders.customer_id` stay non-null for a diner with no account.
#[derive(Debug, Clone)]
pub struct TableSession {
    pub id: Uuid,
    pub table_id: Uuid,
    pub venue_id: Uuid,
    pub tenant_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

impl TableSession {
    /// Live means unended AND unexpired. Both matter: a party that never closes
    /// its tab has to age out on its own, or a table fills its session cap and
    /// stays full until someone notices.
    pub fn is_live(&self, now: DateTime<Utc>) -> bool {
        self.ended_at.is_none() && self.expires_at > now
    }
}

/// May a scan of this table mint a session right now?
///
/// Pure, so the rule is testable without a database and without a clock. This
/// is the control that actually bounds the printed code: ordering to a table at
/// 03:00 when the restaurant is shut must be impossible regardless of how valid
/// the token is.
pub fn orderable_now(
    venue: &Venue,
    table: &Table,
    now: DateTime<Utc>,
) -> Result<(), NotOrderable> {
    if venue.status != VenueStatus::Active {
        return Err(NotOrderable::VenueNotActive);
    }
    if table.status != TableStatus::Open {
        return Err(NotOrderable::TableClosed);
    }
    if !venue.is_open_at(now) {
        return Err(NotOrderable::OutsideOpeningHours);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-08-31 is a Monday. 10:00 UTC is 18:00 in a UTC+8 venue.
    fn monday_1000_utc() -> DateTime<Utc> {
        DateTime::from_timestamp(1_788_170_400, 0).unwrap()
    }

    fn venue(hours: Vec<OpeningWindow>) -> Venue {
        Venue {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            name: "Test".into(),
            kind: VenueKind::Standalone,
            hours,
            utc_offset_minutes: 480,
            status: VenueStatus::Active,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn table(status: TableStatus) -> Table {
        Table {
            id: Uuid::new_v4(),
            venue_id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            label: "A-14".into(),
            token: "tok".into(),
            status,
            printed_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn the_local_day_is_read_in_the_venues_own_offset_not_utc() {
        let now = monday_1000_utc();
        // 18:00 Monday local. A venue open Monday evening is open.
        let v = venue(vec![OpeningWindow { dow: 1, open_minute: 17 * 60, close_minute: 22 * 60 }]);
        assert!(v.is_open_at(now), "18:00 local Monday must be inside 17:00-22:00 Monday");

        // The same instant read as UTC would be 10:00, which this window
        // excludes — so a passing test here proves the offset is applied.
        let utc_morning =
            venue(vec![OpeningWindow { dow: 1, open_minute: 9 * 60, close_minute: 11 * 60 }]);
        assert!(!utc_morning.is_open_at(now), "must not match the UTC wall clock");
    }

    #[test]
    fn a_venue_with_no_hours_is_closed_rather_than_always_open() {
        // An empty schedule is an unfinished onboarding far more often than a
        // deliberate 24/7, and defaulting to open lets a half-set-up venue take
        // orders at 4am.
        assert!(!venue(vec![]).is_open_at(monday_1000_utc()));
    }

    #[test]
    fn a_window_running_past_midnight_covers_the_early_hours_of_the_next_day() {
        // Monday 18:00-01:00, expressed as close_minute 1500.
        let v = venue(vec![OpeningWindow { dow: 1, open_minute: 18 * 60, close_minute: 1500 }]);
        // Monday 18:00 local == 10:00 UTC.
        assert!(v.is_open_at(monday_1000_utc()), "the window's own evening");
        // Tuesday 00:30 local == Monday 16:30 UTC, 6.5h after our anchor.
        let tue_0030 = monday_1000_utc() + Duration::minutes(390);
        assert!(v.is_open_at(tue_0030), "00:30 Tuesday is still Monday's window");
        // Tuesday 01:30 local is past close.
        let tue_0130 = monday_1000_utc() + Duration::minutes(450);
        assert!(!v.is_open_at(tue_0130), "past the 01:00 close");
    }

    #[test]
    fn the_open_boundary_is_inclusive_and_the_close_boundary_is_not() {
        // A minute either side of a boundary is a table that takes an order it
        // cannot cook, or refuses one it could.
        let v = venue(vec![OpeningWindow { dow: 1, open_minute: 18 * 60, close_minute: 22 * 60 }]);
        assert!(v.is_open_at(monday_1000_utc()), "exactly 18:00 is open");
        let one_before = monday_1000_utc() - Duration::minutes(1);
        assert!(!v.is_open_at(one_before), "17:59 is not");
        let at_close = monday_1000_utc() + Duration::minutes(4 * 60);
        assert!(!v.is_open_at(at_close), "exactly 22:00 is closed");
        let before_close = at_close - Duration::minutes(1);
        assert!(v.is_open_at(before_close), "21:59 is open");
    }

    #[test]
    fn a_closed_table_refuses_even_inside_opening_hours() {
        let v = venue(vec![OpeningWindow { dow: 1, open_minute: 0, close_minute: 1440 }]);
        assert_eq!(
            orderable_now(&v, &table(TableStatus::Closed), monday_1000_utc()),
            Err(NotOrderable::TableClosed),
        );
    }

    #[test]
    fn a_paused_venue_refuses_even_with_an_open_table_inside_hours() {
        let mut v = venue(vec![OpeningWindow { dow: 1, open_minute: 0, close_minute: 1440 }]);
        v.status = VenueStatus::Paused;
        assert_eq!(
            orderable_now(&v, &table(TableStatus::Open), monday_1000_utc()),
            Err(NotOrderable::VenueNotActive),
        );
    }

    #[test]
    fn an_open_table_at_an_active_venue_inside_hours_is_orderable() {
        let v = venue(vec![OpeningWindow { dow: 1, open_minute: 0, close_minute: 1440 }]);
        assert_eq!(orderable_now(&v, &table(TableStatus::Open), monday_1000_utc()), Ok(()));
    }

    #[test]
    fn a_session_is_live_only_while_unended_and_unexpired() {
        let now = monday_1000_utc();
        let mut s = TableSession {
            id: Uuid::new_v4(),
            table_id: Uuid::new_v4(),
            venue_id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            created_at: now,
            expires_at: now + Duration::hours(2),
            ended_at: None,
        };
        assert!(s.is_live(now));

        // An abandoned party must age out on its own, or the table's session
        // cap stays full until a human notices.
        assert!(!s.is_live(now + Duration::hours(3)), "expired");

        s.ended_at = Some(now);
        assert!(!s.is_live(now), "ended");
    }
}
