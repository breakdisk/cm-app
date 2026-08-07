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
    pub created_at:   DateTime<Utc>,
    pub updated_at:   DateTime<Utc>,
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
            created_at: now,
            updated_at: now,
        }
    }

    /// Can this courier be offered work right now? Deliberately conservative:
    /// anything other than an active, available courier is a no.
    pub fn is_dispatchable(&self) -> bool {
        self.is_active && self.status == CourierStatus::Available
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

    #[test]
    fn a_new_courier_starts_offline_and_unavailable() {
        let c = courier();
        assert_eq!(c.status, CourierStatus::Offline);
        assert!(!c.is_dispatchable());
    }

    #[test]
    fn only_an_available_active_courier_is_dispatchable() {
        let mut c = courier();
        c.go_available();
        assert!(c.is_dispatchable());

        c.go_offline();
        assert!(!c.is_dispatchable());

        c.go_available();
        c.is_active = false;
        assert!(!c.is_dispatchable(), "a deactivated courier must never be dispatchable");
    }

    /// An assigned courier is not offerable to a second product — this is the
    /// entity-level half of ADR-0015's load-bearing invariant.
    #[test]
    fn an_assigned_courier_is_not_dispatchable() {
        let mut c = courier();
        c.go_available();
        c.mark_assigned();
        assert_eq!(c.status, CourierStatus::Assigned);
        assert!(!c.is_dispatchable());
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
