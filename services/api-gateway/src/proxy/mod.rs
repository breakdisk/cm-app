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
        if path.starts_with("/v1/auth") || path.starts_with("/v1/users") || path.starts_with("/v1/tenants") || path.starts_with("/v1/api-keys") {
            Some(&self.services.identity_url)
        } else if path.starts_with("/v1/shipments") || path.starts_with("/v1/orders") {
            Some(&self.services.order_intake_url)
        } else if path.starts_with("/v1/dispatch") || path.starts_with("/v1/routes") {
            Some(&self.services.dispatch_url)
        } else if path.starts_with("/v1/drivers") {
            Some(&self.services.driver_ops_url)
        } else if path.starts_with("/v1/tracking") || path.starts_with("/v1/delivery") {
            Some(&self.services.delivery_experience_url)
        } else if path.starts_with("/v1/fleet") || path.starts_with("/v1/vehicles") {
            Some(&self.services.fleet_url)
        } else if path.starts_with("/v1/hubs") {
            Some(&self.services.hub_ops_url)
        } else if path.starts_with("/v1/carriers") {
            Some(&self.services.carrier_url)
        } else if path.starts_with("/v1/pod") {
            Some(&self.services.pod_url)
        } else if path.starts_with("/v1/payments") || path.starts_with("/v1/invoices") {
            Some(&self.services.payments_url)
        } else if path.starts_with("/v1/analytics") {
            Some(&self.services.analytics_url)
        } else if path.starts_with("/v1/campaigns") || path.starts_with("/v1/segments") {
            Some(&self.services.marketing_url)
        } else if path.starts_with("/v1/customers") || path.starts_with("/v1/profiles") {
            Some(&self.services.cdp_url)
        } else if path.starts_with("/v1/notifications") || path.starts_with("/v1/engagement") {
            Some(&self.services.engagement_url)
        } else if path.starts_with("/v1/ai") || path.starts_with("/v1/agents") {
            Some(&self.services.ai_layer_url)
        } else {
            None
        }
    }
}
