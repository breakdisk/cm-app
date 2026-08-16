//! Carrier authorization policy.
//!
//! Carrier records are written by two very different callers:
//!
//! * **Tenant operators** (`admin` / `tenant_admin`) hold `carriers:manage` and
//!   act on every carrier in the tenant — onboarding, lifecycle, KYB review.
//! * **Partners** (`partner`) hold `carriers:manage-own` and operate exactly one
//!   carrier: the record whose `contact_email` matches their JWT email.
//!
//! Before ADR-0013 lands a real `partner_id` claim, contact-email identity is
//! the only binding between a portal user and a carrier row — it is the same
//! resolution `GET /v1/carriers/me` and every marketplace handler already uses.
//!
//! This module holds the decision as a pure function so the policy lives in one
//! testable place instead of being restated in eight handlers.

use uuid::Uuid;

use logisticos_auth::rbac::permissions;
use logisticos_errors::AppError;

/// What a caller may do to carrier records, derived from their JWT permissions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CarrierAuthority {
    /// `carriers:manage` — any carrier within the caller's tenant.
    Tenant,
    /// `carriers:manage-own` — only the caller's own carrier record.
    OwnOnly,
    /// Neither permission held.
    None,
}

impl CarrierAuthority {
    /// `carriers:manage` wins when a caller somehow holds both, so adding the
    /// narrow permission to an admin role can never *reduce* their access.
    pub fn from_permissions<S: AsRef<str>>(perms: &[S]) -> Self {
        let has = |want: &str| perms.iter().any(|p| p.as_ref() == want);
        if has(permissions::CARRIERS_MANAGE) {
            Self::Tenant
        } else if has(permissions::CARRIERS_MANAGE_OWN) {
            Self::OwnOnly
        } else {
            Self::None
        }
    }

    /// Onboarding a *new* carrier is a tenant act — a partner must not be able
    /// to conjure carrier records (each one is a rate-shop participant).
    pub fn allows_onboarding(self) -> bool {
        matches!(self, Self::Tenant)
    }

    /// Activate / suspend. Withheld from partners deliberately: self-activation
    /// would bypass the admin review flow, and suspending is how the tenant
    /// removes a carrier from allocation. It is the tenant's lever, not the
    /// partner's — in either direction.
    pub fn allows_lifecycle(self) -> bool {
        matches!(self, Self::Tenant)
    }

    /// Writing `compliance_status` is the KYB verdict. A partner submitting
    /// documents moves to `under_review` through the upload endpoint; only a
    /// tenant operator may declare a carrier `compliant`.
    pub fn allows_compliance_override(self) -> bool {
        matches!(self, Self::Tenant)
    }
}

/// Decide whether `authority` may act on carrier `target`.
///
/// `own` is the id of the caller's own carrier, or `None` when no carrier row
/// matches their email. An `OwnOnly` caller with no carrier record can act on
/// nothing.
///
/// Denials return `NotFound`, never `Forbidden`: a partner probing ids must not
/// be able to tell "exists but is not yours" from "does not exist". This
/// mirrors the existing cross-tenant guard in the carrier handlers.
pub fn authorize_carrier_target(
    authority: CarrierAuthority,
    target: Uuid,
    own: Option<Uuid>,
) -> Result<(), AppError> {
    let permitted = match authority {
        CarrierAuthority::Tenant => true,
        CarrierAuthority::OwnOnly => own == Some(target),
        CarrierAuthority::None => false,
    };
    if permitted {
        Ok(())
    } else {
        Err(AppError::NotFound { resource: "Carrier", id: target.to_string() })
    }
}

/// Read-side counterpart to [`authorize_carrier_target`].
///
/// A caller holding only the narrow `carriers:manage-own` is a partner, and
/// sees just their own record — a competitor's rate cards and SLA history are
/// commercially sensitive. Any other `carriers:read` holder reads tenant-wide;
/// the handler's tenant guard is what stops it there.
pub fn authorize_carrier_read(
    authority: CarrierAuthority,
    target: Uuid,
    own: Option<Uuid>,
) -> Result<(), AppError> {
    match authority {
        CarrierAuthority::OwnOnly if own != Some(target) => {
            Err(AppError::NotFound { resource: "Carrier", id: target.to_string() })
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn perms(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    // ── Authority derivation ──────────────────────────────────────────────

    #[test]
    fn admin_permission_yields_tenant_authority() {
        let a = CarrierAuthority::from_permissions(&perms(&["carriers:read", "carriers:manage"]));
        assert_eq!(a, CarrierAuthority::Tenant);
    }

    #[test]
    fn partner_permission_yields_own_only_authority() {
        let a = CarrierAuthority::from_permissions(&perms(&["carriers:read", "carriers:manage-own"]));
        assert_eq!(a, CarrierAuthority::OwnOnly);
    }

    #[test]
    fn read_only_caller_has_no_write_authority() {
        let a = CarrierAuthority::from_permissions(&perms(&["carriers:read"]));
        assert_eq!(a, CarrierAuthority::None);
    }

    #[test]
    fn manage_outranks_manage_own_when_both_held() {
        let a = CarrierAuthority::from_permissions(&perms(&["carriers:manage-own", "carriers:manage"]));
        assert_eq!(a, CarrierAuthority::Tenant);
    }

    /// `carriers:manage-own` must not be satisfied by a prefix/substring match
    /// against `carriers:manage` in either direction.
    #[test]
    fn manage_own_is_not_matched_by_manage_prefix() {
        let a = CarrierAuthority::from_permissions(&perms(&["carriers:manage-own"]));
        assert_ne!(a, CarrierAuthority::Tenant);
    }

    // ── Target authorization ──────────────────────────────────────────────

    #[test]
    fn tenant_authority_reaches_any_carrier() {
        let other = Uuid::new_v4();
        assert!(authorize_carrier_target(CarrierAuthority::Tenant, other, Some(Uuid::new_v4())).is_ok());
        assert!(authorize_carrier_target(CarrierAuthority::Tenant, other, None).is_ok());
    }

    #[test]
    fn own_only_reaches_its_own_carrier() {
        let mine = Uuid::new_v4();
        assert!(authorize_carrier_target(CarrierAuthority::OwnOnly, mine, Some(mine)).is_ok());
    }

    /// The core regression: a partner must not touch a competitor's record.
    #[test]
    fn own_only_is_denied_another_carrier() {
        let mine = Uuid::new_v4();
        let competitor = Uuid::new_v4();
        let err = authorize_carrier_target(CarrierAuthority::OwnOnly, competitor, Some(mine))
            .expect_err("partner must not reach a competitor's carrier");
        assert!(matches!(err, AppError::NotFound { .. }), "got {err:?}");
    }

    #[test]
    fn own_only_without_a_carrier_record_reaches_nothing() {
        let target = Uuid::new_v4();
        assert!(authorize_carrier_target(CarrierAuthority::OwnOnly, target, None).is_err());
    }

    #[test]
    fn no_authority_is_denied_even_its_own_carrier() {
        let mine = Uuid::new_v4();
        assert!(authorize_carrier_target(CarrierAuthority::None, mine, Some(mine)).is_err());
    }

    /// Denial must be indistinguishable from a missing record so partners
    /// cannot enumerate the tenant's carrier ids.
    #[test]
    fn denial_does_not_leak_carrier_existence() {
        let err = authorize_carrier_target(CarrierAuthority::OwnOnly, Uuid::new_v4(), Some(Uuid::new_v4()))
            .unwrap_err();
        assert!(!matches!(err, AppError::Forbidden { .. }));
    }

    // ── Read scoping ──────────────────────────────────────────────────────

    #[test]
    fn partner_may_read_its_own_carrier() {
        let mine = Uuid::new_v4();
        assert!(authorize_carrier_read(CarrierAuthority::OwnOnly, mine, Some(mine)).is_ok());
    }

    /// Competitor rate cards and SLA history are commercially sensitive.
    #[test]
    fn partner_may_not_read_a_competitor() {
        let competitor = Uuid::new_v4();
        assert!(authorize_carrier_read(CarrierAuthority::OwnOnly, competitor, Some(Uuid::new_v4())).is_err());
    }

    #[test]
    fn tenant_operator_reads_any_carrier() {
        assert!(authorize_carrier_read(CarrierAuthority::Tenant, Uuid::new_v4(), None).is_ok());
    }

    /// `carriers:read` without either manage permission stays tenant-wide —
    /// no role holds that combination today, and read is a tenant-scoped
    /// permission by definition.
    #[test]
    fn plain_read_authority_is_tenant_wide() {
        assert!(authorize_carrier_read(CarrierAuthority::None, Uuid::new_v4(), None).is_ok());
    }

    // ── Capability gates ──────────────────────────────────────────────────

    #[test]
    fn partner_cannot_onboard_activate_suspend_or_self_certify() {
        let a = CarrierAuthority::OwnOnly;
        assert!(!a.allows_onboarding(), "partner must not onboard carriers");
        assert!(!a.allows_lifecycle(), "partner must not activate/suspend");
        assert!(!a.allows_compliance_override(), "partner must not self-certify KYB");
    }

    #[test]
    fn tenant_operator_retains_every_capability() {
        let a = CarrierAuthority::Tenant;
        assert!(a.allows_onboarding());
        assert!(a.allows_lifecycle());
        assert!(a.allows_compliance_override());
    }
}
