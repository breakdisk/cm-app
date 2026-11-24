//! Partner scoping: which partner's data a token may reach.
//!
//! A partner (carrier or neighborhood store) must see only its own rows. Until
//! the `pid` claim existed, a `partner` token reached the whole tenant, and a
//! token without a `pid` still looks exactly like that. So the rollout is
//! staged: services call [`partner_access`], which refuses a partner-role token
//! with no `pid` only once `AUTH__ENFORCE_PARTNER_SCOPE=true`. Before that it is
//! logged and left unscoped, so existing partner sessions keep working while
//! memberships are backfilled. Enforcing first would lock out every partner.

use std::sync::OnceLock;

use logisticos_errors::AppError;
use uuid::Uuid;

use crate::claims::Claims;

/// Roles that mean "acts for a partner". A token holding one of these but no
/// `pid` has lost, or never had, its partner binding.
const PARTNER_ROLES: &[&str] = &["partner", "store_manager", "store_clerk"];

/// What a token says about partner scope, before any rollout switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartnerScope {
    /// Not a partner token. Existing tenant-level rules apply.
    NotPartner,
    /// Bound to this partner, whatever role carried it (including a store rider
    /// holding the plain `driver` role).
    Partner(Uuid),
    /// A partner role with no `pid`.
    MissingPid,
}

/// The decision a service acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartnerAccess {
    /// Apply the service's ordinary tenant-level rules.
    Unscoped,
    /// Restrict every read to this partner's rows.
    Scoped(Uuid),
}

impl Claims {
    pub fn partner_scope(&self) -> PartnerScope {
        match self.pid {
            Some(pid) => PartnerScope::Partner(pid),
            None if PARTNER_ROLES.iter().any(|r| self.has_role(r)) => PartnerScope::MissingPid,
            None => PartnerScope::NotPartner,
        }
    }
}

/// Decide access from a token. `enforce` is the rollout switch; production code
/// calls [`partner_access`], which reads it from the environment.
pub fn resolve_partner_access(c: &Claims, enforce: bool) -> Result<PartnerAccess, AppError> {
    match c.partner_scope() {
        PartnerScope::Partner(pid) => Ok(PartnerAccess::Scoped(pid)),
        PartnerScope::NotPartner => Ok(PartnerAccess::Unscoped),
        PartnerScope::MissingPid if enforce => Err(AppError::PartnerContextRequired),
        PartnerScope::MissingPid => {
            // The log line is the rollout's evidence: enforcement is safe once
            // this has been absent for one full token lifetime.
            tracing::warn!(
                user_id = %c.user_id,
                tenant_id = %c.tenant_id,
                "partner_scope_missing_pid: partner-role token has no pid, left unscoped until enforcement is on"
            );
            Ok(PartnerAccess::Unscoped)
        }
    }
}

fn enforcement_on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("AUTH__ENFORCE_PARTNER_SCOPE")
            .map(|v| v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    })
}

/// [`resolve_partner_access`] with the rollout switch read from
/// `AUTH__ENFORCE_PARTNER_SCOPE`, once per process.
pub fn partner_access(c: &Claims) -> Result<PartnerAccess, AppError> {
    resolve_partner_access(c, enforcement_on())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claims::Claims;
    use logisticos_errors::AppError;
    use uuid::Uuid;

    fn claims_with(roles: &[&str], pid: Option<Uuid>) -> Claims {
        Claims::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "demo".into(),
            "growth".into(),
            "x@example.com".into(),
            roles.iter().map(|r| r.to_string()).collect(),
            vec![],
            3600,
        )
        .with_partner(pid)
    }

    #[test]
    fn old_token_without_pid_still_decodes() {
        let mut json = serde_json::to_value(claims_with(&["partner"], None)).unwrap();
        json.as_object_mut().unwrap().remove("pid");
        let decoded: Claims = serde_json::from_value(json).expect("a token minted before pid must decode");
        assert_eq!(decoded.pid, None);
    }

    #[test]
    fn partner_roles_without_pid_are_missing() {
        for role in ["partner", "store_manager", "store_clerk"] {
            assert_eq!(claims_with(&[role], None).partner_scope(), PartnerScope::MissingPid, "{role}");
        }
    }

    #[test]
    fn platform_driver_without_pid_is_not_a_partner() {
        assert_eq!(claims_with(&["driver"], None).partner_scope(), PartnerScope::NotPartner);
        assert_eq!(claims_with(&["admin"], None).partner_scope(), PartnerScope::NotPartner);
    }

    #[test]
    fn store_rider_with_pid_is_scoped() {
        let pid = Uuid::new_v4();
        assert_eq!(claims_with(&["driver"], Some(pid)).partner_scope(), PartnerScope::Partner(pid));
    }

    #[test]
    fn missing_pid_is_denied_when_enforced() {
        let err = resolve_partner_access(&claims_with(&["partner"], None), true).unwrap_err();
        assert!(matches!(err, AppError::PartnerContextRequired), "got {err:?}");
        assert_eq!(err.error_code(), "PARTNER_CONTEXT_REQUIRED");
        assert_eq!(err.status_code(), axum::http::StatusCode::FORBIDDEN);
    }

    #[test]
    fn missing_pid_stays_unscoped_before_enforcement() {
        let access = resolve_partner_access(&claims_with(&["partner"], None), false).unwrap();
        assert_eq!(access, PartnerAccess::Unscoped);
    }

    #[test]
    fn pid_always_scopes_even_before_enforcement() {
        let pid = Uuid::new_v4();
        for enforce in [false, true] {
            let access = resolve_partner_access(&claims_with(&["partner"], Some(pid)), enforce).unwrap();
            assert_eq!(access, PartnerAccess::Scoped(pid));
        }
    }

    #[test]
    fn non_partner_is_unscoped_whether_or_not_enforced() {
        for enforce in [false, true] {
            let access = resolve_partner_access(&claims_with(&["admin"], None), enforce).unwrap();
            assert_eq!(access, PartnerAccess::Unscoped);
        }
    }
}
