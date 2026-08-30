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

                LegRecovery::Escalate if leg.escalated_at.is_some() => {
                    // Already raised. Found by running the sweep against a real
                    // database: without this it re-raised the same leg on every
                    // 60-second tick, so one kitchen nobody could reach wrote
                    // sixty telemetry rows and paged ops sixty times in an hour.
                    // An alert that repeats every minute is an alert nobody
                    // reads. The leg stays `pending` and stays in the queue —
                    // it is the *notification* that fires once, not the problem
                    // that goes away.
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

                    // Stamped last, and only after the alert has been raised: a
                    // stamp written first would silence a leg whose telemetry
                    // then failed to write, which is the one case where the
                    // alert matters most.
                    if let Err(err) = self.legs.mark_escalated(leg.tenant_id, leg.leg_id).await {
                        tracing::error!(err = %err, leg_id = %leg.leg_id,
                            "could not stamp leg as escalated — it will raise again next tick");
                    }
                }
            }
        }

        Ok(escalated)
    }
}

#[cfg(test)]
mod sweep_tests {
    use super::*;
    use crate::domain::repositories::{
        AwaitingLeg, LegTransition, TransitionResponse, VendorLegRow,
    };
    use crate::domain::entities::TelemetryEvent;
    use std::sync::Mutex;
    use uuid::Uuid;

    /// Records what the sweep did, and lets a test say which legs are already
    /// stamped without needing a database.
    #[derive(Default)]
    struct Legs {
        rows:      Mutex<Vec<AwaitingLeg>>,
        escalated: Mutex<Vec<Uuid>>,
    }

    #[async_trait::async_trait]
    impl VendorLegRepository for Legs {
        async fn find_awaiting_acceptance(&self) -> anyhow::Result<Vec<AwaitingLeg>> {
            Ok(self.rows.lock().unwrap().clone())
        }
        async fn mark_escalated(&self, _t: Uuid, leg_id: Uuid) -> anyhow::Result<()> {
            self.escalated.lock().unwrap().push(leg_id);
            // Mirrors the real UPDATE: once stamped, later sweeps see it set.
            for r in self.rows.lock().unwrap().iter_mut() {
                if r.leg_id == leg_id {
                    r.escalated_at = Some(Utc::now());
                }
            }
            Ok(())
        }
        async fn transition(
            &self, _t: Uuid, _v: Uuid, _l: Uuid, _to: LegStatus,
            _r: Option<i32>, _rr: Option<&str>,
        ) -> anyhow::Result<LegTransition> {
            unreachable!("the ladder must never move a leg")
        }
        async fn list_open(&self, _t: Uuid, _v: Uuid) -> anyhow::Result<Vec<VendorLegRow>> {
            Ok(vec![])
        }
        async fn find_idempotent_response(
            &self, _t: Uuid, _v: Uuid, _k: &str,
        ) -> anyhow::Result<Option<TransitionResponse>> {
            Ok(None)
        }
        async fn record_idempotent_response(
            &self, _t: Uuid, _v: Uuid, _k: &str, _l: Uuid, _a: &str, _resp: &TransitionResponse,
        ) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct Events {
        realerts: Mutex<Vec<Uuid>>,
    }

    #[async_trait::async_trait]
    impl VendorLegEvents for Events {
        async fn leg_received(&self, leg: &LegRef) -> anyhow::Result<()> {
            self.realerts.lock().unwrap().push(leg.leg_id);
            Ok(())
        }
        async fn leg_accepted(&self, _l: &LegRef, _r: i32) -> anyhow::Result<()> { Ok(()) }
        async fn leg_rejected(&self, _l: &LegRef, _r: &str) -> anyhow::Result<()> { Ok(()) }
    }

    #[derive(Default)]
    struct Telemetry {
        appended: Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl crate::domain::repositories::TelemetryRepository for Telemetry {
        async fn append(&self, e: &TelemetryEvent) -> anyhow::Result<()> {
            self.appended.lock().unwrap().push(e.event_type.clone());
            Ok(())
        }
        async fn timeline(
            &self, _t: Uuid, _o: Uuid,
        ) -> anyhow::Result<Vec<TelemetryEvent>> {
            Ok(vec![])
        }
    }

    fn leg(age_mins: i64, stamped: bool) -> AwaitingLeg {
        AwaitingLeg {
            leg_id:               Uuid::new_v4(),
            order_id:             Uuid::new_v4(),
            tenant_id:            Uuid::new_v4(),
            vendor_id:            Uuid::new_v4(),
            goods_subtotal_cents: 1_000,
            created_at:           Utc::now() - Duration::minutes(age_mins),
            escalated_at:         if stamped { Some(Utc::now()) } else { None },
        }
    }

    fn service(rows: Vec<AwaitingLeg>) -> (LegRecoveryService, Arc<Legs>, Arc<Events>, Arc<Telemetry>) {
        let legs = Arc::new(Legs { rows: Mutex::new(rows), escalated: Mutex::new(vec![]) });
        let events = Arc::new(Events::default());
        let telemetry = Arc::new(Telemetry::default());
        let svc = LegRecoveryService::new(legs.clone(), events.clone(), telemetry.clone());
        (svc, legs, events, telemetry)
    }

    #[tokio::test]
    async fn an_old_unanswered_leg_is_raised_exactly_once_across_repeated_sweeps() {
        // The defect this test exists for, found by running the sweep against a
        // real database: it re-raised the same leg on every 60-second tick, so
        // one unreachable kitchen paged ops sixty times in an hour.
        let (svc, legs, _ev, tel) = service(vec![leg(30, false)]);

        assert_eq!(svc.sweep().await.unwrap(), 1, "first pass raises it");
        assert_eq!(svc.sweep().await.unwrap(), 0, "second pass must stay silent");
        assert_eq!(svc.sweep().await.unwrap(), 0, "and every pass after");

        assert_eq!(legs.escalated.lock().unwrap().len(), 1, "stamped once");
        assert_eq!(
            tel.appended.lock().unwrap().len(), 1,
            "one telemetry row, not one per tick",
        );
    }

    #[tokio::test]
    async fn a_leg_in_the_realert_window_is_re_published_and_not_escalated() {
        let (svc, legs, ev, tel) = service(vec![leg(4, false)]);

        assert_eq!(svc.sweep().await.unwrap(), 0, "not old enough to escalate");
        assert_eq!(ev.realerts.lock().unwrap().len(), 1, "re-alerted instead");
        assert!(legs.escalated.lock().unwrap().is_empty());
        assert!(tel.appended.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_fresh_leg_is_left_entirely_alone() {
        let (svc, legs, ev, tel) = service(vec![leg(1, false)]);

        assert_eq!(svc.sweep().await.unwrap(), 0);
        assert!(ev.realerts.lock().unwrap().is_empty(), "no chime inside the grace window");
        assert!(legs.escalated.lock().unwrap().is_empty());
        assert!(tel.appended.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn an_already_stamped_leg_is_silent_even_though_it_is_still_pending() {
        // It stays in the queue and still blocks the order. It is the
        // notification that fires once, not the problem that goes away.
        let (svc, legs, ev, tel) = service(vec![leg(120, true)]);

        assert_eq!(svc.sweep().await.unwrap(), 0);
        assert!(legs.escalated.lock().unwrap().is_empty());
        assert!(tel.appended.lock().unwrap().is_empty());
        assert!(ev.realerts.lock().unwrap().is_empty());
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
