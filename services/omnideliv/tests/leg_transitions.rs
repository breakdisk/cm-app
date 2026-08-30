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

use logisticos_omnideliv::domain::entities::OrderStatus;

#[test]
fn an_unrecognised_wire_string_is_rejected_rather_than_defaulted() {
    // `order_repo::leg_status` depends entirely on this: a row written by a
    // newer deploy must fail loudly, not decode as Pending and re-offer work
    // that is already underway.
    assert_eq!(LegStatus::from_wire("bogus"), None);
    assert_eq!(LegStatus::from_wire(""), None);
    assert_eq!(LegStatus::from_wire("PENDING"), None, "parsing is case-sensitive");
}

#[test]
fn a_leg_still_being_prepared_blocks_collection() {
    // The bug this test exists for: under the old four states, "not pending"
    // meant "resolved". It no longer does. A leg sitting at Ready is accepted
    // and cooked and still on the counter — the order must not advance.
    for s in [LegStatus::Pending, LegStatus::Accepted, LegStatus::Preparing, LegStatus::Ready] {
        assert!(s.blocks_collection(), "{s:?} must block the order from advancing");
    }
    for s in [LegStatus::PickedUp, LegStatus::Rejected, LegStatus::Failed, LegStatus::Served] {
        assert!(!s.blocks_collection(), "{s:?} is resolved and must not block");
    }
}

#[test]
fn an_order_with_a_leg_on_the_counter_does_not_advance_to_delivering() {
    let mut o = order_with(&[LegStatus::PickedUp, LegStatus::Ready]);
    o.status = OrderStatus::Collecting;
    assert!(
        o.all_legs_collected().is_err(),
        "one leg collected and one still ready must not advance the order",
    );
}

#[test]
fn an_order_whose_legs_are_all_resolved_advances() {
    let mut o = order_with(&[LegStatus::PickedUp, LegStatus::Rejected]);
    o.status = OrderStatus::Collecting;
    assert!(o.all_legs_collected().is_ok(), "a rejected leg is resolved, not outstanding");
    assert_eq!(o.status, OrderStatus::Delivering);
}

#[test]
fn every_status_is_covered_by_the_transition_graph_lookup() {
    // `LegStatus::ALL` is what the repository derives its SQL predecessor list
    // from. A variant missing from it would silently become untransitionable.
    assert_eq!(LegStatus::ALL.len(), 9);
    for s in LegStatus::ALL {
        assert_eq!(LegStatus::from_wire(s.as_str()), Some(s));
    }
}

#[test]
fn a_failed_leg_is_neither_accepted_nor_rejected() {
    // `Failed` means the leg passed acceptance and broke afterwards, so it is
    // not a vendor refusal — conflating the two would send an ops team down
    // the wrong remediation path. The consequence, which is easy to misread
    // on a dashboard, is that `accepted + rejected` does NOT equal the leg
    // count when any leg failed.
    let o = order_with(&[LegStatus::Accepted, LegStatus::Failed]);
    assert_eq!(
        o.acceptance_state(),
        AcceptanceState::Resolved { accepted: 1, rejected: 0, accepted_subtotal_cents: 1_000 },
    );
}

#[test]
fn an_order_with_no_legs_is_resolved_and_owed_nothing() {
    let o = order_with(&[]);
    assert_eq!(
        o.acceptance_state(),
        AcceptanceState::Resolved { accepted: 0, rejected: 0, accepted_subtotal_cents: 0 },
    );
}
