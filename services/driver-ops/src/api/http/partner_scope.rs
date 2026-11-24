//! Who may read a carrier's manifest and rider roster, and whose.
//!
//! Both used to be open: the manifest had no permission check at all and took
//! the carrier from a query parameter, and the roster showed the whole tenant.
//! A partner could read any carrier's, and a store rider's plain `driver` role
//! reached them too. The decision is kept apart from the handlers so it can be
//! tested without standing up the service.

use logisticos_auth::{
    claims::Claims,
    partner::{resolve_partner_access, PartnerAccess},
    rbac::permissions,
};
use logisticos_errors::AppError;
use uuid::Uuid;

fn holds_any(c: &Claims, wanted: &[&'static str]) -> bool {
    wanted.iter().any(|p| c.has_permission(p))
}

fn forbidden(first: &'static str) -> AppError {
    AppError::Forbidden { resource: first.to_owned() }
}

/// The carrier a manifest read is limited to (`None` = the whole tenant), or an
/// error when the caller may not read a manifest at all.
///
/// A token bound to a partner always gets that partner's own manifest, whatever
/// `carrier_id` it asked for. An unbound token needs a fleet or carrier
/// permission; a partner role with no `pid` is let through only until
/// enforcement is on, so existing partner sessions keep working while
/// memberships are backfilled, and it can still name any carrier in that window.
pub fn manifest_scope_when(
    c: &Claims,
    enforce: bool,
    requested_carrier: Option<Uuid>,
) -> Result<Option<Uuid>, AppError> {
    let access = resolve_partner_access(c, enforce)?;
    let allowed = match access {
        PartnerAccess::Scoped(_) => holds_any(c, &[
            permissions::FLEET_READ,
            permissions::CARRIERS_MANAGE,
            permissions::CARRIERS_MANAGE_OWN,
            permissions::STORE_RIDERS_MANAGE,
        ]),
        PartnerAccess::Unscoped => holds_any(c, &[
            permissions::FLEET_READ,
            permissions::CARRIERS_MANAGE,
            permissions::CARRIERS_MANAGE_OWN,
        ]),
    };
    if !allowed {
        return Err(forbidden(permissions::FLEET_READ));
    }
    Ok(access.carrier_filter(requested_carrier))
}

/// [`manifest_scope_when`] with the rollout switch read from the environment.
pub fn manifest_scope(c: &Claims, requested_carrier: Option<Uuid>) -> Result<Option<Uuid>, AppError> {
    manifest_scope_when(c, logisticos_auth::partner::enforcement_enabled(), requested_carrier)
}

/// [`roster_scope_when`] with the rollout switch read from the environment.
pub fn roster_scope(c: &Claims) -> Result<Option<Uuid>, AppError> {
    roster_scope_when(c, logisticos_auth::partner::enforcement_enabled())
}

/// The carrier a roster read is limited to (`None` = the whole tenant), or an
/// error when the caller may not list riders.
pub fn roster_scope_when(c: &Claims, enforce: bool) -> Result<Option<Uuid>, AppError> {
    let access = resolve_partner_access(c, enforce)?;
    let allowed = match access {
        // A partner lists its own riders with its own authority.
        PartnerAccess::Scoped(_) => holds_any(c, &[
            permissions::FLEET_READ,
            permissions::CARRIERS_MANAGE_OWN,
            permissions::STORE_RIDERS_MANAGE,
        ]),
        PartnerAccess::Unscoped => c.has_permission(permissions::FLEET_READ),
    };
    if !allowed {
        return Err(forbidden(permissions::FLEET_READ));
    }
    Ok(access.carrier_filter(None))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(roles: &[&str], perms: &[&str], pid: Option<Uuid>) -> Claims {
        Claims::new(
            Uuid::new_v4(), Uuid::new_v4(), "demo".into(), "growth".into(), "x@example.com".into(),
            roles.iter().map(|r| r.to_string()).collect(),
            perms.iter().map(|p| p.to_string()).collect(),
            3600,
        )
        .with_partner(pid)
    }

    #[test]
    fn manifest_ignores_a_requested_carrier_for_a_scoped_token() {
        let (mine, theirs) = (Uuid::new_v4(), Uuid::new_v4());
        let c = token(&["partner"], &[permissions::CARRIERS_MANAGE_OWN], Some(mine));
        assert_eq!(manifest_scope_when(&c, false, Some(theirs)).unwrap(), Some(mine));
        assert_eq!(manifest_scope_when(&c, true, None).unwrap(), Some(mine));
    }

    #[test]
    fn manifest_requires_a_permission() {
        // A platform driver: no fleet or carrier permission, no pid.
        let c = token(&["driver"], &[permissions::SHIPMENT_READ, permissions::DISPATCH_VIEW], None);
        assert!(matches!(manifest_scope_when(&c, true, None), Err(AppError::Forbidden { .. })));
    }

    #[test]
    fn a_store_rider_cannot_read_the_manifest() {
        // The plain driver role with a pid: scoped, but holds nothing for it.
        let c = token(&["driver"], &[permissions::SHIPMENT_READ], Some(Uuid::new_v4()));
        assert!(matches!(manifest_scope_when(&c, true, None), Err(AppError::Forbidden { .. })));
    }

    #[test]
    fn a_store_manager_reads_only_their_own_manifest() {
        let mine = Uuid::new_v4();
        let c = token(&["store_manager"], &[permissions::STORE_RIDERS_MANAGE], Some(mine));
        assert_eq!(manifest_scope_when(&c, true, Some(Uuid::new_v4())).unwrap(), Some(mine));
    }

    #[test]
    fn an_operator_may_name_any_carrier_or_none() {
        let wanted = Uuid::new_v4();
        let c = token(&["admin"], &[permissions::FLEET_READ], None);
        assert_eq!(manifest_scope_when(&c, true, Some(wanted)).unwrap(), Some(wanted));
        assert_eq!(manifest_scope_when(&c, true, None).unwrap(), None);
    }

    /// The rollout window: a partner with no pid keeps working before
    /// enforcement, and is refused with the specific code after it.
    #[test]
    fn a_partner_with_no_pid_works_until_enforcement_then_is_refused() {
        let c = token(&["partner"], &[permissions::CARRIERS_MANAGE_OWN], None);
        let named = Uuid::new_v4();
        assert_eq!(manifest_scope_when(&c, false, Some(named)).unwrap(), Some(named));
        assert!(matches!(manifest_scope_when(&c, true, Some(named)), Err(AppError::PartnerContextRequired)));
    }

    #[test]
    fn partner_lists_only_its_riders() {
        let mine = Uuid::new_v4();
        let c = token(&["partner"], &[permissions::CARRIERS_MANAGE_OWN], Some(mine));
        assert_eq!(roster_scope_when(&c, true).unwrap(), Some(mine));
    }

    #[test]
    fn a_scoped_token_with_fleet_read_is_still_pinned_to_its_partner() {
        let mine = Uuid::new_v4();
        let c = token(&["admin"], &[permissions::FLEET_READ], Some(mine));
        assert_eq!(roster_scope_when(&c, true).unwrap(), Some(mine));
    }

    #[test]
    fn a_store_rider_cannot_list_riders() {
        let c = token(&["driver"], &[permissions::SHIPMENT_READ, permissions::DISPATCH_VIEW], Some(Uuid::new_v4()));
        assert!(matches!(roster_scope_when(&c, true), Err(AppError::Forbidden { .. })));
    }

    #[test]
    fn an_operator_lists_the_whole_roster() {
        let c = token(&["admin"], &[permissions::FLEET_READ], None);
        assert_eq!(roster_scope_when(&c, true).unwrap(), None);
    }

    #[test]
    fn someone_without_fleet_read_or_a_partner_cannot_list_riders() {
        let c = token(&["merchant"], &[permissions::SHIPMENT_CREATE], None);
        assert!(matches!(roster_scope_when(&c, true), Err(AppError::Forbidden { .. })));
    }
}
