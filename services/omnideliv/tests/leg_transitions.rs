//! The vendor-leg transition graph.
//!
//! Pure domain arithmetic over the entities — no database, no broker — so this
//! runs on a dev machine with no Postgres, same as `settlement_invariant.rs`.

use logisticos_omnideliv::domain::entities::LegStatus;

#[test]
fn a_pending_leg_can_be_accepted_or_rejected_and_nothing_else() {
    assert!(LegStatus::Pending.can_transition_to(LegStatus::Accepted));
    assert!(LegStatus::Pending.can_transition_to(LegStatus::Rejected));
    assert!(!LegStatus::Pending.can_transition_to(LegStatus::Ready));
    assert!(!LegStatus::Pending.can_transition_to(LegStatus::PickedUp));
    assert!(!LegStatus::Pending.can_transition_to(LegStatus::Settled));
}

#[test]
fn an_accepted_leg_may_skip_preparing() {
    // A florist wrapping one bouquet has no meaningful "preparing" step; a
    // kitchen does. Both are legal rather than forcing a fake transition.
    assert!(LegStatus::Accepted.can_transition_to(LegStatus::Preparing));
    assert!(LegStatus::Accepted.can_transition_to(LegStatus::Ready));
    assert!(LegStatus::Preparing.can_transition_to(LegStatus::Ready));
}

#[test]
fn a_ready_leg_leaves_by_courier_or_by_table() {
    assert!(LegStatus::Ready.can_transition_to(LegStatus::PickedUp));
    assert!(LegStatus::Ready.can_transition_to(LegStatus::Served));
}

#[test]
fn terminal_states_never_move_again() {
    for s in [LegStatus::Rejected, LegStatus::Failed, LegStatus::Settled] {
        assert!(s.is_terminal(), "{s:?} should be terminal");
        for next in [LegStatus::Accepted, LegStatus::Ready, LegStatus::Settled] {
            assert!(!s.can_transition_to(next), "{s:?} must not move to {next:?}");
        }
    }
}

#[test]
fn any_live_leg_can_be_failed_by_an_operator() {
    // Preserves the existing `mark_failed` behaviour, which has no single
    // legal predecessor — an operator can fail a leg at any live point.
    for s in [LegStatus::Pending, LegStatus::Accepted, LegStatus::Preparing, LegStatus::Ready] {
        assert!(s.can_transition_to(LegStatus::Failed), "{s:?} should be failable");
    }
}

#[test]
fn every_status_round_trips_through_its_wire_string() {
    for s in [
        LegStatus::Pending, LegStatus::Accepted, LegStatus::Preparing,
        LegStatus::Ready, LegStatus::PickedUp, LegStatus::Served,
        LegStatus::Rejected, LegStatus::Failed, LegStatus::Settled,
    ] {
        assert_eq!(LegStatus::from_wire(s.as_str()), Some(s), "round trip failed for {s:?}");
    }
}
