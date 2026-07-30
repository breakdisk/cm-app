//! Reverse proxy layer: forwards validated requests to downstream services.
//! Uses reqwest for HTTP proxying. In production this is replaced/augmented by Envoy sidecar.

use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, Uri},
    response::Response,
};
use std::sync::Arc;
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
