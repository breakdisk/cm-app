use uuid::Uuid;

use logisticos_auth::claims::Claims;
use logisticos_auth::partner::{enforcement_enabled, PartnerScope};

/// Derived from a verified JWT and injected into every MCP tool handler.
/// `tenant_id` is always JWT-derived, never taken from tool arguments (ADR-0004).
#[derive(Debug, Clone)]
pub struct McpContext {
    pub tenant_id:   Uuid,
    pub actor_uid:   Uuid,
    pub permissions: Vec<String>,
    pub trace_id:    String,
    /// The partner (carrier or store) the token is bound to, from the `pid`
    /// claim. `None` for a tenant-level token.
    pub pid:         Option<Uuid>,
    /// The token holds a partner role (`partner`, `store_*`) but carries no
    /// `pid`: it lost, or never had, its partner binding. Refused once
    /// `AUTH__ENFORCE_PARTNER_SCOPE` is on, exactly as HTTP handlers do.
    pub partner_role_without_pid: bool,
}

impl McpContext {
    /// Build the context from validated claims. Every constructor must go
    /// through here so `pid` is never forgotten.
    pub fn from_claims(claims: &Claims, trace_id: String) -> Self {
        Self {
            tenant_id:   claims.tenant_id,
            actor_uid:   claims.user_id,
            permissions: claims.permissions.clone(),
            trace_id,
            pid:         claims.pid,
            partner_role_without_pid: matches!(claims.partner_scope(), PartnerScope::MissingPid),
        }
    }

    pub fn has_permission(&self, perm: &str) -> bool {
        self.permissions.contains(&perm.to_owned())
            || self.permissions.contains(&"*".to_owned())
    }

    /// Refuse a partner-bound caller on a tenant-wide hub surface. Same
    /// semantics as `logisticos_auth::partner::require_unscoped`: holding
    /// `shipments:read` (which partner, store and driver roles do) is not
    /// enough to read the hub's containers, customs queue or inventory.
    pub fn require_unscoped(&self, what: &str) -> Result<(), String> {
        self.require_unscoped_when(enforcement_enabled(), what)
    }

    /// [`Self::require_unscoped`] with the rollout switch given, for tests.
    pub fn require_unscoped_when(&self, enforce: bool, what: &str) -> Result<(), String> {
        if self.pid.is_some() {
            return Err(format!("permission denied: {what} is not available to a partner-scoped token"));
        }
        if self.partner_role_without_pid && enforce {
            return Err("permission denied: partner context required".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claims(roles: &[&str], pid: Option<Uuid>) -> Claims {
        Claims::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "demo".into(),
            "growth".into(),
            "x@example.com".into(),
            roles.iter().map(|r| r.to_string()).collect(),
            vec!["shipments:read".into()],
            3600,
        )
        .with_partner(pid)
    }

    fn ctx(roles: &[&str], pid: Option<Uuid>) -> McpContext {
        McpContext::from_claims(&claims(roles, pid), "t".into())
    }

    #[test]
    fn a_scoped_caller_is_refused() {
        let c = ctx(&["driver"], Some(Uuid::new_v4()));
        assert!(c.pid.is_some());
        assert!(c.require_unscoped_when(false, "hub inventory").is_err());
        assert!(c.require_unscoped_when(true, "hub inventory").is_err());
    }

    #[test]
    fn an_unscoped_caller_is_not() {
        let c = ctx(&["tenant_admin"], None);
        assert!(c.require_unscoped_when(false, "hub inventory").is_ok());
        assert!(c.require_unscoped_when(true, "hub inventory").is_ok());
    }

    #[test]
    fn a_partner_role_without_pid_is_refused_only_once_enforced() {
        let c = ctx(&["partner"], None);
        assert!(c.require_unscoped_when(false, "hub inventory").is_ok());
        assert!(c.require_unscoped_when(true, "hub inventory").is_err());
    }
}
