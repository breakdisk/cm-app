//! Role-Based Access Control permission definitions.
//!
//! Permissions follow the pattern: `<resource>:<action>`
//! Resources map 1:1 to microservices.
//! Actions: create, read, update, delete, list, assign, approve, export

/// Well-known permission constants used across all services.
/// Services check these via `claims.has_permission(permissions::SHIPMENT_CREATE)`.
pub mod permissions {
    // ── Shipments ────────────────────────────────────────────
    pub const SHIPMENT_CREATE:   &str = "shipments:create";
    pub const SHIPMENT_READ:     &str = "shipments:read";
    pub const SHIPMENT_UPDATE:   &str = "shipments:update";
    pub const SHIPMENT_CANCEL:   &str = "shipments:cancel";
    pub const SHIPMENT_BULK:     &str = "shipments:bulk";
    pub const SHIPMENT_EXPORT:   &str = "shipments:export";

    // ── Dispatch ─────────────────────────────────────────────
    pub const DISPATCH_ASSIGN:   &str = "dispatch:assign";
    pub const DISPATCH_REROUTE:  &str = "dispatch:reroute";
    pub const DISPATCH_VIEW:     &str = "dispatch:view";

    // ── Drivers ──────────────────────────────────────────────
    pub const DRIVER_CREATE:     &str = "drivers:create";
    pub const DRIVER_READ:       &str = "drivers:read";
    pub const DRIVER_MANAGE:     &str = "drivers:manage";

    // ── Fleet ────────────────────────────────────────────────
    pub const FLEET_READ:        &str = "fleet:read";
    pub const FLEET_MANAGE:      &str = "fleet:manage";
    // Aliases used by driver-ops HTTP handlers
    pub const FLEET_VIEW:        &str = "fleet:read";

    // ── Payments / Billing ───────────────────────────────────
    pub const PAYMENTS_READ:     &str = "payments:read";
    pub const PAYMENTS_RECONCILE:&str = "payments:reconcile";
    pub const PAYMENTS_EXPORT:   &str = "payments:export";
    // Aliases used by the payments service HTTP handlers
    pub const BILLING_VIEW:      &str = "payments:read";
    pub const BILLING_MANAGE:    &str = "payments:reconcile";
    /// Elevated billing admin — approve/disburse/reject withdrawal requests.
    pub const BILLING_ADMIN:     &str = "payments:admin";
    // Narrow self-scoped permission: a driver may read their own day's COD
    // summary (end-of-shift cash bag) but nothing else billing-related.
    pub const DRIVER_COD_VIEW:   &str = "payments:cod-read-own";

    // ── Analytics ────────────────────────────────────────────
    pub const ANALYTICS_VIEW:    &str = "analytics:view";
    pub const ANALYTICS_EXPORT:  &str = "analytics:export";

    // ── Marketing ────────────────────────────────────────────
    pub const CAMPAIGNS_CREATE:  &str = "campaigns:create";
    pub const CAMPAIGNS_SEND:    &str = "campaigns:send";

    // ── Users / Tenants (admin) ───────────────────────────────
    pub const USERS_INVITE:      &str = "users:invite";
    pub const USERS_MANAGE:      &str = "users:manage";
    pub const TENANT_MANAGE:     &str = "tenants:manage";
    pub const API_KEYS_MANAGE:   &str = "api_keys:manage";

    // ── Onboarding (draft-tenant scope) ──────────────────────
    // Narrow permissions minted by AuthService::exchange_firebase for draft
    // tenants. These are the ONLY permissions a draft-tenant JWT receives;
    // `finalize_self` promotes the tenant to active, after which the owner
    // gets the full `merchant` permission set on next refresh.
    pub const TENANT_UPDATE_SELF: &str = "tenants:update-self";
    pub const BILLING_SETUP:      &str = "billing:setup";

    // ── Carriers ─────────────────────────────────────────────
    /// Tenant-wide carrier authority: onboard, edit, activate/suspend, and
    /// mint API keys for **any** carrier in the tenant. Admin/tenant_admin only.
    pub const CARRIERS_MANAGE:   &str = "carriers:manage";
    // Narrow self-scoped permission: a partner may edit the profile, rate
    // cards and API key of the single carrier record whose `contact_email`
    // matches their JWT email — and nothing else carrier-related. It does NOT
    // grant onboarding new carriers, lifecycle transitions (activate/suspend),
    // or compliance/KYB overrides; those stay with CARRIERS_MANAGE so the
    // tenant retains its lever over the partner. See ADR-0013.
    pub const CARRIERS_MANAGE_OWN: &str = "carriers:manage-own";
    pub const CARRIERS_READ:     &str = "carriers:read";

    // ── Customers / CDP ───────────────────────────────────────
    pub const CUSTOMERS_VIEW:    &str = "customers:read";
    pub const CUSTOMERS_MANAGE:  &str = "customers:manage";

    // ── Segments ─────────────────────────────────────────────
    pub const SEGMENTS_VIEW:     &str = "segments:read";
    pub const SEGMENTS_MANAGE:   &str = "segments:manage";

    // ── Compliance ───────────────────────────────────────────
    pub const COMPLIANCE_REVIEW: &str = "compliance:review";
    pub const COMPLIANCE_ADMIN:  &str = "compliance:admin";

    // ── Webhooks ─────────────────────────────────────────────
    pub const WEBHOOKS_READ:     &str = "webhooks:read";
    pub const WEBHOOKS_MANAGE:   &str = "webhooks:manage";
}

/// Predefined role → permissions mappings applied at tenant setup.
/// Each role is additive; a user can hold multiple roles.
pub fn default_permissions_for_role(role: &str) -> Vec<&'static str> {
    match role {
        // Full access within the tenant (not cross-tenant)
        "admin" => vec![
            permissions::SHIPMENT_CREATE, permissions::SHIPMENT_READ,
            permissions::SHIPMENT_UPDATE, permissions::SHIPMENT_CANCEL,
            permissions::SHIPMENT_BULK, permissions::SHIPMENT_EXPORT,
            permissions::DISPATCH_ASSIGN, permissions::DISPATCH_REROUTE, permissions::DISPATCH_VIEW,
            permissions::DRIVER_CREATE, permissions::DRIVER_READ, permissions::DRIVER_MANAGE,
            permissions::FLEET_READ, permissions::FLEET_MANAGE,
            permissions::PAYMENTS_READ, permissions::PAYMENTS_RECONCILE, permissions::PAYMENTS_EXPORT,
            permissions::BILLING_ADMIN,
            permissions::ANALYTICS_VIEW, permissions::ANALYTICS_EXPORT,
            permissions::CAMPAIGNS_CREATE, permissions::CAMPAIGNS_SEND,
            permissions::USERS_INVITE, permissions::USERS_MANAGE,
            permissions::API_KEYS_MANAGE,
            permissions::CARRIERS_MANAGE, permissions::CARRIERS_READ,
            permissions::CUSTOMERS_VIEW, permissions::CUSTOMERS_MANAGE,
            permissions::SEGMENTS_VIEW, permissions::SEGMENTS_MANAGE,
            permissions::COMPLIANCE_REVIEW, permissions::COMPLIANCE_ADMIN,
            permissions::WEBHOOKS_READ, permissions::WEBHOOKS_MANAGE,
        ],
        "dispatcher" => vec![
            permissions::SHIPMENT_READ, permissions::SHIPMENT_UPDATE,
            permissions::DISPATCH_ASSIGN, permissions::DISPATCH_REROUTE, permissions::DISPATCH_VIEW,
            permissions::DRIVER_READ, permissions::FLEET_READ,
        ],
        "merchant" => vec![
            permissions::SHIPMENT_CREATE, permissions::SHIPMENT_READ,
            permissions::SHIPMENT_CANCEL, permissions::SHIPMENT_BULK,
            permissions::ANALYTICS_VIEW,
            permissions::CUSTOMERS_VIEW,
            permissions::SEGMENTS_VIEW,
        ],
        "driver" => vec![
            permissions::SHIPMENT_READ,
            permissions::DISPATCH_VIEW,
            permissions::DRIVER_COD_VIEW,
        ],
        "finance" => vec![
            permissions::PAYMENTS_READ, permissions::PAYMENTS_RECONCILE,
            permissions::PAYMENTS_EXPORT, permissions::BILLING_ADMIN,
            permissions::ANALYTICS_VIEW,
        ],
        "readonly" => vec![
            permissions::SHIPMENT_READ, permissions::DISPATCH_VIEW,
            permissions::ANALYTICS_VIEW,
        ],
        // End-customer: can book and track their own shipments
        "customer" => vec![
            permissions::SHIPMENT_CREATE, permissions::SHIPMENT_READ,
            permissions::SHIPMENT_CANCEL,
        ],
        // A partner is a carrier's own operator. It gets self-scoped carrier
        // authority only — CARRIERS_MANAGE would let one partner edit, suspend
        // or mint an API key for a competing carrier in the same tenant.
        "partner" => vec![
            permissions::CARRIERS_READ,
            permissions::CARRIERS_MANAGE_OWN,
            permissions::SHIPMENT_READ,
            permissions::PAYMENTS_READ,
            permissions::ANALYTICS_VIEW,
        ],
        "tenant_admin" => vec![
            permissions::SHIPMENT_CREATE, permissions::SHIPMENT_READ,
            permissions::SHIPMENT_UPDATE, permissions::SHIPMENT_CANCEL,
            permissions::SHIPMENT_BULK, permissions::SHIPMENT_EXPORT,
            permissions::DISPATCH_ASSIGN, permissions::DISPATCH_REROUTE, permissions::DISPATCH_VIEW,
            permissions::DRIVER_CREATE, permissions::DRIVER_READ, permissions::DRIVER_MANAGE,
            permissions::FLEET_READ, permissions::FLEET_MANAGE,
            permissions::PAYMENTS_READ, permissions::PAYMENTS_RECONCILE, permissions::PAYMENTS_EXPORT,
            permissions::BILLING_ADMIN,
            permissions::ANALYTICS_VIEW, permissions::ANALYTICS_EXPORT,
            permissions::CAMPAIGNS_CREATE, permissions::CAMPAIGNS_SEND,
            permissions::USERS_INVITE, permissions::USERS_MANAGE,
            permissions::API_KEYS_MANAGE,
            permissions::CARRIERS_MANAGE, permissions::CARRIERS_READ,
            permissions::CUSTOMERS_VIEW, permissions::CUSTOMERS_MANAGE,
            permissions::SEGMENTS_VIEW, permissions::SEGMENTS_MANAGE,
            permissions::COMPLIANCE_REVIEW, permissions::COMPLIANCE_ADMIN,
            permissions::WEBHOOKS_READ, permissions::WEBHOOKS_MANAGE,
        ],
        "hub_scanner" => vec![
            permissions::SHIPMENT_READ,
            permissions::SHIPMENT_UPDATE,
        ],
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn role(r: &str) -> Vec<&'static str> {
        default_permissions_for_role(r)
    }

    /// The partner role is a carrier's own operator. Granting it tenant-wide
    /// `carriers:manage` would let one partner edit, suspend, or mint an API
    /// key for a competing carrier in the same tenant — see ADR-0013 and
    /// `services/carrier/src/application/authz.rs`.
    #[test]
    fn partner_does_not_hold_tenant_wide_carrier_authority() {
        let p = role("partner");
        assert!(
            !p.contains(&permissions::CARRIERS_MANAGE),
            "partner must not hold carriers:manage — use carriers:manage-own",
        );
        assert!(p.contains(&permissions::CARRIERS_MANAGE_OWN));
        assert!(p.contains(&permissions::CARRIERS_READ));
    }

    /// The narrow permission is partner-only; handing it to an operator role
    /// would be harmless but signals confusion about which is which.
    #[test]
    fn manage_own_is_granted_to_partner_alone() {
        for r in ["admin", "tenant_admin", "dispatcher", "merchant", "driver",
                  "finance", "readonly", "customer", "hub_scanner"] {
            assert!(
                !role(r).contains(&permissions::CARRIERS_MANAGE_OWN),
                "{r} should not hold carriers:manage-own",
            );
        }
    }

    /// Tenant operators keep full carrier authority — the partner change must
    /// not have narrowed the admin path.
    #[test]
    fn operator_roles_retain_carrier_manage() {
        for r in ["admin", "tenant_admin"] {
            assert!(role(r).contains(&permissions::CARRIERS_MANAGE), "{r} lost carriers:manage");
        }
    }

    /// The two permission strings must stay distinct — a substring-matching
    /// check anywhere would otherwise silently conflate them.
    #[test]
    fn carrier_permission_strings_are_distinct() {
        assert_ne!(permissions::CARRIERS_MANAGE, permissions::CARRIERS_MANAGE_OWN);
    }

    #[test]
    fn unknown_role_has_no_permissions() {
        assert!(role("undefined_role").is_empty());
    }
}
