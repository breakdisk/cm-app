//! Creating a venue and its tables.
//!
//! Pure domain arithmetic over the entities — no database, no broker — same as
//! `leg_transitions.rs`.
//!
//! The point of these is that creation is the ONLY place these values are
//! checked. `is_open_at`, `orderable_now` and the scan endpoint all trust the
//! stored row, and every scan refusal is a deliberately indistinguishable 404 —
//! so a window that can never match does not surface as an error anywhere. It
//! surfaces as a venue whose printed codes silently never work.

use chrono::{TimeZone, Utc};
use logisticos_omnideliv::domain::entities::{
    orderable_now, NotOrderable, OpeningWindow, Table, TableStatus, Venue, VenueInvalid,
    VenueKind, VenueStatus,
};
use uuid::Uuid;

fn win(dow: u32, open_minute: u32, close_minute: u32) -> OpeningWindow {
    OpeningWindow { dow, open_minute, close_minute }
}

fn all_week() -> Vec<OpeningWindow> {
    (1..=7).map(|d| win(d, 0, 1440)).collect()
}

#[test]
fn a_venue_needs_a_name() {
    let now = Utc::now();
    assert_eq!(
        Venue::new(Uuid::new_v4(), "   ", VenueKind::Standalone, all_week(), 480, now).unwrap_err(),
        VenueInvalid::NameEmpty
    );
}

#[test]
fn a_venue_is_active_from_creation_and_keeps_its_offset() {
    let now = Utc::now();
    let v = Venue::new(Uuid::new_v4(), "  Kanto Freestyle  ", VenueKind::Standalone, all_week(), 480, now)
        .expect("valid");
    // Trimmed, because a trailing space in a venue name is never intentional
    // and shows up on the print sheet.
    assert_eq!(v.name, "Kanto Freestyle");
    assert_eq!(v.status, VenueStatus::Active);
    assert_eq!(v.utc_offset_minutes, 480);
}

#[test]
fn an_offset_from_no_timezone_on_earth_is_refused() {
    let now = Utc::now();
    // +15:00 exists nowhere. Accepting it would put the venue's whole schedule
    // an hour off with nothing to see.
    assert_eq!(
        Venue::new(Uuid::new_v4(), "Nowhere", VenueKind::Standalone, all_week(), 900, now).unwrap_err(),
        VenueInvalid::OffsetOutOfRange(900)
    );
    // The real extremes must still be accepted.
    for offset in [-720, 480, 240, 840] {
        assert!(
            Venue::new(Uuid::new_v4(), "Somewhere", VenueKind::Standalone, all_week(), offset, now)
                .is_ok(),
            "offset {offset} should be valid"
        );
    }
}

#[test]
fn a_window_that_can_never_match_is_refused_at_creation() {
    let now = Utc::now();

    // Closes before it opens: `covers` can never fire, so every scan at this
    // venue would 404 forever with no error anywhere to explain it.
    assert_eq!(
        Venue::new(Uuid::new_v4(), "Inverted", VenueKind::Standalone, vec![win(1, 1080, 540)], 480, now).unwrap_err(),
        VenueInvalid::WindowInverted { open: 1080, close: 540 }
    );

    // Day 0 and day 8 do not exist — dow is 1 = Monday .. 7 = Sunday.
    assert_eq!(
        Venue::new(Uuid::new_v4(), "Day zero", VenueKind::Standalone, vec![win(0, 540, 1080)], 480, now).unwrap_err(),
        VenueInvalid::DayOutOfRange(0)
    );
    assert_eq!(
        Venue::new(Uuid::new_v4(), "Day eight", VenueKind::Standalone, vec![win(8, 540, 1080)], 480, now).unwrap_err(),
        VenueInvalid::DayOutOfRange(8)
    );

    // An open past local midnight can never be reached either.
    assert_eq!(
        Venue::new(Uuid::new_v4(), "Late open", VenueKind::Standalone, vec![win(1, 1500, 1600)], 480, now).unwrap_err(),
        VenueInvalid::WindowOutOfRange { open: 1500, close: 1600 }
    );
}

#[test]
fn a_window_running_past_midnight_is_still_valid() {
    let now = Utc::now();
    // 18:00 -> 01:00 is expressed as close_minute 1500, deliberately, rather
    // than as a second window on the following day. Rejecting it would make
    // every late-night kitchen unrepresentable.
    let v = Venue::new(
        Uuid::new_v4(),
        "Late kitchen",
        VenueKind::Standalone,
        vec![win(1, 1080, 1500)],
        480,
        now,
    )
    .expect("a window past midnight is valid");
    assert_eq!(v.hours.len(), 1);
}

#[test]
fn a_venue_with_no_hours_is_created_but_says_it_will_not_scan() {
    let now = Utc::now();
    // Allowed: setting a place up before it opens is normal. But it is exactly
    // the state where every printed code refuses every scan with the
    // indistinguishable 404, so the operator has to be told on the way out.
    let v = Venue::new(Uuid::new_v4(), "Not yet open", VenueKind::Standalone, vec![], 480, now)
        .expect("an empty schedule is allowed");
    assert_eq!(v.orderable_now_hint(now), Some(NotOrderable::OutsideOpeningHours));
    assert!(!v.is_open_at(now));
}

#[test]
fn the_hint_reports_a_paused_venue_before_its_hours() {
    // 12:00 UTC on a Monday = 20:00 in a +480 venue.
    let now = Utc.with_ymd_and_hms(2026, 8, 31, 12, 0, 0).unwrap();
    let mut v = Venue::new(Uuid::new_v4(), "Open all week", VenueKind::Standalone, all_week(), 480, now)
        .expect("valid");
    assert_eq!(v.orderable_now_hint(now), None, "open venue, inside hours");

    v.status = VenueStatus::Paused;
    assert_eq!(v.orderable_now_hint(now), Some(NotOrderable::VenueNotActive));
}

#[test]
fn a_table_is_born_open_unprinted_and_with_its_own_secret() {
    let now = Utc::now();
    let venue_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let a = Table::new(venue_id, tenant_id, " 12 ", now).expect("valid");
    let b = Table::new(venue_id, tenant_id, "13", now).expect("valid");

    assert_eq!(a.label, "12");
    assert!(a.printed_at.is_none(), "a new code is not on paper yet");
    assert_eq!(a.token.len(), 32, "uuid v4, hyphens stripped");
    assert_ne!(a.token, b.token, "two tables must never share a code");
}

#[test]
fn a_table_needs_a_label() {
    let now = Utc::now();
    assert_eq!(
        Table::new(Uuid::new_v4(), Uuid::new_v4(), "  ", now).unwrap_err(),
        VenueInvalid::LabelEmpty
    );
    let long = "x".repeat(41);
    assert_eq!(
        Table::new(Uuid::new_v4(), Uuid::new_v4(), &long, now).unwrap_err(),
        VenueInvalid::LabelTooLong
    );
}


// ---------------------------------------------------------------------------
// Editing a venue, and the kill switch.
//
// `orderable_now` refuses every scan while a venue is not Active, so
// `VenueStatus` is the stop button for the whole QR surface at that venue --
// and until now nothing on the platform could press it.
// ---------------------------------------------------------------------------

#[test]
fn pausing_a_venue_stops_every_scan_at_it() {
    // Monday 12:00 UTC = 20:00 in a +480 venue, inside all-week hours.
    let now = Utc.with_ymd_and_hms(2026, 8, 31, 12, 0, 0).unwrap();
    let mut v = Venue::new(Uuid::new_v4(), "Open", VenueKind::Standalone, all_week(), 480, now)
        .expect("valid");
    let t = Table::new(v.id, v.tenant_id, "1", now).expect("valid");

    assert!(orderable_now(&v, &t, now).is_ok(), "open venue takes orders");

    v.apply(None, None, None, Some(VenueStatus::Paused), now).expect("pause is valid");
    assert_eq!(orderable_now(&v, &t, now), Err(NotOrderable::VenueNotActive));

    // And back again -- pausing must be reversible, or it is not a stop button.
    v.apply(None, None, None, Some(VenueStatus::Active), now).expect("resume is valid");
    assert!(orderable_now(&v, &t, now).is_ok());
}

#[test]
fn closing_one_table_leaves_the_rest_of_the_venue_trading() {
    let now = Utc.with_ymd_and_hms(2026, 8, 31, 12, 0, 0).unwrap();
    let v = Venue::new(Uuid::new_v4(), "Open", VenueKind::Standalone, all_week(), 480, now)
        .expect("valid");
    let open = Table::new(v.id, v.tenant_id, "1", now).expect("valid");
    let mut shut = Table::new(v.id, v.tenant_id, "2", now).expect("valid");
    shut.status = TableStatus::Closed;

    assert_eq!(orderable_now(&v, &shut, now), Err(NotOrderable::TableClosed));
    assert!(orderable_now(&v, &open, now).is_ok(), "the other tables keep trading");
}

#[test]
fn an_update_validates_by_the_same_rules_as_creation() {
    let now = Utc::now();
    let mut v = Venue::new(Uuid::new_v4(), "Fine", VenueKind::Standalone, all_week(), 480, now)
        .expect("valid");

    // Every rule that blocks creation must block an edit, or a venue becomes
    // editable into a state it could never have been created in.
    assert_eq!(v.apply(Some("  "), None, None, None, now).unwrap_err(), VenueInvalid::NameEmpty);
    assert_eq!(
        v.apply(None, None, Some(900), None, now).unwrap_err(),
        VenueInvalid::OffsetOutOfRange(900)
    );
    assert_eq!(
        v.apply(None, Some(vec![win(1, 1080, 540)]), None, None, now).unwrap_err(),
        VenueInvalid::WindowInverted { open: 1080, close: 540 }
    );
    assert_eq!(
        v.apply(None, Some(vec![win(9, 540, 1080)]), None, None, now).unwrap_err(),
        VenueInvalid::DayOutOfRange(9)
    );
}

#[test]
fn a_rejected_update_changes_nothing_at_all() {
    let now = Utc::now();
    let mut v = Venue::new(Uuid::new_v4(), "Original", VenueKind::Standalone, all_week(), 480, now)
        .expect("valid");

    // A valid name alongside an invalid offset. Everything is checked before
    // anything is assigned, so this must leave the venue untouched -- a
    // half-applied edit is how a venue ends up renamed but still broken, with
    // the operator believing the whole change failed.
    let err = v.apply(Some("Renamed"), None, Some(-9999), None, now).unwrap_err();
    assert_eq!(err, VenueInvalid::OffsetOutOfRange(-9999));
    assert_eq!(v.name, "Original", "the name must not have been applied");
    assert_eq!(v.utc_offset_minutes, 480);
    assert_eq!(v.status, VenueStatus::Active);
}

#[test]
fn an_update_touches_only_the_fields_it_is_given() {
    let now = Utc::now();
    let mut v = Venue::new(Uuid::new_v4(), "Before", VenueKind::Standalone, all_week(), 480, now)
        .expect("valid");

    v.apply(Some(" After "), None, None, None, now).expect("valid");
    assert_eq!(v.name, "After", "trimmed, like creation");
    assert_eq!(v.hours.len(), 7, "hours untouched");
    assert_eq!(v.utc_offset_minutes, 480, "offset untouched");
    assert_eq!(v.status, VenueStatus::Active, "status untouched");

    v.apply(None, Some(vec![win(1, 540, 1080)]), None, None, now).expect("valid");
    assert_eq!(v.hours.len(), 1);
    assert_eq!(v.name, "After", "name survives an hours-only edit");
}
