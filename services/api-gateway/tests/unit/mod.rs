//! Unit tests over the gateway's public surface: route resolution, tier
//! limits, the MCP registry, and config deserialisation.
//!
//! The inline `routing_tests` in `src/proxy/mod.rs` cover the collision-prone
//! prefixes; this file pins the rest of the table so a reordered `else if`
//! chain cannot silently hand a service's paths to another.

use logisticos_api_gateway::{
    config::{Config, ServicesConfig},
    proxy::{diner_may_reach, ProxyClient},
    ratelimit::limit_for_tier,
    registry::{seed_platform_servers, McpRegistry, McpServerEntry, McpTool},
};
use uuid::Uuid;

fn required_services() -> serde_json::Value {
    serde_json::json!({
        "identity_url": "http://identity", "cdp_url": "http://cdp",
        "engagement_url": "http://engagement", "order_intake_url": "http://order-intake",
        "dispatch_url": "http://dispatch", "driver_ops_url": "http://driver-ops",
        "delivery_experience_url": "http://delivery-experience", "fleet_url": "http://fleet",
        "hub_ops_url": "http://hub-ops", "carrier_url": "http://carrier",
        "pod_url": "http://pod", "payments_url": "http://payments",
        "analytics_url": "http://analytics", "marketing_url": "http://marketing",
        "business_logic_url": "http://business-logic", "ai_layer_url": "http://ai-layer",
    })
}

fn services() -> ServicesConfig {
    let mut v = required_services();
    v["compliance_url"] = "http://compliance".into();
    v["webhooks_url"] = "http://webhooks".into();
    v["connectors_url"] = "http://connectors".into();
    serde_json::from_value(v).expect("ServicesConfig deserialises")
}

fn resolve(path: &str) -> Option<String> {
    ProxyClient::new(services()).resolve_upstream(path).map(str::to_owned)
}

#[test]
fn every_flat_prefix_reaches_its_owning_service() {
    let table = [
        ("/v1/auth/login", "http://identity"),
        ("/v1/users/me", "http://identity"),
        ("/v1/tenants/me", "http://identity"),
        ("/v1/api-keys", "http://identity"),
        ("/v1/audit-log", "http://identity"),
        ("/v1/push-tokens", "http://identity"),
        ("/v1/shipments/abc", "http://order-intake"),
        ("/v1/orders", "http://order-intake"),
        ("/v1/tasks", "http://driver-ops"),
        ("/v1/location", "http://driver-ops"),
        ("/v1/drivers/abc/status", "http://driver-ops"),
        ("/v1/tracking/public/CM-1", "http://delivery-experience"),
        ("/v1/delivery/feedback", "http://delivery-experience"),
        ("/v1/fleet/vehicles", "http://fleet"),
        ("/v1/vehicles", "http://fleet"),
        ("/v1/hubs", "http://hub-ops"),
        ("/v1/consolidation/plans", "http://hub-ops"),
        ("/v1/containers", "http://hub-ops"),
        ("/v1/pallets", "http://hub-ops"),
        ("/v1/carriers/me", "http://carrier"),
        ("/v1/marketplace/listings", "http://carrier"),
        ("/v1/pod/submit", "http://pod"),
        ("/v1/pops", "http://pod"),
        ("/v1/otps/verify", "http://pod"),
        ("/v1/payments/intents", "http://payments"),
        ("/v1/invoices", "http://payments"),
        ("/v1/wallet", "http://payments"),
        ("/v1/cod/remittances", "http://payments"),
        ("/v1/subscriptions/current", "http://payments"),
        ("/v1/analytics/metrics", "http://analytics"),
        ("/v1/campaigns", "http://marketing"),
        ("/v1/journeys", "http://marketing"),
        ("/v1/segments", "http://cdp"),
        ("/v1/profiles/abc", "http://cdp"),
        ("/v1/notifications", "http://engagement"),
        ("/v1/engagement/x", "http://engagement"),
        ("/v1/templates", "http://engagement"),
        ("/v1/ai/chat", "http://ai-layer"),
        ("/v1/agents/sessions", "http://ai-layer"),
        ("/mcp", "http://ai-layer"),
        ("/v1/rules", "http://business-logic"),
        ("/api/v1/compliance/documents", "http://compliance"),
        ("/v1/webhooks", "http://webhooks"),
        ("/v1/connectors/shopify/callback", "http://connectors"),
    ];
    for (path, want) in table {
        assert_eq!(resolve(path).as_deref(), Some(want), "{path}");
    }
}

#[test]
fn unknown_paths_resolve_to_nothing() {
    for path in ["/", "/v1", "/v2/shipments", "/shipments", "/v1/nope", "/health"] {
        assert_eq!(resolve(path), None, "{path} must have no upstream");
    }
}

#[test]
fn optional_services_resolve_to_nothing_when_unconfigured() {
    let mut svc = services();
    svc.compliance_url = None;
    svc.webhooks_url = None;
    svc.connectors_url = None;
    let proxy = ProxyClient::new(svc);
    assert_eq!(proxy.resolve_upstream("/api/v1/compliance/x"), None);
    assert_eq!(proxy.resolve_upstream("/v1/webhooks"), None);
    assert_eq!(proxy.resolve_upstream("/v1/connectors/x"), None);
    // ...while required services are unaffected.
    assert_eq!(proxy.resolve_upstream("/v1/users/me"), Some("http://identity"));
}

#[test]
fn internal_segments_are_refused_anywhere_in_the_path() {
    for path in [
        "/v1/internal/auth/exchange-firebase",
        "/v1/users/internal/x",
        "/v1/shipments/internal/backfill",
        "/v1/payments/internal/anything",
    ] {
        assert_eq!(resolve(path), None, "{path} must never be proxied");
    }
}

#[test]
fn diner_allowlist_is_method_and_path_scoped() {
    assert!(diner_may_reach("POST", "/v1/omnideliv/orders/checkout"));
    assert!(!diner_may_reach("GET", "/v1/omnideliv/orders/checkout"));
    assert!(!diner_may_reach("POST", "/v1/shipments"));
    assert!(!diner_may_reach("POST", "/v1/omnideliv/mesh/run"));
}

#[test]
fn tier_limits() {
    assert_eq!(limit_for_tier("starter"), 100);
    assert_eq!(limit_for_tier("growth"), 500);
    assert_eq!(limit_for_tier("business"), 2_000);
    assert_eq!(limit_for_tier("enterprise"), 10_000);
    // An unknown tier must get the lowest limit, never a higher one.
    assert_eq!(limit_for_tier("platinum"), 100);
    assert_eq!(limit_for_tier(""), 100);
    assert_eq!(limit_for_tier("ENTERPRISE"), 100, "tier match is case-sensitive");
}

fn entry(service: &str, active: bool) -> McpServerEntry {
    McpServerEntry {
        id: Uuid::new_v4(),
        name: format!("{service} MCP"),
        service: service.into(),
        endpoint: format!("https://{service}.example/mcp"),
        tools: vec![McpTool {
            name: format!("{service}_tool"),
            description: "d".into(),
            input_schema: serde_json::json!({"type": "object"}),
        }],
        tenant_id: None,
        is_active: active,
    }
}

#[test]
fn tenant_mcp_registration_requires_enterprise() {
    for tier in ["starter", "growth", "business", ""] {
        let mut reg = McpRegistry::new();
        assert!(
            reg.register_tenant_server(entry("acme", true), tier).is_err(),
            "{tier:?} must not register an external MCP server"
        );
        assert!(reg.all_tools().is_empty(), "a refused registration must leave no trace");
    }
    let mut reg = McpRegistry::new();
    assert!(reg.register_tenant_server(entry("acme", true), "enterprise").is_ok());
    assert_eq!(reg.all_tools().len(), 1);
}

#[test]
fn all_tools_hides_inactive_servers() {
    let mut reg = McpRegistry::new();
    reg.register_platform_server(entry("live", true));
    reg.register_platform_server(entry("retired", false));
    let tools = reg.all_tools();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].0, "live");
    assert!(reg.get("retired").is_some(), "inactive servers stay addressable by name");
}

#[test]
fn registering_a_platform_server_twice_replaces_it() {
    let mut reg = McpRegistry::new();
    reg.register_platform_server(entry("dispatch", true));
    let mut second = entry("dispatch", true);
    second.endpoint = "https://new.example/mcp".into();
    reg.register_platform_server(second);
    assert_eq!(reg.all_tools().len(), 1);
    assert_eq!(reg.get("dispatch").map(|e| e.endpoint.as_str()), Some("https://new.example/mcp"));
}

#[test]
fn seeding_points_each_server_at_its_configured_service() {
    let svc = services();
    let mut reg = McpRegistry::new();
    seed_platform_servers(&mut reg, &svc);

    let dispatch = reg.get("dispatch").expect("dispatch MCP seeded");
    assert_eq!(dispatch.endpoint, "http://dispatch/mcp");
    assert!(dispatch.tenant_id.is_none(), "seeded servers are platform-level");
    for name in ["assign_driver", "optimize_route", "get_available_drivers"] {
        assert!(dispatch.tools.iter().any(|t| t.name == name), "dispatch missing {name}");
    }
    assert_eq!(
        reg.get("order-intake").map(|e| e.endpoint.as_str()),
        Some("http://order-intake/mcp")
    );

    // Every tool a client can discover carries a JSON-schema object.
    for (svc_name, tool) in reg.all_tools() {
        assert!(!tool.name.is_empty() && !tool.description.is_empty(), "{svc_name}");
        assert_eq!(tool.input_schema["type"], "object", "{svc_name}/{}", tool.name);
    }
}

#[test]
fn config_defaults_cors_to_wildcard_and_optionals_to_none() {
    let cfg: Config = serde_json::from_value(serde_json::json!({
        "app": {"host": "0.0.0.0", "port": 8080, "env": "test"},
        "auth": {"jwt_secret": "s", "jwt_expiry_seconds": 1, "refresh_token_expiry_seconds": 2},
        "redis": {"url": "redis://x"},
        "services": required_services(),
    }))
    .expect("config deserialises");
    assert_eq!(cfg.app.cors_allowed_origins, "*");
    assert!(cfg.services.field_ops_url.is_none());
    assert!(cfg.services.omnideliv_url.is_none());
    assert!(cfg.services.promotions_url.is_none());
    assert!(cfg.services.compliance_url.is_none());
}

#[test]
fn config_rejects_a_missing_required_service() {
    let r: Result<ServicesConfig, _> =
        serde_json::from_value(serde_json::json!({"identity_url": "a"}));
    assert!(r.is_err(), "a gateway missing a required upstream must fail to boot, not guess");
}
