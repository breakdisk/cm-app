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
        // Identity & Auth
        if path.starts_with("/v1/auth") || path.starts_with("/v1/users") || path.starts_with("/v1/tenants") || path.starts_with("/v1/api-keys") {
            Some(&self.services.identity_url)
        // Order & Shipment Intake
        } else if path.starts_with("/v1/shipments") || path.starts_with("/v1/orders") {
            Some(&self.services.order_intake_url)
        // Dispatch & Routing — dispatch service exposes /v1/routes, /v1/queue, /v1/assignments
        } else if path.starts_with("/v1/routes")
            || path.starts_with("/v1/queue")
            || path.starts_with("/v1/assignments")
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
        // Hub Operations
        } else if path.starts_with("/v1/hubs") {
            Some(&self.services.hub_ops_url)
        // Carrier Management
        } else if path.starts_with("/v1/carriers") {
            Some(&self.services.carrier_url)
        // Proof of Delivery (includes /otps for recipient OTP verification)
        } else if path.starts_with("/v1/pod") || path.starts_with("/v1/otps") {
            Some(&self.services.pod_url)
        // Payments & Invoices (includes /wallet for customer app)
        } else if path.starts_with("/v1/payments") || path.starts_with("/v1/invoices") || path.starts_with("/v1/wallet") || path.starts_with("/v1/cod") {
            Some(&self.services.payments_url)
        // Analytics
        } else if path.starts_with("/v1/analytics") {
            Some(&self.services.analytics_url)
        // Marketing Automation
        } else if path.starts_with("/v1/campaigns") || path.starts_with("/v1/segments") {
            Some(&self.services.marketing_url)
        // Customer Data Platform
        } else if path.starts_with("/v1/customers") || path.starts_with("/v1/profiles") {
            Some(&self.services.cdp_url)
        // Engagement & Notifications
        } else if path.starts_with("/v1/notifications") || path.starts_with("/v1/engagement") {
            Some(&self.services.engagement_url)
        // AI Intelligence Layer
        } else if path.starts_with("/v1/ai") || path.starts_with("/v1/agents") {
            Some(&self.services.ai_layer_url)
        } else {
            None
        }
    }
}
