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

    // ── Carriers ─────────────────────────────────────────────
    pub const CARRIERS_MANAGE:   &str = "carriers:manage";
    pub const CARRIERS_READ:     &str = "carriers:read";

    // ── Customers / CDP ───────────────────────────────────────
    pub const CUSTOMERS_VIEW:    &str = "customers:read";
    pub const CUSTOMERS_MANAGE:  &str = "customers:manage";

    // ── Compliance ───────────────────────────────────────────
    pub const COMPLIANCE_REVIEW: &str = "compliance:review";
    pub const COMPLIANCE_ADMIN:  &str = "compliance:admin";
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
            permissions::ANALYTICS_VIEW, permissions::ANALYTICS_EXPORT,
            permissions::CAMPAIGNS_CREATE, permissions::CAMPAIGNS_SEND,
            permissions::USERS_INVITE, permissions::USERS_MANAGE,
            permissions::API_KEYS_MANAGE,
            permissions::CARRIERS_MANAGE, permissions::CARRIERS_READ,
            permissions::CUSTOMERS_VIEW, permissions::CUSTOMERS_MANAGE,
            permissions::COMPLIANCE_REVIEW, permissions::COMPLIANCE_ADMIN,
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
        ],
        "driver" => vec![
            permissions::SHIPMENT_READ,
            permissions::DISPATCH_VIEW,
        ],
        "finance" => vec![
            permissions::PAYMENTS_READ, permissions::PAYMENTS_RECONCILE,
            permissions::PAYMENTS_EXPORT, permissions::ANALYTICS_VIEW,
        ],
        "readonly" => vec![
            permissions::SHIPMENT_READ, permissions::DISPATCH_VIEW,
            permissions::ANALYTICS_VIEW,
        ],
        _ => vec![],
    }
}
