//! Reverse proxy layer: forwards validated requests to downstream services.
//! Uses reqwest for HTTP proxying. In production this is replaced/augmented by Envoy sidecar.

use crate::config::ServicesConfig;

pub struct ProxyClient {
    pub client: reqwest::Client,
    pub services: ServicesConfig,
}

impl ProxyClient {
    pub fn new(services: ServicesConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .pool_max_idle_per_host(20)
            .build()
            .expect("Failed to build HTTP client");
        Self { client, services }
    }

    /// Resolve the base URL for a service given the request path prefix.
    pub fn resolve_upstream(&self, path: &str) -> Option<&str> {
        // Internal-only routes (e.g. Firebase → LogisticOS JWT exchange) must
        // never be reachable from the public gateway; reject them here as a
        // defence-in-depth measure on top of identity's internal-secret guard.
        if path.starts_with("/v1/internal") || path.contains("/internal/") {
            return None;
        }
        // Tier-prefixed routes are matched first and deliberately so.
        //
        // This resolver is a first-match-wins chain over a single flat `/v1`
        // namespace, which works only while one team owns every resource name.
        // It stopped being true with the second product: field-ops serves
        // assignments, dispatch already owns `/v1/assignments`, and OmniDeliv
        // will serve orders against an `/v1/orders` that resolves to
        // order-intake. Rather than arbitrate collisions by branch order —
        // which silently re-breaks whenever a branch is added above another —
        // every non-LogisticOS tier carries its own prefix and is resolved
        // before the flat names below. Adding a product means adding a prefix
        // here, not auditing twenty existing branches.
        //
        // Both are optional: an environment that has not deployed them yet gets
        // a 503 for these paths instead of a gateway that fails to boot.
        if path.starts_with("/v1/field-ops") {
            return self.services.field_ops_url.as_deref();
        }
        if path.starts_with("/v1/omnideliv") {
            return self.services.omnideliv_url.as_deref();
        }
        // Identity & Auth
        if path.starts_with("/v1/auth") || path.starts_with("/v1/users") || path.starts_with("/v1/tenants") || path.starts_with("/v1/api-keys") || path.starts_with("/v1/audit-log") || path.starts_with("/v1/push-tokens") {
            Some(&self.services.identity_url)
        // Order & Shipment Intake
        } else if path.starts_with("/v1/shipments") || path.starts_with("/v1/orders") {
            Some(&self.services.order_intake_url)
        // Dispatch & Routing — dispatch service exposes /v1/routes, /v1/queue,
        // /v1/assignments, and /v1/offers (gig grab surface)
        } else if path.starts_with("/v1/routes")
            || path.starts_with("/v1/queue")
            || path.starts_with("/v1/assignments")
            || path.starts_with("/v1/offers")
        {
            Some(&self.services.dispatch_url)
        // Driver Operations (includes /tasks and /location from driver app)
        } else if path.starts_with("/v1/drivers") || path.starts_with("/v1/tasks") || path.starts_with("/v1/location") {
            Some(&self.services.driver_ops_url)
        // Delivery Experience & Tracking
        } else if path.starts_with("/v1/tracking") || path.starts_with("/v1/delivery") {
            Some(&self.services.delivery_experience_url)
        // Fleet Management
        } else if path.starts_with("/v1/fleet") || path.starts_with("/v1/vehicles") {
            Some(&self.services.fleet_url)
        // Hub Operations (hubs, consolidation plans/specs, container/pallet management)
        } else if path.starts_with("/v1/hubs")
            || path.starts_with("/v1/consolidation")
            || path.starts_with("/v1/containers")
            || path.starts_with("/v1/pallets")
        {
            Some(&self.services.hub_ops_url)
        // Carrier Management + Marketplace listings/bookings
        } else if path.starts_with("/v1/carriers") || path.starts_with("/v1/marketplace") {
            Some(&self.services.carrier_url)
        // Proof of Delivery + Proof of Pickup (includes /otps for recipient OTP verification)
        } else if path.starts_with("/v1/pod") || path.starts_with("/v1/pops") || path.starts_with("/v1/otps") {
            Some(&self.services.pod_url)
        // Payments & Invoices (includes /wallet for customer app)
        } else if path.starts_with("/v1/payments") || path.starts_with("/v1/invoices") || path.starts_with("/v1/wallet") || path.starts_with("/v1/cod") {
            Some(&self.services.payments_url)
        // Analytics
        } else if path.starts_with("/v1/analytics") {
            Some(&self.services.analytics_url)
        // Marketing Automation + Journey Builder
        } else if path.starts_with("/v1/campaigns") || path.starts_with("/v1/journeys") {
            Some(&self.services.marketing_url)
        // Customer communication history — engagement service owns /sends
        // Must be checked before the generic /v1/customers → CDP rule.
        } else if path.starts_with("/v1/customers/") && path.ends_with("/sends") {
            Some(&self.services.engagement_url)
        // Customer Data Platform — customers + segment CRUD + segment membership
        } else if path.starts_with("/v1/customers") || path.starts_with("/v1/segments") || path.starts_with("/v1/profiles") {
            Some(&self.services.cdp_url)
        // Engagement & Notifications (templates are managed by the engagement service)
        } else if path.starts_with("/v1/notifications") || path.starts_with("/v1/engagement") || path.starts_with("/v1/templates") {
            Some(&self.services.engagement_url)
        // AI Intelligence Layer (including the remote MCP transport at /mcp —
        // Enterprise Extension per ADR-0004)
        } else if path.starts_with("/v1/ai") || path.starts_with("/v1/agents") || path.starts_with("/mcp") {
            Some(&self.services.ai_layer_url)
        // Business Logic / Automation Rules
        } else if path.starts_with("/v1/rules") {
            Some(&self.services.business_logic_url)
        // Compliance (note: uses /api/v1/compliance/* prefix, not /v1/)
        } else if path.starts_with("/api/v1/compliance") {
            self.services.compliance_url.as_deref()
        // Webhooks management (admin portal Settings → Webhooks tab)
        } else if path.starts_with("/v1/webhooks") {
            self.services.webhooks_url.as_deref()
        // E-commerce connectors (Shopify, WooCommerce, etc.)
        } else if path.starts_with("/v1/connectors") {
            self.services.connectors_url.as_deref()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod routing_tests {
    use super::*;

    // Deliberately inline rather than in `tests/`: this crate's `tests/unit/`
    // and `tests/integration/` are `mod.rs` files with no top-level `.rs`
    // sibling, so cargo never builds them as test targets. A guard placed
    // there would compile-check as part of nothing and report nothing.
    fn services() -> ServicesConfig {
        ServicesConfig {
            identity_url:            "http://identity:8001".into(),
            cdp_url:                 "http://cdp:8002".into(),
            engagement_url:          "http://engagement:8003".into(),
            order_intake_url:        "http://order-intake:8004".into(),
            dispatch_url:            "http://dispatch:8005".into(),
            driver_ops_url:          "http://driver-ops:8006".into(),
            delivery_experience_url: "http://delivery-experience:8007".into(),
            fleet_url:               "http://fleet:8008".into(),
            hub_ops_url:             "http://hub-ops:8009".into(),
            carrier_url:             "http://carrier:8010".into(),
            pod_url:                 "http://pod:8011".into(),
            payments_url:            "http://payments:8012".into(),
            analytics_url:           "http://analytics:8013".into(),
            marketing_url:           "http://marketing:8014".into(),
            business_logic_url:      "http://business-logic:8015".into(),
            ai_layer_url:            "http://ai-layer:8016".into(),
            compliance_url:          None,
            webhooks_url:            None,
            connectors_url:          None,
            field_ops_url:           Some("http://field-ops:8090".into()),
            omnideliv_url:           Some("http://omnideliv:8091".into()),
        }
    }

    fn resolve(path: &str) -> Option<String> {
        ProxyClient::new(services())
            .resolve_upstream(path)
            .map(str::to_owned)
    }

    /// The collision this prefix exists to prevent. Unprefixed, every one of
    /// these paths starts with `/v1/assignments` and resolves to dispatch.
    #[test]
    fn field_ops_assignment_routes_do_not_resolve_to_dispatch() {
        for path in [
            "/v1/field-ops/assignments/offer",
            "/v1/field-ops/assignments/1e9f/claim",
            "/v1/field-ops/couriers/1e9f/position",
        ] {
            assert_eq!(
                resolve(path).as_deref(),
                Some("http://field-ops:8090"),
                "{path} must reach field-ops"
            );
        }
    }

    /// Dispatch keeps `/v1/assignments`. The driver app calls
    /// `PUT /v1/assignments/:id/accept` in production; if this flips, shipment
    /// accept/reject breaks in the field.
    #[test]
    fn dispatch_keeps_the_unprefixed_assignment_routes() {
        for path in [
            "/v1/assignments/1e9f/accept",
            "/v1/assignments/1e9f/reject",
            "/v1/offers",
            "/v1/routes",
            "/v1/queue",
        ] {
            assert_eq!(
                resolve(path).as_deref(),
                Some("http://dispatch:8005"),
                "{path} must stay on dispatch"
            );
        }
    }

    /// The dangerous one: an unprefixed `POST /v1/orders` reaches order-intake
    /// and *succeeds*, creating a real shipment, rather than 404-ing the way a
    /// wrong-service GET would.
    #[test]
    fn omnideliv_orders_never_reach_order_intake() {
        assert_eq!(
            resolve("/v1/omnideliv/orders").as_deref(),
            Some("http://omnideliv:8091")
        );
        assert_eq!(
            resolve("/v1/orders").as_deref(),
            Some("http://order-intake:8004"),
            "LogisticOS order intake must keep the flat name"
        );
        assert_eq!(
            resolve("/v1/shipments").as_deref(),
            Some("http://order-intake:8004")
        );
    }

    #[test]
    fn omnideliv_product_routes_resolve_to_omnideliv() {
        for path in [
            "/v1/omnideliv/baskets",
            "/v1/omnideliv/baskets/1e9f",
            "/v1/omnideliv/catalog/search",
        ] {
            assert_eq!(
                resolve(path).as_deref(),
                Some("http://omnideliv:8091"),
                "{path} must reach omnideliv"
            );
        }
    }

    /// An environment that has not deployed the tier yet must 503, not fall
    /// through to whichever flat-name branch happens to match.
    #[test]
    fn unconfigured_tiers_resolve_to_nothing() {
        let mut svc = services();
        svc.field_ops_url = None;
        svc.omnideliv_url = None;
        let proxy = ProxyClient::new(svc);
        assert_eq!(proxy.resolve_upstream("/v1/field-ops/assignments/offer"), None);
        assert_eq!(proxy.resolve_upstream("/v1/omnideliv/baskets"), None);
    }

    /// The internal-route guard runs before the tier prefixes and must stay
    /// that way — a tier prefix must not become a way to reach `/internal/`.
    #[test]
    fn internal_routes_stay_unreachable_through_a_tier_prefix() {
        assert_eq!(resolve("/v1/field-ops/internal/couriers"), None);
        assert_eq!(resolve("/v1/omnideliv/internal/orders"), None);
        assert_eq!(resolve("/v1/internal/auth/exchange"), None);
    }
}
