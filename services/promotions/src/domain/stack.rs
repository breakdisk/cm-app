//! How discounts stack on one move, in the design's order.
//!
//! 1. One code per move. A promo code and the corporate rate compete; the
//!    larger wins and they never add.
//! 2. The tier discount comes off on top, capped by the tier's own cap.
//! 3. Stored credit comes off last; whatever does not fit rolls over.
//! 4. Every step is clipped by the ceiling, `min(pct of gross, flat)`, and when
//!    it binds the quote says so rather than truncating in silence.
//!
//! Discounts come off what the customer is billed. Nothing here reads or moves
//! what the driver is paid: settlement never sees promotion state.

use serde::Serialize;

/// What the customer would pay before any discount, in minor units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gross {
    pub carriage_cents: i64,
    pub accessorial_cents: i64,
}

impl Gross {
    pub fn total(&self) -> i64 {
        self.carriage_cents.max(0) + self.accessorial_cents.max(0)
    }
}

/// `min(pct% of gross, flat_cents)` — the most any move can be discounted.
/// The flat cap is in the move's own currency, never converted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ceiling {
    pub pct: i64,
    pub flat_cents: i64,
}

impl Ceiling {
    pub fn room(&self, gross: &Gross) -> i64 {
        let by_pct = gross.total() * self.pct.clamp(0, 100) / 100;
        by_pct.min(self.flat_cents.max(0)).max(0)
    }
}

/// A discount that wants to apply, before the ceiling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub label: String,
    pub cents: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Candidates {
    pub code: Option<Candidate>,
    pub corporate: Option<Candidate>,
    /// The tier's discount and its per-move cap.
    pub tier: Option<(Candidate, i64)>,
    pub credit_cents: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LineKind {
    Code,
    Corporate,
    Tier,
    Credit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiscountLine {
    pub kind: LineKind,
    pub label: String,
    pub amount_cents: i64,
    /// True when the ceiling cut this line short.
    pub clipped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stacked {
    /// Only lines that take something off. A code the ceiling reduced to
    /// nothing is not applied, so it is not redeemed either.
    pub lines: Vec<DiscountLine>,
    pub total_off_cents: i64,
    pub ceiling_cents: i64,
    pub ceiling_binds: bool,
    /// Credit that did not fit this move and stays on the account.
    pub credit_rollover_cents: i64,
    /// A code was offered but the corporate rate was larger, so the code is
    /// not used — and not spent.
    pub code_lost_to_corporate: bool,
}

/// Take up to `want` out of the room left under the ceiling.
fn take(kind: LineKind, label: &str, want: i64, room: &mut i64, binds: &mut bool, lines: &mut Vec<DiscountLine>) -> i64 {
    let want = want.max(0);
    let got = want.min(*room);
    if got < want {
        *binds = true;
    }
    if got > 0 {
        *room -= got;
        lines.push(DiscountLine { kind, label: label.to_owned(), amount_cents: got, clipped: got < want });
    }
    got
}

pub fn stack(gross: &Gross, c: &Candidates, ceiling: &Ceiling) -> Stacked {
    let ceiling_cents = ceiling.room(gross);
    let mut room = ceiling_cents;
    let mut binds = false;
    let mut lines = Vec::new();

    // 1. The larger of code and corporate. On a tie the corporate rate wins,
    //    so the customer's one code a month is not spent for nothing.
    let mut code_lost_to_corporate = false;
    match (&c.code, &c.corporate) {
        (Some(code), Some(corp)) if corp.cents >= code.cents => {
            code_lost_to_corporate = true;
            take(LineKind::Corporate, &corp.label, corp.cents, &mut room, &mut binds, &mut lines);
        }
        (Some(code), _) => {
            take(LineKind::Code, &code.label, code.cents, &mut room, &mut binds, &mut lines);
        }
        (None, Some(corp)) => {
            take(LineKind::Corporate, &corp.label, corp.cents, &mut room, &mut binds, &mut lines);
        }
        (None, None) => {}
    }

    // 2. Tier, within its own cap first.
    if let Some((tier, cap)) = &c.tier {
        let want = tier.cents.min((*cap).max(0));
        take(LineKind::Tier, &tier.label, want, &mut room, &mut binds, &mut lines);
    }

    // 3. Credit last. What does not fit rolls over.
    let credit = c.credit_cents.max(0);
    let used = take(LineKind::Credit, "Account credit", credit, &mut room, &mut binds, &mut lines);

    let total_off_cents = lines.iter().map(|l| l.amount_cents).sum();
    Stacked {
        lines,
        total_off_cents,
        ceiling_cents,
        ceiling_binds: binds,
        credit_rollover_cents: credit - used,
        code_lost_to_corporate,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(label: &str, cents: i64) -> Candidate {
        Candidate { label: label.into(), cents }
    }

    /// The design's defaults: 20% of gross, 50 units.
    const CEILING: Ceiling = Ceiling { pct: 20, flat_cents: 5_000 };

    #[test]
    fn the_ceiling_is_the_smaller_of_the_share_and_the_flat_cap() {
        // 20% of 214.00 is 42.80, under 50.
        assert_eq!(CEILING.room(&Gross { carriage_cents: 21_400, accessorial_cents: 0 }), 4_280);
        // 20% of 400.00 is 80.00; the flat 50 binds.
        assert_eq!(CEILING.room(&Gross { carriage_cents: 30_000, accessorial_cents: 10_000 }), 5_000);
    }

    #[test]
    fn a_code_alone_comes_off_whole_when_it_fits() {
        let gross = Gross { carriage_cents: 30_000, accessorial_cents: 10_000 };
        let s = stack(&gross, &Candidates { code: Some(cand("MOVE20", 2_500)), ..Default::default() }, &CEILING);
        assert_eq!(s.total_off_cents, 2_500);
        assert!(!s.ceiling_binds);
    }

    /// The larger wins; they never add. The code is not spent.
    #[test]
    fn a_code_and_the_corporate_rate_compete() {
        let gross = Gross { carriage_cents: 30_000, accessorial_cents: 0 };
        let c = Candidates {
            code: Some(cand("MOVE20", 2_500)),
            corporate: Some(cand("ACME-FRT", 3_600)),
            ..Default::default()
        };
        let s = stack(&gross, &c, &CEILING);
        assert_eq!(s.lines.len(), 1);
        assert_eq!(s.lines[0].kind, LineKind::Corporate);
        assert!(s.code_lost_to_corporate);

        let c = Candidates { corporate: Some(cand("ACME-FRT", 1_000)), ..c };
        let s = stack(&gross, &c, &CEILING);
        assert_eq!(s.lines[0].kind, LineKind::Code);
        assert!(!s.code_lost_to_corporate);
    }

    #[test]
    fn the_tier_is_held_to_its_own_cap_before_the_ceiling() {
        let gross = Gross { carriage_cents: 30_000, accessorial_cents: 10_000 };
        let c = Candidates { tier: Some((cand("Silver", 3_000), 2_000)), ..Default::default() };
        let s = stack(&gross, &c, &CEILING);
        assert_eq!(s.total_off_cents, 2_000);
        assert!(!s.ceiling_binds, "the tier cap is not the ceiling");
    }

    /// Code 25 + tier 20 + credit 15 = 60 wanted against a ceiling of 50.
    /// Credit is last, so it is what gives way, and the rest rolls over.
    #[test]
    fn credit_comes_off_last_and_rolls_over() {
        let gross = Gross { carriage_cents: 30_000, accessorial_cents: 10_000 };
        let c = Candidates {
            code: Some(cand("MOVE20", 2_500)),
            tier: Some((cand("Silver", 2_000), 2_000)),
            credit_cents: 1_500,
            ..Default::default()
        };
        let s = stack(&gross, &c, &CEILING);
        assert_eq!(s.total_off_cents, 5_000);
        assert!(s.ceiling_binds);
        assert_eq!(s.credit_rollover_cents, 1_000);
        let credit = s.lines.iter().find(|l| l.kind == LineKind::Credit).unwrap();
        assert_eq!(credit.amount_cents, 500);
        assert!(credit.clipped);
    }

    /// A ceiling of 0 turns discounts off. The code is then not applied, so it
    /// must not be redeemed.
    #[test]
    fn a_code_the_ceiling_leaves_nothing_of_is_not_applied() {
        let gross = Gross { carriage_cents: 30_000, accessorial_cents: 0 };
        let off = Ceiling { pct: 0, flat_cents: 5_000 };
        let s = stack(&gross, &Candidates { code: Some(cand("MOVE20", 2_500)), ..Default::default() }, &off);
        assert!(s.lines.is_empty());
        assert!(s.ceiling_binds);
    }

    #[test]
    fn nothing_offered_takes_nothing_off() {
        let gross = Gross { carriage_cents: 30_000, accessorial_cents: 0 };
        let s = stack(&gross, &Candidates::default(), &CEILING);
        assert_eq!(s.total_off_cents, 0);
        assert!(!s.ceiling_binds);
    }
}
