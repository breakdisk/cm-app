//! MCP Server Registry — tracks all available MCP servers for the AI layer.
//! Populated at startup by reading service discovery (K8s env vars or static config).
//! Enterprise tenants can register additional external MCP servers.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerEntry {
    pub id: Uuid,
    pub name: String,
    pub service: String,         // e.g. "dispatch", "order-intake"
    pub endpoint: String,        // MCP server URL
    pub tools: Vec<McpTool>,
    pub tenant_id: Option<Uuid>, // None = platform-level; Some = tenant-registered
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,  // JSON Schema
}

pub struct McpRegistry {
    servers: HashMap<String, McpServerEntry>,
}

impl McpRegistry {
    pub fn new() -> Self {
        Self { servers: HashMap::new() }
    }

    /// Register a platform MCP server (called at gateway startup for each service).
    pub fn register_platform_server(&mut self, entry: McpServerEntry) {
        self.servers.insert(entry.service.clone(), entry);
    }

    /// Register an enterprise tenant's external MCP server.
    /// Validates that the tenant tier is Enterprise before accepting.
    pub fn register_tenant_server(
        &mut self,
        entry: McpServerEntry,
        tier: &str,
    ) -> Result<(), &'static str> {
        if tier != "enterprise" {
            return Err("External MCP server registration requires Enterprise subscription");
        }
        self.servers.insert(format!("tenant:{}", entry.id), entry);
        Ok(())
    }

    pub fn get(&self, service: &str) -> Option<&McpServerEntry> {
        self.servers.get(service)
    }

    /// Return all tools across all active servers — used by AI agents for tool discovery.
    pub fn all_tools(&self) -> Vec<(&str, &McpTool)> {
        self.servers
            .values()
            .filter(|s| s.is_active)
            .flat_map(|s| s.tools.iter().map(|t| (s.service.as_str(), t)))
            .collect()
    }
}

/// Seed the registry with platform-level MCP servers at startup.
pub fn seed_platform_servers(registry: &mut McpRegistry, cfg: &crate::config::ServicesConfig) {
    let servers = vec![
        McpServerEntry {
            id: Uuid::new_v4(),
            name: "Dispatch MCP".into(),
            service: "dispatch".into(),
            endpoint: format!("{}/mcp", cfg.dispatch_url),
            tools: vec![
                McpTool { name: "assign_driver".into(), description: "Assign the optimal available driver to a shipment".into(), input_schema: serde_json::json!({"type":"object","properties":{"shipment_id":{"type":"string"}},"required":["shipment_id"]}) },
                McpTool { name: "optimize_route".into(), description: "Re-optimize a driver's route given new stops".into(), input_schema: serde_json::json!({"type":"object","properties":{"driver_id":{"type":"string"}},"required":["driver_id"]}) },
                McpTool { name: "get_available_drivers".into(), description: "List drivers available in a zone".into(), input_schema: serde_json::json!({"type":"object","properties":{"zone_id":{"type":"string"}}}) },
            ],
            tenant_id: None,
            is_active: true,
        },
        McpServerEntry {
            id: Uuid::new_v4(),
            name: "Order Intake MCP".into(),
            service: "order-intake".into(),
            endpoint: format!("{}/mcp", cfg.order_intake_url),
            tools: vec![
                McpTool { name: "get_shipment".into(), description: "Retrieve full shipment details by ID or tracking number".into(), input_schema: serde_json::json!({"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}) },
                McpTool { name: "reschedule_delivery".into(), description: "Reschedule a failed delivery attempt".into(), input_schema: serde_json::json!({"type":"object","properties":{"shipment_id":{"type":"string"},"preferred_date":{"type":"string"}},"required":["shipment_id"]}) },
                McpTool { name: "cancel_shipment".into(), description: "Cancel a shipment that has not yet been picked up".into(), input_schema: serde_json::json!({"type":"object","properties":{"shipment_id":{"type":"string"},"reason":{"type":"string"}},"required":["shipment_id","reason"]}) },
            ],
            tenant_id: None,
            is_active: true,
        },
        McpServerEntry {
            id: Uuid::new_v4(),
            name: "Engagement MCP".into(),
            service: "engagement".into(),
            endpoint: format!("{}/mcp", cfg.engagement_url),
            tools: vec![
                McpTool { name: "send_notification".into(), description: "Send a notification to a customer via specified channel".into(), input_schema: serde_json::json!({"type":"object","properties":{"customer_id":{"type":"string"},"channel":{"type":"string","enum":["whatsapp","sms","email","push"]},"template_id":{"type":"string"},"variables":{"type":"object"}},"required":["customer_id","channel","template_id"]}) },
            ],
            tenant_id: None,
            is_active: true,
        },
        McpServerEntry {
            id: Uuid::new_v4(),
            name: "Analytics MCP".into(),
            service: "analytics".into(),
            endpoint: format!("{}/mcp", cfg.analytics_url),
            tools: vec![
                McpTool { name: "get_delivery_metrics".into(), description: "Get delivery KPIs for a date range".into(), input_schema: serde_json::json!({"type":"object","properties":{"from":{"type":"string"},"to":{"type":"string"},"zone_id":{"type":"string"}},"required":["from","to"]}) },
                McpTool { name: "get_zone_demand_forecast".into(), description: "Get predicted shipment volume for a zone for the next N days".into(), input_schema: serde_json::json!({"type":"object","properties":{"zone_id":{"type":"string"},"days":{"type":"integer"}},"required":["zone_id","days"]}) },
            ],
            tenant_id: None,
            is_active: true,
        },
    ];

    for server in servers {
        registry.register_platform_server(server);
    }
}
