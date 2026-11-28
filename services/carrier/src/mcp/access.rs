//! Per-tool access control for the carrier MCP surface.
//!
//! Authentication proves who the caller is; this decides what they may do.
//! Every tool names the permission it needs, taken from the same catalogue the
//! carrier HTTP routes use, so MCP is never looser than HTTP. One table, one
//! function (`authorize`), used by `/mcp/tools` (what is listed) and by
//! `tools::dispatch` (what runs) so the two cannot disagree.
//!
//! Mapping, and why:
//!
//! | tool                      | permission        | HTTP analogue |
//! |---------------------------|-------------------|---------------|
//! | `get_carrier_rates`       | `carriers:read`   | `GET /v1/carriers/rate-shop` |
//! | `track_carrier_shipment`  | `carriers:read`   | read routes under `/v1/carriers/:id/*` |
//! | `get_carrier_label`       | `carriers:read`   | same |
//! | `book_carrier_shipment`   | `carriers:manage` | onboard / activate / suspend |
//! | `cancel_carrier_shipment` | `carriers:manage` | same |
//!
//! Book and cancel have no HTTP twin: the platform books 3PL shipments only
//! from the Kafka allocation worker. They spend the tenant's account with a
//! real carrier, so they take the tenant-wide operator permission and never
//! `carriers:manage-own`, which a partner holds.
//!
//! `partner_denied` tools act on a booking reference or tracking number
//! through the tenant's own 3PL credentials. A partner is another carrier's
//! operator: it may rate-shop (as over HTTP) but must not read or cancel the
//! tenant's bookings.

use logisticos_auth::rbac::permissions;

use super::context::McpContext;

pub struct McpToolRule {
    pub tool: &'static str,
    /// The caller needs this permission (`*` satisfies any).
    pub permission: &'static str,
    /// Refuse a partner-scoped token even if it holds `permission`.
    pub partner_denied: bool,
}

pub const MCP_TOOL_RULES: &[McpToolRule] = &[
    McpToolRule { tool: "get_carrier_rates",       permission: permissions::CARRIERS_READ,   partner_denied: false },
    McpToolRule { tool: "track_carrier_shipment",  permission: permissions::CARRIERS_READ,   partner_denied: true  },
    McpToolRule { tool: "get_carrier_label",       permission: permissions::CARRIERS_READ,   partner_denied: true  },
    McpToolRule { tool: "book_carrier_shipment",   permission: permissions::CARRIERS_MANAGE, partner_denied: true  },
    McpToolRule { tool: "cancel_carrier_shipment", permission: permissions::CARRIERS_MANAGE, partner_denied: true  },
];

/// Why a caller may not use a tool.
#[derive(Debug, PartialEq, Eq)]
pub enum Denied {
    /// Not in the table: unknown, or added without an access decision.
    NotExposed { tool: String },
    MissingPermission { tool: &'static str, permission: &'static str },
    PartnerScoped { tool: &'static str },
}

impl Denied {
    pub fn message(&self) -> String {
        match self {
            Denied::NotExposed { tool } => format!("Tool '{tool}' is not available over MCP"),
            Denied::MissingPermission { tool, permission } => {
                format!("Tool '{tool}' requires the permission: {permission}")
            }
            Denied::PartnerScoped { tool } => {
                format!("Tool '{tool}' is not available to partner-scoped tokens")
            }
        }
    }
}

/// The single access decision for `tool`. Fail-closed: a tool with no rule is
/// refused.
pub fn authorize(ctx: &McpContext, tool: &str) -> Result<(), Denied> {
    let rule = MCP_TOOL_RULES
        .iter()
        .find(|r| r.tool == tool)
        .ok_or_else(|| Denied::NotExposed { tool: tool.to_string() })?;

    if !ctx.has_permission(rule.permission) {
        return Err(Denied::MissingPermission { tool: rule.tool, permission: rule.permission });
    }
    if rule.partner_denied && ctx.partner_id.is_some() {
        return Err(Denied::PartnerScoped { tool: rule.tool });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn ctx(perms: &[&str], partner: bool) -> McpContext {
        McpContext {
            tenant_id:   Uuid::new_v4(),
            actor_uid:   Uuid::new_v4(),
            permissions: perms.iter().map(|p| (*p).to_string()).collect(),
            partner_id:  partner.then(Uuid::new_v4),
            trace_id:    "t".into(),
        }
    }

    #[test]
    fn read_permission_reaches_read_tools_only() {
        let c = ctx(&[permissions::CARRIERS_READ], false);
        for t in ["get_carrier_rates", "track_carrier_shipment", "get_carrier_label"] {
            assert_eq!(authorize(&c, t), Ok(()), "{t}");
        }
        for t in ["book_carrier_shipment", "cancel_carrier_shipment"] {
            assert_eq!(
                authorize(&c, t),
                Err(Denied::MissingPermission { tool: t, permission: permissions::CARRIERS_MANAGE }),
                "{t}"
            );
        }
    }

    #[test]
    fn manage_reaches_write_tools() {
        let c = ctx(&[permissions::CARRIERS_MANAGE], false);
        assert_eq!(authorize(&c, "book_carrier_shipment"), Ok(()));
        assert_eq!(authorize(&c, "cancel_carrier_shipment"), Ok(()));
    }

    #[test]
    fn no_permissions_reach_nothing() {
        let c = ctx(&[], false);
        for r in MCP_TOOL_RULES {
            assert!(authorize(&c, r.tool).is_err(), "{}", r.tool);
        }
    }

    #[test]
    fn manage_own_is_not_enough_for_the_write_tools() {
        let c = ctx(&[permissions::CARRIERS_MANAGE_OWN, permissions::CARRIERS_READ], false);
        assert!(authorize(&c, "book_carrier_shipment").is_err());
        assert!(authorize(&c, "cancel_carrier_shipment").is_err());
    }

    #[test]
    fn wildcard_reaches_every_ruled_tool_and_no_unruled_one() {
        let c = ctx(&["*"], false);
        for r in MCP_TOOL_RULES {
            assert_eq!(authorize(&c, r.tool), Ok(()), "{}", r.tool);
        }
        assert!(matches!(authorize(&c, "drop_all_tables"), Err(Denied::NotExposed { .. })));
    }

    #[test]
    fn a_partner_may_rate_shop_but_not_touch_bookings() {
        let c = ctx(&[permissions::CARRIERS_READ, permissions::CARRIERS_MANAGE], true);
        assert_eq!(authorize(&c, "get_carrier_rates"), Ok(()));
        for t in ["track_carrier_shipment", "get_carrier_label", "book_carrier_shipment", "cancel_carrier_shipment"] {
            assert_eq!(authorize(&c, t), Err(Denied::PartnerScoped { tool: t }), "{t}");
        }
    }

    /// The table and the tool list must name the same tools, so adding a tool
    /// forces an access decision.
    #[test]
    fn every_listed_tool_has_a_rule_and_every_rule_is_listed() {
        let listed: Vec<String> = crate::mcp::tools::list()
            .as_array()
            .map(|a| a.iter().filter_map(|t| t["name"].as_str().map(String::from)).collect())
            .unwrap_or_default();
        assert_eq!(listed.len(), MCP_TOOL_RULES.len());
        for name in &listed {
            assert!(MCP_TOOL_RULES.iter().any(|r| r.tool == name), "{name} has no rule");
        }
    }

    /// MCP must never be looser than HTTP: the permissions here are exactly
    /// the strings the carrier routes demand, and no role may be handed one
    /// it lacks. The write tools must not be satisfiable by the partner role.
    #[test]
    fn rules_use_permissions_existing_roles_already_hold() {
        use logisticos_auth::rbac::default_permissions_for_role;
        let partner = default_permissions_for_role("partner");
        assert!(!partner.contains(&permissions::CARRIERS_MANAGE));
        let admin = default_permissions_for_role("admin");
        for r in MCP_TOOL_RULES {
            assert!(admin.contains(&r.permission), "admin lacks {} for {}", r.permission, r.tool);
        }
    }
}
