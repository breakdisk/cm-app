//! The human operating in the field. Platform-tier: shared by every product
//! that dispatches one, distinct from the customer profile in the CDP.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CourierStatus {
    Offline,
    Available,
    Assigned,
    OnBreak,
}

impl CourierStatus {
    /// The wire and database representation. One definition, so the repository
    /// and the CHECK constraint cannot drift — `{:?}`-lowercasing would render
    /// `OnBreak` as `onbreak` against a column expecting `on_break`.
    pub fn as_str(&self) -> &'static str {
        match self {
            CourierStatus::Offline   => "offline",
            CourierStatus::Available => "available",
            CourierStatus::Assigned  => "assigned",
            CourierStatus::OnBreak   => "on_break",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Courier {
    pub id:           Uuid,
    pub tenant_id:    Uuid,
    pub user_id:      Uuid,
    pub first_name:   String,
    pub last_name:    String,
    pub phone:        String,
    pub status:       CourierStatus,
    pub vehicle_type: Option<String>,
    pub zone:         Option<String>,
    /// Render cache only. The authoritative position is the newest row in
    /// `field_ops.courier_locations`; never proximity-search on these.
    pub last_lat:     Option<f64>,
    pub last_lng:     Option<f64>,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub is_active:    bool,
    /// What the compliance service last said about this courier, verbatim.
    ///
    /// `None` means compliance has never spoken about them — not that they are
    /// non-compliant. Every courier alive today is in that state, so the two
    /// must stay distinguishable: see `compliance_assignable`.
    pub compliance_status: Option<String>,
    /// Whether compliance says this courier may be assigned work.
    ///
    /// Taken verbatim from the `compliance.status_changed` event rather than
    /// re-derived from `compliance_status`. Which statuses are assignable is
    /// compliance's rule (`Expired` is assignable, deliberately — there is a
    /// grace period), and a second copy of it here is how the two services
    /// start disagreeing about who is allowed to work.
    ///
    /// Defaults to `true` so an unknown courier fails open.
    pub compliance_assignable: bool,
    pub compliance_updated_at: Option<DateTime<Utc>>,
    pub created_at:   DateTime<Utc>,
    pub updated_at:   DateTime<Utc>,
}

/// Why a courier is not being offered work.
///
/// One enum because the question has several answers that look identical from
/// outside — a courier who is suspended, one who is off duty and one who is
/// blocked on documents are all simply "not getting jobs" to the person asking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchBlock {
    /// Suspended by ops. Not the courier's own duty toggle.
    Suspended,
    /// The courier has not gone on duty, or is on a break, or is mid-job.
    OffDuty,
    /// Compliance says no. Carries the status so the roster can say which
    /// problem it is — `pending_submission` and `suspended` need different
    /// things from the courier and from ops.
    Compliance(String),
}

impl DispatchBlock {
    /// A stable machine-readable tag for the wire. Not `{:?}` — the roster
    /// renders this and a derived Debug string is not a contract.
    pub fn code(&self) -> &'static str {
        match self {
            DispatchBlock::Suspended     => "suspended",
            DispatchBlock::OffDuty       => "off_duty",
            DispatchBlock::Compliance(_) => "compliance",
        }
    }
}

impl Courier {
    pub fn new(
        tenant_id: Uuid,
        user_id: Uuid,
        first_name: String,
        last_name: String,
        phone: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            user_id,
            first_name,
            last_name,
            phone,
            status: CourierStatus::Offline,
            vehicle_type: None,
            zone: None,
            last_lat: None,
            last_lng: None,
            last_seen_at: None,
            is_active: true,
            // Unknown, not blocked. A courier who has just signed up has no
            // compliance profile yet; refusing to dispatch them here would make
            // registration itself the thing that stops them working.
            compliance_status: None,
            compliance_assignable: true,
            compliance_updated_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Why this courier cannot be offered work right now, or `None` if they
    /// can. Deliberately conservative: anything other than an active, available
    /// and (when enforced) compliance-cleared courier is a no.
    ///
    /// `enforce_compliance` is a rollout flag, not a policy the entity owns.
    /// It ships false: no courier has a compliance profile today, so enforcing
    /// on day one would stop the live fleet the moment profiles start being
    /// created. With it false the compliance term is evaluated and reported but
    /// does not block, which is what makes the observe-only rollout possible.
    ///
    /// Order is not arbitrary. Suspension is ops' lever and outranks everything;
    /// duty is the courier's own and they can fix it in a tap; compliance is
    /// last because it is the slowest to resolve and the least useful thing to
    /// tell someone who is also simply off duty.
    pub fn dispatch_block(&self, enforce_compliance: bool) -> Option<DispatchBlock> {
        if !self.is_active {
            return Some(DispatchBlock::Suspended);
        }
        if self.status != CourierStatus::Available {
            return Some(DispatchBlock::OffDuty);
        }
        if enforce_compliance && !self.compliance_assignable {
            return Some(DispatchBlock::Compliance(
                self.compliance_status
                    .clone()
                    .unwrap_or_else(|| "unknown".to_owned()),
            ));
        }
        None
    }

    /// Can this courier be offered work right now?
    ///
    /// Takes the flag rather than defaulting it: a no-argument
    /// `is_dispatchable()` would let a call site skip the compliance term
    /// without saying so, and silently skipping a gate is the failure mode this
    /// whole change exists to close.
    pub fn is_dispatchable(&self, enforce_compliance: bool) -> bool {
        self.dispatch_block(enforce_compliance).is_none()
    }

    /// Record what compliance last said. Verbatim — `assignable` is their
    /// answer, never re-derived from `status` here.
    pub fn set_compliance(&mut self, status: String, assignable: bool, at: DateTime<Utc>) {
        self.compliance_status = Some(status);
        self.compliance_assignable = assignable;
        self.compliance_updated_at = Some(at);
        self.updated_at = Utc::now();
    }

    pub fn go_available(&mut self) { self.set_status(CourierStatus::Available); }
    pub fn go_offline(&mut self)   { self.set_status(CourierStatus::Offline); }
    pub fn mark_assigned(&mut self) { self.set_status(CourierStatus::Assigned); }

    fn set_status(&mut self, status: CourierStatus) {
        self.status = status;
        self.updated_at = Utc::now();
    }

    pub fn record_position(&mut self, lat: f64, lng: f64) {
        self.last_lat = Some(lat);
        self.last_lng = Some(lng);
        self.last_seen_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn courier() -> Courier {
        Courier::new(Uuid::new_v4(), Uuid::new_v4(), "Rico".into(), "M".into(), "+639170000000".into())
    }

    /// Both enforcement modes, for the cases where compliance is not the
    /// deciding term. Every pre-compliance assertion must hold identically
    /// whether or not the flag is on — that is what makes the rollout safe.
    fn dispatchable_either_way(c: &Courier) -> bool {
        let off = c.is_dispatchable(false);
        assert_eq!(off, c.is_dispatchable(true),
            "compliance must not change this courier's answer");
        off
    }

    #[test]
    fn a_new_courier_starts_offline_and_unavailable() {
        let c = courier();
        assert_eq!(c.status, CourierStatus::Offline);
        assert!(!dispatchable_either_way(&c));
    }

    #[test]
    fn only_an_available_active_courier_is_dispatchable() {
        let mut c = courier();
        c.go_available();
        assert!(dispatchable_either_way(&c));

        c.go_offline();
        assert!(!dispatchable_either_way(&c));

        c.go_available();
        c.is_active = false;
        assert!(!dispatchable_either_way(&c), "a deactivated courier must never be dispatchable");
    }

    /// An assigned courier is not offerable to a second product — this is the
    /// entity-level half of ADR-0015's load-bearing invariant.
    #[test]
    fn an_assigned_courier_is_not_dispatchable() {
        let mut c = courier();
        c.go_available();
        c.mark_assigned();
        assert_eq!(c.status, CourierStatus::Assigned);
        assert!(!dispatchable_either_way(&c));
    }

    // ── Compliance ─────────────────────────────────────────────────────────

    /// The state every courier in production is in right now: nothing has ever
    /// created a compliance profile for them. Unknown must fail OPEN, or
    /// enabling enforcement stops the entire live fleet at once.
    #[test]
    fn a_courier_compliance_has_never_spoken_about_is_dispatchable() {
        let mut c = courier();
        c.go_available();
        assert_eq!(c.compliance_status, None);
        assert!(c.compliance_assignable, "unknown must default to assignable");
        assert!(c.is_dispatchable(true));
    }

    #[test]
    fn a_blocked_courier_is_refused_only_when_enforcement_is_on() {
        let mut c = courier();
        c.go_available();
        c.set_compliance("pending_submission".into(), false, Utc::now());

        assert!(c.is_dispatchable(false),
            "observe-only mode must not change who gets work");
        assert!(!c.is_dispatchable(true));
        assert_eq!(
            c.dispatch_block(true),
            Some(DispatchBlock::Compliance("pending_submission".into())),
        );
    }

    /// Compliance owns which statuses are assignable — `expired` is one of
    /// them, deliberately, because there is a grace period. This entity stores
    /// the answer it is given and never re-derives it from the status string.
    #[test]
    fn assignability_comes_from_the_event_not_the_status_string() {
        let mut c = courier();
        c.go_available();
        c.set_compliance("expired".into(), true, Utc::now());
        assert!(c.is_dispatchable(true),
            "an alarming-sounding status compliance calls assignable must still dispatch");
    }

    /// Suspension outranks compliance: ops pulling someone off the road is the
    /// answer worth reporting, and it is the one they can act on.
    #[test]
    fn suspension_is_reported_ahead_of_compliance() {
        let mut c = courier();
        c.go_available();
        c.is_active = false;
        c.set_compliance("rejected".into(), false, Utc::now());
        assert_eq!(c.dispatch_block(true), Some(DispatchBlock::Suspended));
    }

    /// Off duty outranks compliance too. Telling a courier who simply has not
    /// clocked on that their documents are the problem sends them to the wrong
    /// screen.
    #[test]
    fn being_off_duty_is_reported_ahead_of_compliance() {
        let mut c = courier();
        c.go_offline();
        c.set_compliance("rejected".into(), false, Utc::now());
        assert_eq!(c.dispatch_block(true), Some(DispatchBlock::OffDuty));
    }

    /// The block codes are rendered by the ops roster, so they are a wire
    /// contract rather than a debug convenience.
    #[test]
    fn block_codes_are_stable() {
        assert_eq!(DispatchBlock::Suspended.code(), "suspended");
        assert_eq!(DispatchBlock::OffDuty.code(), "off_duty");
        assert_eq!(DispatchBlock::Compliance("x".into()).code(), "compliance");
    }

    /// A courier with no profile is not the same as one compliance cleared,
    /// and the roster has to be able to tell them apart to know who still
    /// needs onboarding.
    #[test]
    fn unknown_and_cleared_are_distinguishable() {
        let mut unknown = courier();
        let mut cleared = courier();
        cleared.set_compliance("compliant".into(), true, Utc::now());

        unknown.go_available();
        cleared.go_available();

        assert!(unknown.is_dispatchable(true));
        assert!(cleared.is_dispatchable(true));
        assert_eq!(unknown.compliance_status, None);
        assert_eq!(cleared.compliance_status.as_deref(), Some("compliant"));
    }

    #[test]
    fn recording_a_position_updates_last_seen() {
        let mut c = courier();
        assert!(c.last_seen_at.is_none());
        c.record_position(14.5995, 120.9842);
        assert_eq!(c.last_lat, Some(14.5995));
        assert!(c.last_seen_at.is_some());
    }

    /// The status strings are a database contract: the CHECK constraint in
    /// migration 0001 lists exactly these, and `on_break` is the one a derived
    /// lowercase would get wrong.
    #[test]
    fn status_strings_match_the_check_constraint() {
        assert_eq!(CourierStatus::Offline.as_str(), "offline");
        assert_eq!(CourierStatus::Available.as_str(), "available");
        assert_eq!(CourierStatus::Assigned.as_str(), "assigned");
        assert_eq!(CourierStatus::OnBreak.as_str(), "on_break");
    }
}
