//! How much of a captured payment a cancellation returns.
//!
//! order-intake states the retention as a rate, `retention_bps`, never as an
//! amount. It cannot reliably know what was captured: the booking total lives in
//! the quote token rather than on the shipment, and partial capture exists.
//! Payments does know, so the fee is computed here, on what was actually taken.
//!
//! Backward compatible by construction: an event with no `retention_bps` came
//! from an order-intake that predates the penalty policy, and those
//! cancellations were always full refunds.

/// 100%, in basis points.
pub const FULL_RETENTION_BPS: i64 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefundDecision {
    /// Refund everything that was captured.
    Full,
    /// Refund this many cents, fewer than were captured.
    Partial(i64),
    /// Everything retained. Record nothing as owed and call no gateway.
    None,
}

/// Retention rounds down, in the customer's favour: a fraction of a cent the
/// customer did not owe is never kept.
pub fn refund_decision(captured_cents: i64, retention_bps: Option<i64>) -> Result<RefundDecision, String> {
    let bps = match retention_bps {
        None | Some(0) => return Ok(RefundDecision::Full),
        Some(b) if !(0..=FULL_RETENTION_BPS).contains(&b) => {
            return Err(format!("retention of {b} bps is outside 0..={FULL_RETENTION_BPS}"));
        }
        Some(b) => b,
    };

    let captured = captured_cents.max(0);
    // i128 so a large capture times 10_000 cannot overflow.
    let retained = (captured as i128 * bps as i128 / FULL_RETENTION_BPS as i128) as i64;
    let refund = captured - retained;

    Ok(if refund <= 0 {
        RefundDecision::None
    } else if refund == captured {
        RefundDecision::Full
    } else {
        RefundDecision::Partial(refund)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No rate came from an order-intake predating the policy: a full refund.
    #[test]
    fn an_event_from_before_the_policy_is_a_full_refund() {
        assert_eq!(refund_decision(21_400, None), Ok(RefundDecision::Full));
        assert_eq!(refund_decision(21_400, Some(0)), Ok(RefundDecision::Full));
    }

    /// 15% of 21_400 is 3_210 retained, so 18_190 comes back.
    #[test]
    fn a_late_cancel_refunds_only_the_remainder() {
        assert_eq!(refund_decision(21_400, Some(1_500)), Ok(RefundDecision::Partial(18_190)));
    }

    /// Move day: everything retained. Call no gateway.
    #[test]
    fn a_full_retention_calls_no_gateway() {
        assert_eq!(refund_decision(21_400, Some(FULL_RETENTION_BPS)), Ok(RefundDecision::None));
    }

    /// 15% of 101 is 15.15; the customer keeps the .15.
    #[test]
    fn retention_rounds_down_in_the_customers_favour() {
        assert_eq!(refund_decision(101, Some(1_500)), Ok(RefundDecision::Partial(86)));
        // A rate so small it rounds to nothing retained is simply a full refund.
        assert_eq!(refund_decision(9_999, Some(1)), Ok(RefundDecision::Full));
    }

    /// The fee is computed on what was captured, not on what was authorized.
    #[test]
    fn retention_is_a_share_of_what_was_captured() {
        assert_eq!(refund_decision(10_000, Some(1_500)), Ok(RefundDecision::Partial(8_500)));
    }

    #[test]
    fn a_rate_above_100_percent_is_refused() {
        assert!(refund_decision(21_400, Some(FULL_RETENTION_BPS + 1)).is_err());
    }

    #[test]
    fn a_negative_rate_is_refused() {
        assert!(refund_decision(21_400, Some(-1)).is_err());
    }
}
