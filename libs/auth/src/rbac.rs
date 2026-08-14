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
    // The end-customer equivalent: their own receipts, nothing else. Not
    // BILLING_VIEW -- that is tenant-wide and also gates the wallet summary,
    // the COD batches and the partner commission ledger.
    pub const BILLING_READ_OWN:  &str = "payments:read-own";

    // ── Analytics ────────────────────────────────────────────
    pub const ANALYTICS_VIEW:    &str = "analytics:view";
    pub const ANALYTICS_EXPORT:  &str = "analytics:export";

    // ── Marketing ────────────────────────────────────────────
    pub const CAMPAIGNS_CREATE:  &str = "campaigns:create";
    pub const CAMPAIGNS_SEND:    &str = "campaigns:send";

    // ── Engagement (notifications, templates) ─────────────────
    // The engagement service used to declare these strings privately, so they
    // matched no entry below and every one of its endpoints answered 403 to
    // every caller, including admins. It now imports these constants, which
    // makes the link a compile error rather than a silent mismatch.
    pub const ENGAGEMENT_READ:   &str = "engagement:read";
    pub const ENGAGEMENT_SEND:   &str = "engagement:send";
    pub const ENGAGEMENT_TEMPLATES_WRITE: &str = "engagement:templates:write";
    // Self-scoped, like DRIVER_COD_VIEW above: an end customer reads their own
    // notification history and nothing else. ENGAGEMENT_READ is tenant-wide —
    // granting it to the customer role would expose every customer's messages.
    pub const ENGAGEMENT_READ_OWN: &str = "engagement:read-own";

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
    //
    // TENANT_UPDATE_SELF is also the standing permission for a tenant admin
    // editing their *own* tenant: profile fields and white-label branding.
    // It is deliberately not TENANT_MANAGE, which is platform-scoped -- that
    // one gates `PUT /v1/pricing/features/:key/tiers`, which takes no tenant
    // id and rewrites the pricing matrix for every tenant on the platform,
    // and `PUT /v1/tenants/:id/tier`, which would be a free self-upgrade to
    // Enterprise. Platform admins reach those through the `*` wildcard; no
    // role grants TENANT_MANAGE, and that is intentional.
    pub const TENANT_UPDATE_SELF: &str = "tenants:update-self";
    pub const BILLING_SETUP:      &str = "billing:setup";

    // ── OmniDeliv vendors ────────────────────────────────────
    // Approving a store is the review that stands between "anyone with a
    // login" and "food listed to customers", so it is an operator action.
    // Until this existed the approve route took `_claims` and checked
    // nothing -- an applicant could approve their own application.
    pub const VENDORS_MANAGE:    &str = "vendors:manage";

    // ── Carriers ─────────────────────────────────────────────
    pub const CARRIERS_MANAGE:   &str = "carriers:manage";
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
            permissions::ENGAGEMENT_READ, permissions::ENGAGEMENT_SEND,
            permissions::ENGAGEMENT_TEMPLATES_WRITE,
            permissions::USERS_INVITE, permissions::USERS_MANAGE,
            permissions::TENANT_UPDATE_SELF,
            permissions::API_KEYS_MANAGE,
            permissions::VENDORS_MANAGE,
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
            permissions::ENGAGEMENT_READ,
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
            // Self-scoped only. The customer app's notification screen reads
            // its own history; the handler pins customer_id to the token.
            permissions::ENGAGEMENT_READ_OWN,
            // Likewise the Invoices and Receipt screens.
            permissions::BILLING_READ_OWN,
        ],
        "partner" => vec![
            permissions::CARRIERS_READ,
            permissions::CARRIERS_MANAGE,
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
            permissions::ENGAGEMENT_READ, permissions::ENGAGEMENT_SEND,
            permissions::ENGAGEMENT_TEMPLATES_WRITE,
            permissions::USERS_INVITE, permissions::USERS_MANAGE,
            permissions::TENANT_UPDATE_SELF,
            permissions::API_KEYS_MANAGE,
            permissions::VENDORS_MANAGE,
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
mod grant_tests {
    use super::*;

    fn perms(role: &str) -> Vec<&'static str> { default_permissions_for_role(role) }

    /// The invariant behind the split. TENANT_MANAGE gates
    /// `PUT /v1/pricing/features/:key/tiers`, which takes no tenant id and
    /// rewrites the pricing matrix for the whole platform, and the tier
    /// upgrade, which would be a free jump to Enterprise. Both endpoints
    /// currently answer 403 to every role, and that is the correct state --
    /// granting this to "fix" the 403 is the mistake this test exists to stop.
    /// Platform admins hold the `*` wildcard instead.
    #[test]
    fn no_role_may_hold_tenant_manage() {
        for role in ["admin", "tenant_admin", "merchant", "dispatcher", "driver",
                     "finance", "readonly", "customer", "partner", "hub_scanner"] {
            assert!(
                !perms(role).contains(&permissions::TENANT_MANAGE),
                "{role} must not hold {} -- it is platform-scoped",
                permissions::TENANT_MANAGE,
            );
        }
    }

    /// Tenant admins edit their own tenant and their own branding. Without
    /// this the white-label feature is unreachable by anyone.
    #[test]
    fn tenant_admins_can_edit_their_own_tenant() {
        for role in ["admin", "tenant_admin"] {
            assert!(perms(role).contains(&permissions::TENANT_UPDATE_SELF), "{role}");
        }
    }

    /// Every engagement endpoint was gated on strings no role held.
    #[test]
    fn staff_roles_can_reach_the_engagement_api() {
        for role in ["admin", "tenant_admin"] {
            let p = perms(role);
            assert!(p.contains(&permissions::ENGAGEMENT_READ), "{role} read");
            assert!(p.contains(&permissions::ENGAGEMENT_SEND), "{role} send");
            assert!(p.contains(&permissions::ENGAGEMENT_TEMPLATES_WRITE), "{role} templates");
        }
        assert!(perms("merchant").contains(&permissions::ENGAGEMENT_READ));
    }

    /// The customer app reads its own notification history -- and only that.
    /// ENGAGEMENT_READ is tenant-wide: holding it would let any end customer
    /// list every message the tenant ever sent to anyone.
    #[test]
    fn a_customer_gets_self_scoped_read_and_not_tenant_wide_read() {
        let p = perms("customer");
        assert!(p.contains(&permissions::ENGAGEMENT_READ_OWN), "needs own-read");
        assert!(!p.contains(&permissions::ENGAGEMENT_READ), "must NOT be tenant-wide");
    }

    /// An unknown role is not a free pass.
    /// Same rule as the engagement pair: the customer app's billing screens
    /// need their own receipts, and BILLING_VIEW is tenant-wide -- it also
    /// gates the wallet summary, COD batches and the partner commission
    /// ledger, none of which belong to an end customer.
    #[test]
    fn a_customer_gets_self_scoped_billing_and_not_tenant_wide_billing() {
        let p = perms("customer");
        assert!(p.contains(&permissions::BILLING_READ_OWN), "needs own-read");
        assert!(!p.contains(&permissions::BILLING_VIEW), "must NOT be tenant-wide");
        assert!(!p.contains(&permissions::PAYMENTS_READ), "same string as BILLING_VIEW");
    }

    /// `POST /v1/wallet/withdraw` reserves against the **tenant's** settlement
    /// wallet -- `TransactionType::Withdrawal` is commented "Merchant bank
    /// transfer" -- and is gated on BILLING_MANAGE. The customer app shipped a
    /// Wallet screen with a "Request Withdrawal" button pointed straight at it.
    /// The only thing between an end customer and the merchant's money was this
    /// grant being absent.
    ///
    /// The screen is gone now, but the incentive that made it dangerous is not:
    /// a broken customer-facing screen invites someone to "fix" the 403 by
    /// granting the permission. That is the mistake this test exists to stop.
    #[test]
    fn a_customer_can_never_reach_the_tenant_wallet() {
        let p = perms("customer");
        assert!(!p.contains(&permissions::BILLING_MANAGE), "withdraws tenant funds");
        assert!(!p.contains(&permissions::PAYMENTS_RECONCILE), "same string as BILLING_MANAGE");
        assert!(!p.contains(&permissions::BILLING_ADMIN), "approves withdrawals");
    }

    /// Approving a vendor is what stands between "anyone with a login" and
    /// "a store listed to customers". The route checked nothing at all until
    /// 2026-08-14, so an applicant could approve their own application; the
    /// permission is useless if no operator role can hold it.
    #[test]
    fn operators_can_approve_vendors_and_merchants_cannot() {
        for role in ["admin", "tenant_admin"] {
            assert!(perms(role).contains(&permissions::VENDORS_MANAGE), "{role}");
        }
        // A merchant applies; they do not review their own application.
        assert!(!perms("merchant").contains(&permissions::VENDORS_MANAGE));
        assert!(!perms("customer").contains(&permissions::VENDORS_MANAGE));
    }

    #[test]
    fn an_unknown_role_gets_nothing() {
        assert!(perms("not_a_role").is_empty());
    }
}
