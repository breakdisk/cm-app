//! What to do about a leg no store ever answered.
//!
//! Separate from `recovery_service` because it asks a different question on a
//! different clock. That one asks "did a courier take this order"; this one
//! asks "did the store even look".
//!
//! A sweep rather than a consumer, for the same reason: a leg nobody answered
//! is defined by an event that never arrived, and nothing event-driven can
//! notice an absence. Only a timer can.
//!
//! ## Why the terminal rung is a human and not an auto-reject
//!
//! The collection consumer refuses to credit a leg that is not awaiting
//! collection — that guard is what stops a store being paid for an order it
//! refused. It also means auto-rejecting an unanswered leg would stop a store
//! being paid for food it *did* cook and merely forgot to accept on the tablet.
//! A tablet on bad Wi-Fi during a lunch rush is exactly when that happens, and
//! exactly the wrong moment to silently not pay someone.
//!
//! So an unanswered leg stays `Pending`. That still blocks the order from
//! advancing — `blocks_collection()` includes `Pending` — and ops is told which
//! kitchen to ring.

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};

use crate::domain::entities::telemetry::event_type;
use crate::domain::entities::{LegStatus, TelemetryEvent};
use crate::domain::repositories::{TelemetryRepository, VendorLegRepository};
use crate::infrastructure::messaging::{LegRef, VendorLegEvents};

/// Below this a store simply may not have looked yet. A kitchen at a lunch
/// rush does not check a screen the second it chimes.
const GRACE_MINUTES: i64 = 2;
/// Past this, re-alerting has stopped being useful and a person is needed.
const ESCALATE_MINUTES: i64 = 8;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum LegRecovery {
    /// Answered, or not waiting on an answer.
    None,
    /// Still fresh — leave it alone.
    Wait,
    /// Old enough that the first alert plausibly missed. Send it again.
    Realert,
    /// Out of time. Tell a human. Deliberately not a state change — see the
    /// module docs.
    Escalate,
}

/// What a given leg needs, as of `now`.
///
/// `now` is a parameter rather than an internal `Utc::now()` so the boundaries
/// can be tested exactly. A function that reads the clock itself can only be
/// tested relative to the real clock, which cannot pin a boundary and goes
/// flaky near it. Same reasoning as `recovery_service::decide`.
pub fn decide(answered: bool, created_at: DateTime<Utc>, now: DateTime<Utc>) -> LegRecovery {
    if answered {
        return LegRecovery::None;
    }
    let age = now - created_at;
    if age < Duration::minutes(GRACE_MINUTES) {
        LegRecovery::Wait
    } else if age < Duration::minutes(ESCALATE_MINUTES) {
        LegRecovery::Realert
    } else {
        LegRecovery::Escalate
    }
}

/// The periodic sweep over legs nobody answered.
pub struct LegRecoveryService {
    legs:      Arc<dyn VendorLegRepository>,
    events:    Arc<dyn VendorLegEvents>,
    telemetry: Arc<dyn TelemetryRepository>,
}

impl LegRecoveryService {
    pub fn new(
        legs: Arc<dyn VendorLegRepository>,
        events: Arc<dyn VendorLegEvents>,
        telemetry: Arc<dyn TelemetryRepository>,
    ) -> Self {
        Self { legs, events, telemetry }
    }

    /// One pass. Returns how many legs were escalated, so the caller logs a
    /// number that means something rather than "sweep ran".
    pub async fn sweep(&self) -> anyhow::Result<usize> {
        let now = Utc::now();
        let waiting = self.legs.find_awaiting_acceptance().await?;
        let mut escalated = 0;

        for leg in waiting {
            // Everything the query returns is `pending` by construction, so the
            // leg has not answered. `decide` still takes the flag rather than
            // assuming it — the query is not guaranteed to be its only caller.
            match decide(false, leg.created_at, now) {
                LegRecovery::None | LegRecovery::Wait => {}

                LegRecovery::Realert => {
                    // Republishes the same event checkout published. A transport
                    // that missed the first gets another chance; one that
                    // delivered it delivers a duplicate, which for a store is a
                    // second chime about an order it has not answered — the
                    // right behaviour, not a defect.
                    let r = LegRef {
                        tenant_id:            leg.tenant_id,
                        vendor_id:            leg.vendor_id,
                        order_id:             leg.order_id,
                        leg_id:               leg.leg_id,
                        goods_subtotal_cents: leg.goods_subtotal_cents,
                        status:               LegStatus::Pending,
                    };
                    if let Err(e) = self.events.leg_received(&r).await {
                        tracing::warn!(err = %e, leg_id = %leg.leg_id, "re-alert publish failed");
                    }
                }

                LegRecovery::Escalate => {
                    escalated += 1;
                    // Loud, and with the vendor named: the ops question is
                    // always "is that kitchen open", and an alert that does not
                    // say which kitchen cannot be acted on.
                    tracing::error!(
                        leg_id = %leg.leg_id, order_id = %leg.order_id,
                        vendor_id = %leg.vendor_id, tenant_id = %leg.tenant_id,
                        age_minutes = (now - leg.created_at).num_minutes(),
                        "vendor has not answered this order — needs a human",
                    );

                    let e = TelemetryEvent::new(
                        leg.tenant_id,
                        leg.order_id,
                        event_type::VENDOR_LEG_UNANSWERED,
                        None,
                        None,
                        serde_json::json!({
                            "leg_id":      leg.leg_id,
                            "vendor_id":   leg.vendor_id,
                            "age_minutes": (now - leg.created_at).num_minutes(),
                        }),
                    );
                    if let Err(err) = self.telemetry.append(&e).await {
                        tracing::error!(err = %err, leg_id = %leg.leg_id,
                            "unanswered-leg telemetry failed");
                    }
                }
            }
        }

        Ok(escalated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(mins: i64) -> (DateTime<Utc>, DateTime<Utc>) {
        let created = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        (created, created + Duration::minutes(mins))
    }

    #[test]
    fn an_answered_leg_needs_nothing_however_old_it_is() {
        let (c, n) = at(600);
        assert_eq!(decide(true, c, n), LegRecovery::None);
    }

    #[test]
    fn a_fresh_leg_is_left_alone() {
        let (c, n) = at(1);
        assert_eq!(decide(false, c, n), LegRecovery::Wait);
    }

    #[test]
    fn the_boundaries_are_exact() {
        // Written as exact boundaries because an off-by-one here is a store
        // alerted twice in a minute, or never alerted at all.
        let (c, n) = at(GRACE_MINUTES);
        assert_eq!(decide(false, c, n), LegRecovery::Realert, "grace is exclusive");

        let (c, n) = at(GRACE_MINUTES - 1);
        assert_eq!(decide(false, c, n), LegRecovery::Wait);

        let (c, n) = at(ESCALATE_MINUTES);
        assert_eq!(decide(false, c, n), LegRecovery::Escalate, "escalate is exclusive");

        let (c, n) = at(ESCALATE_MINUTES - 1);
        assert_eq!(decide(false, c, n), LegRecovery::Realert);
    }

    #[test]
    fn a_clock_that_went_backwards_does_not_escalate() {
        // NTP correction, or a row written by a host running fast. A negative
        // age must read as fresh, not as ancient — `Duration` is signed, so
        // this works only because the comparison is `<` against a positive
        // bound rather than an absolute value.
        let created = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let now = created - Duration::minutes(30);
        assert_eq!(decide(false, created, now), LegRecovery::Wait);
    }

    #[test]
    fn the_ladder_only_ever_reads_the_clock_and_never_the_status() {
        // Guards the module's central claim: no rung of this ladder changes a
        // leg. If a future edit adds a state change, `decide` will need a new
        // variant and this test is where that shows up.
        for mins in [0, 1, 2, 5, 7, 8, 60, 6_000] {
            let (c, n) = at(mins);
            let d = decide(false, c, n);
            assert!(
                matches!(d, LegRecovery::Wait | LegRecovery::Realert | LegRecovery::Escalate),
                "age {mins}m produced {d:?}, which is not a rung of the ladder",
            );
        }
    }
}
