//! The delivery PIN policy.
//!
//! Decided 2026-09-15:
//! - **Required.** A POD cannot be submitted without the recipient's PIN. Every
//!   POD is a delivery — driver-ops has only `Pickup` and `Delivery` tasks, and
//!   the driver app sends pickups, returns and hub drops through POP — so there
//!   is no task type to exempt and no client-supplied flag to trust.
//! - **Shown at booking.** The PIN lives until delivery, not 15 minutes. The
//!   default 60 days covers sea freight (30–45 days) with margin.
//!
//! A PIN that lives for weeks can be guessed through `POST /v1/otps/verify`, so
//! wrong attempts are counted and the PIN locks.

use serde::Deserialize;

/// Env: `DELIVERY_PIN__REQUIRED`, `DELIVERY_PIN__TTL_HOURS`, `DELIVERY_PIN__MAX_ATTEMPTS`.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DeliveryPinPolicy {
    pub required:     bool,
    pub ttl_hours:    i64,
    pub max_attempts: i32,
}

impl Default for DeliveryPinPolicy {
    fn default() -> Self {
        Self { required: true, ttl_hours: 60 * 24, max_attempts: 5 }
    }
}

impl DeliveryPinPolicy {
    pub fn ttl(&self) -> chrono::Duration {
        chrono::Duration::hours(self.ttl_hours)
    }

    /// Called at startup, so a bad policy stops the deploy rather than every delivery.
    pub fn validate(&self) -> Result<(), String> {
        if self.ttl_hours < 1 {
            return Err(format!("DELIVERY_PIN__TTL_HOURS must be at least 1 (got {})", self.ttl_hours));
        }
        if self.max_attempts < 1 {
            return Err(format!("DELIVERY_PIN__MAX_ATTEMPTS must be at least 1 (got {})", self.max_attempts));
        }
        Ok(())
    }
}

/// Why a PIN was not accepted. Surfaced to the driver verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinRejection {
    /// Submit arrived without a PIN while the policy requires one.
    Missing,
    /// No unused, unexpired PIN exists for the shipment.
    NotIssued,
    /// Every attempt has been spent.
    Locked,
    Wrong { attempts_left: i32 },
}

impl PinRejection {
    pub fn message(&self) -> String {
        match self {
            Self::Missing =>
                "A delivery PIN is required. Ask the recipient for the PIN from their booking or text message.".into(),
            Self::NotIssued =>
                "There is no live delivery PIN for this shipment. Send one to the recipient.".into(),
            Self::Locked =>
                "Too many wrong PINs. Send a new PIN to the recipient.".into(),
            Self::Wrong { attempts_left: 0 } =>
                "Wrong PIN. It is now locked — send a new PIN to the recipient.".into(),
            Self::Wrong { attempts_left: 1 } =>
                "Wrong PIN. 1 attempt left.".into(),
            Self::Wrong { attempts_left } =>
                format!("Wrong PIN. {attempts_left} attempts left."),
        }
    }
}

/// Attempts remaining once `attempts_used` — including the one just made — are spent.
pub fn attempts_left(max_attempts: i32, attempts_used: i32) -> i32 {
    (max_attempts - attempts_used).max(0)
}

/// What `POST /v1/otps/generate` did. Whether the caller may see `code` is the
/// handler's decision — a driver never does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssuedPin {
    New { otp_id: uuid::Uuid, code: String },
    KeptLive { otp_id: uuid::Uuid },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueDecision {
    /// Create a new PIN, replacing any earlier one.
    Issue,
    /// Keep the live PIN the recipient already holds.
    KeepLive,
}

/// Whether `POST /v1/otps/generate` issues a new PIN.
///
/// A new PIN replaces the one the recipient holds — on their booking screen, or
/// in the text they already received. The driver app's "send code" at the door
/// used to do exactly that on every tap. A live PIN is now kept unless the
/// caller explicitly asks for a new one. A locked PIN is not live, so it is
/// replaced without asking.
///
/// `live_pin_failed_attempts` is `Some` when an unused, unexpired PIN exists.
pub fn issue_decision(live_pin_failed_attempts: Option<i32>, reissue: bool, max_attempts: i32) -> IssueDecision {
    match live_pin_failed_attempts {
        Some(failed) if !reissue && failed < max_attempts => IssueDecision::KeepLive,
        _ => IssueDecision::Issue,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_policy_requires_a_pin_that_outlives_sea_freight() {
        let p = DeliveryPinPolicy::default();
        assert!(p.required);
        assert!(p.ttl() >= chrono::Duration::days(45), "must outlive a 30–45 day sea shipment");
        assert_eq!(p.max_attempts, 5);
        assert!(p.validate().is_ok());
    }

    #[test]
    fn a_policy_that_could_never_accept_a_pin_does_not_boot() {
        assert!(DeliveryPinPolicy { ttl_hours: 0, ..Default::default() }.validate().is_err());
        assert!(DeliveryPinPolicy { max_attempts: 0, ..Default::default() }.validate().is_err());
    }

    // The customer's booking-screen PIN must survive the driver tapping "send code".
    #[test]
    fn a_live_pin_is_kept_unless_a_new_one_is_asked_for() {
        assert_eq!(issue_decision(Some(0), false, 5), IssueDecision::KeepLive);
        assert_eq!(issue_decision(Some(4), false, 5), IssueDecision::KeepLive);
        assert_eq!(issue_decision(Some(0), true, 5), IssueDecision::Issue);
    }

    #[test]
    fn no_pin_or_a_locked_pin_is_replaced_without_asking() {
        assert_eq!(issue_decision(None, false, 5), IssueDecision::Issue);
        assert_eq!(issue_decision(Some(5), false, 5), IssueDecision::Issue);
    }

    #[test]
    fn attempts_left_never_goes_negative() {
        assert_eq!(attempts_left(5, 1), 4);
        assert_eq!(attempts_left(5, 5), 0);
        assert_eq!(attempts_left(5, 9), 0);
    }

    #[test]
    fn the_last_wrong_attempt_says_the_pin_is_locked() {
        assert!(PinRejection::Wrong { attempts_left: 0 }.message().contains("locked"));
        assert_eq!(PinRejection::Wrong { attempts_left: 1 }.message(), "Wrong PIN. 1 attempt left.");
        assert_eq!(PinRejection::Wrong { attempts_left: 3 }.message(), "Wrong PIN. 3 attempts left.");
    }
}
