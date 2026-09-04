use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Why a delivery could not be completed.
///
/// Deliberately small. Each value earns its place by changing what ops does
/// next; anything finer belongs in `note`, which a human reads. A set that
/// grows to cover every story a courier might tell becomes a set nobody
/// filters on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExceptionReason {
    /// No answer at the door or by phone.
    CustomerUnreachable,
    /// The pin is wrong, blocked, or cannot be entered.
    AddressUnreachable,
    /// The recipient declined the goods.
    CustomerRefused,
    /// COD order, and the customer has no cash.
    CannotPay,
    /// Damaged in transit or at pickup.
    GoodsDamaged,
    /// Accident, breakdown, or a safety problem. About the courier, not the order.
    CourierBlocked,
}

impl ExceptionReason {
    pub const ALL: &'static [ExceptionReason] = &[
        ExceptionReason::CustomerUnreachable,
        ExceptionReason::AddressUnreachable,
        ExceptionReason::CustomerRefused,
        ExceptionReason::CannotPay,
        ExceptionReason::GoodsDamaged,
        ExceptionReason::CourierBlocked,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            ExceptionReason::CustomerUnreachable => "customer_unreachable",
            ExceptionReason::AddressUnreachable => "address_unreachable",
            ExceptionReason::CustomerRefused => "customer_refused",
            ExceptionReason::CannotPay => "cannot_pay",
            ExceptionReason::GoodsDamaged => "goods_damaged",
            ExceptionReason::CourierBlocked => "courier_blocked",
        }
    }

    /// Case-sensitive on purpose: the wire format is one spelling, and
    /// accepting variants of it invites two clients that disagree.
    pub fn parse(s: &str) -> Option<ExceptionReason> {
        ExceptionReason::ALL.iter().copied().find(|r| r.as_str() == s)
    }
}

/// One report from a courier that a delivery could not be completed.
///
/// Append-only in practice: nothing in Phase 1 updates a row after it is
/// written. Phase 2 sets the `resolved_*` trio and nothing else.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignmentException {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub assignment_id: Uuid,
    pub courier_id: Uuid,
    pub reason: ExceptionReason,
    pub note: Option<String>,
    /// D4: where the goods ended up, in the courier's words.
    pub goods_disposition: Option<String>,
    pub capture_lat: Option<f64>,
    pub capture_lng: Option<f64>,
    /// Supplied by the app, stable across offline replays of the same tap.
    pub client_ref: Uuid,
    /// The phone's clock at the tap. Absent for anything not raised on a device.
    pub device_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    pub server_timestamp: chrono::DateTime<chrono::Utc>,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
    pub resolved_by: Option<Uuid>,
    pub resolution: Option<String>,
}

impl AssignmentException {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tenant_id: Uuid,
        assignment_id: Uuid,
        courier_id: Uuid,
        reason: ExceptionReason,
        note: Option<String>,
        goods_disposition: Option<String>,
        capture: Option<(f64, f64)>,
        client_ref: Uuid,
        device_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            assignment_id,
            courier_id,
            reason,
            note,
            goods_disposition,
            capture_lat: capture.map(|c| c.0),
            capture_lng: capture.map(|c| c.1),
            client_ref,
            device_timestamp,
            server_timestamp: chrono::Utc::now(),
            resolved_at: None,
            resolved_by: None,
            resolution: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_reason_round_trips_through_its_wire_string() {
        for r in ExceptionReason::ALL {
            assert_eq!(ExceptionReason::parse(r.as_str()), Some(*r));
        }
    }

    /// The set is closed on purpose. An unrecognised reason is a client that
    /// has drifted from the server, and accepting it would put a value in the
    /// ops queue that no triage rule knows how to route.
    #[test]
    fn an_unknown_reason_is_refused_rather_than_stored() {
        assert_eq!(ExceptionReason::parse("customer_was_rude"), None);
        assert_eq!(ExceptionReason::parse(""), None);
        assert_eq!(ExceptionReason::parse("CUSTOMER_UNREACHABLE"), None);
    }

    #[test]
    fn a_new_exception_is_open_and_stamps_the_server_clock() {
        let e = AssignmentException::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            ExceptionReason::CannotPay,
            Some("no cash, asked to pay by card".to_owned()),
            Some("left with the customer's neighbour".to_owned()),
            Some((14.5995, 120.9842)),
            Uuid::new_v4(),
            None,
        );

        assert!(e.resolved_at.is_none(), "a new exception is open");
        assert_eq!(e.reason, ExceptionReason::CannotPay);
        assert_eq!(e.capture_lat, Some(14.5995));
        assert!(e.device_timestamp.is_none());
    }

    /// The courier's own clock is kept even when it disagrees with ours: a phone
    /// that queued this offline is the only witness to when it actually happened.
    #[test]
    fn a_device_timestamp_is_preserved_rather_than_replaced() {
        let tapped = chrono::Utc::now() - chrono::Duration::hours(2);
        let e = AssignmentException::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            ExceptionReason::CustomerUnreachable,
            None,
            None,
            None,
            Uuid::new_v4(),
            Some(tapped),
        );
        assert_eq!(e.device_timestamp, Some(tapped));
        assert!(e.server_timestamp > tapped);
    }
}
