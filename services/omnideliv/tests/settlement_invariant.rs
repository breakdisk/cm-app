//! The settlement balance invariant, swept across the input space.
//!
//! Hand-written examples cover the shapes someone thought of. This sweeps the
//! rounding edges — subtotals whose commission lands on a fraction of a cent,
//! which is exactly where an integer-maths bug hides.
//!
//! Needs no database and no network: it is pure arithmetic over the domain
//! entities, so unlike the other integration tests here it genuinely runs
//! everywhere, including on a dev machine with no Postgres.

use logisticos_omnideliv::domain::entities::{Order, VendorLeg};
use uuid::Uuid;

fn check(subtotals: &[i64], bps: &[i32], fee: i64, tip: i64, trip: i64) {
    let legs: Vec<VendorLeg> = subtotals
        .iter()
        .zip(bps.iter())
        .map(|(s, b)| VendorLeg::settle(Uuid::new_v4(), Uuid::new_v4(), *s, *b))
        .collect();

    // Per-leg invariant first — a leg that does not split exactly makes the
    // order-level failure much harder to localise.
    for l in &legs {
        assert_eq!(
            l.commission_cents + l.payout_cents,
            l.goods_subtotal_cents,
            "leg failed to split exactly: subtotal={} bps={}",
            l.goods_subtotal_cents, l.commission_bps
        );
        assert!(l.commission_cents >= 0 && l.payout_cents >= 0, "no negative money");
    }

    let o = Order::place(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(),
                         legs, fee, tip, trip, 14.5995, 120.9842);
    let s = o.settlement();

    assert_eq!(
        o.grand_total_cents,
        s.vendor_payouts_cents + s.commissions_cents + s.courier_earnings_cents + s.partner_margin_cents,
        "settlement did not balance: subtotals={subtotals:?} bps={bps:?} fee={fee} tip={tip} trip={trip}"
    );
}

#[test]
fn settlement_balances_across_the_rounding_edges() {
    // Subtotals chosen to land commission on a fraction of a cent at common
    // rates: primes, near-primes and values just off a round number.
    let subtotals = [1_i64, 7, 99, 101, 999, 1_001, 3_333, 9_999, 12_345, 99_999, 1_000_003];
    let rates     = [0_i32, 1, 250, 999, 1_500, 3_333, 5_000, 9_999, 10_000];
    let fees      = [0_i64, 1, 4_900, 7_900];
    let tips      = [0_i64, 1, 4_000];

    let mut cases = 0;
    for &s in &subtotals {
        for &b in &rates {
            for &fee in &fees {
                for &tip in &tips {
                    // Courier trip never exceeds the fee here. A trip that costs
                    // more than the fee is a real scenario — see
                    // `an_underwater_delivery_fee_still_balances` in the entity
                    // tests — but it is a pricing question, and keeping this
                    // sweep to the priced-correctly space means a failure here
                    // is unambiguously an arithmetic bug.
                    for &trip in &[0, fee / 2, fee] {
                        check(&[s], &[b], fee, tip, trip);
                        cases += 1;
                    }
                }
            }
        }
    }

    // Multi-vendor: the case the flat fee exists for.
    for &a in &subtotals {
        for &b in &subtotals {
            check(&[a, b], &[1_500, 1_200], 7_900, 4_000, 5_800);
            check(&[a, b, a], &[1_500, 1_200, 999], 7_900, 0, 5_800);
            cases += 2;
        }
    }

    assert!(cases > 3_000, "sweep should cover thousands of cases, covered {cases}");
}

/// A 100% commission rate pays the vendor nothing and the Partner everything,
/// which is a valid configuration (a Partner-owned dark store) and must still
/// balance. Zero commission is the mirror case.
#[test]
fn the_commission_extremes_still_balance() {
    for bps in [0_i32, 10_000] {
        check(&[50_000], &[bps], 7_900, 2_000, 5_800);
    }

    let all_commission = VendorLeg::settle(Uuid::new_v4(), Uuid::new_v4(), 50_000, 10_000);
    assert_eq!(all_commission.payout_cents, 0);
    assert_eq!(all_commission.commission_cents, 50_000);

    let none = VendorLeg::settle(Uuid::new_v4(), Uuid::new_v4(), 50_000, 0);
    assert_eq!(none.payout_cents, 50_000);
    assert_eq!(none.commission_cents, 0);
}

/// An empty basket is not orderable, but the arithmetic must not misbehave if
/// one reaches settlement — a panic or a nonsense total here would be a far
/// worse failure than the empty order itself.
#[test]
fn an_order_with_no_legs_is_arithmetically_sound() {
    let o = Order::place(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(),
                         vec![], 4_900, 0, 3_500, 14.5995, 120.9842);
    let s = o.settlement();

    assert_eq!(o.goods_total_cents, 0);
    assert_eq!(o.grand_total_cents, 4_900);
    assert_eq!(
        o.grand_total_cents,
        s.vendor_payouts_cents + s.commissions_cents + s.courier_earnings_cents + s.partner_margin_cents,
    );
}
