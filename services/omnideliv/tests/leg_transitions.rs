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

use logisticos_omnideliv::domain::entities::{AcceptanceState, Order, VendorLeg};
use uuid::Uuid;

fn order_with(statuses: &[LegStatus]) -> Order {
    let legs: Vec<VendorLeg> = statuses
        .iter()
        .map(|s| {
            let mut l = VendorLeg::settle(Uuid::new_v4(), Uuid::new_v4(), 1_000, 1_500);
            l.status = *s;
            l
        })
        .collect();
    Order::place(
        Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(),
        legs, 0, 0, 0, 14.5995, 120.9842,
    )
}

#[test]
fn an_order_with_a_pending_leg_is_still_waiting() {
    let o = order_with(&[LegStatus::Accepted, LegStatus::Pending]);
    assert_eq!(o.acceptance_state(), AcceptanceState::Awaiting { outstanding: 1 });
}

#[test]
fn the_barrier_lifts_only_when_every_leg_has_answered() {
    let o = order_with(&[LegStatus::Accepted, LegStatus::Rejected, LegStatus::Preparing]);
    assert_eq!(
        o.acceptance_state(),
        AcceptanceState::Resolved { accepted: 2, rejected: 1, accepted_subtotal_cents: 2_000 },
    );
}

#[test]
fn an_order_every_stall_refused_is_resolved_with_nothing_accepted() {
    let o = order_with(&[LegStatus::Rejected, LegStatus::Rejected]);
    assert_eq!(
        o.acceptance_state(),
        AcceptanceState::Resolved { accepted: 0, rejected: 2, accepted_subtotal_cents: 0 },
    );
}

#[test]
fn the_accepted_subtotal_excludes_refused_legs() {
    // The number the acceptance barrier captures. A rejected leg's subtotal
    // must never reach it.
    let o = order_with(&[LegStatus::Ready, LegStatus::Rejected]);
    match o.acceptance_state() {
        AcceptanceState::Resolved { accepted_subtotal_cents, .. } => {
            assert_eq!(accepted_subtotal_cents, 1_000, "only the surviving leg counts");
        }
        other => panic!("expected Resolved, got {other:?}"),
    }
}
