/// MCP Tool executor — calls downstream service REST APIs on behalf of agents.
///
/// Each tool maps to a specific service endpoint. The tool registry is populated
/// at startup from the api-gateway's MCP server registry (same seed data).
///
/// Tool execution is: deserialise input → HTTP call → return JSON result.
/// All tool calls are logged as AgentActions regardless of outcome.
use std::collections::HashMap;
use std::sync::Arc;
use serde_json::{json, Value};

use crate::domain::entities::ToolDefinition;

/// Result of a single tool execution.
#[derive(Debug)]
pub struct ToolResult {
    pub tool_use_id: String,
    pub content:     Value,
    pub is_error:    bool,
}

/// Registered tool with its handler.
type ToolHandler = Arc<dyn Fn(Value) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Value>> + Send>> + Send + Sync>;

pub struct ToolRegistry {
    definitions: Vec<ToolDefinition>,
    handlers:    HashMap<String, ToolHandler>,
    http:        reqwest::Client,
    base_urls:   ServiceUrls,
}

#[derive(Debug, Clone)]
pub struct ServiceUrls {
    pub dispatch:    String,
    pub order_intake: String,
    pub driver_ops:  String,
    pub payments:    String,
    pub engagement:  String,
    pub analytics:   String,
    pub cdp:         String,
    pub hub_ops:     String,
    pub fleet:       String,
}

impl ToolRegistry {
    pub fn new(base_urls: ServiceUrls) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("tool http client");
        let mut registry = Self {
            definitions: Vec::new(),
            handlers: HashMap::new(),
            http,
            base_urls,
        };
        registry.register_all_tools();
        registry
    }

    pub fn definitions(&self) -> &[ToolDefinition] {
        &self.definitions
    }

    pub async fn execute(&self, tool_name: &str, input: Value, tool_use_id: String) -> ToolResult {
        match self.handlers.get(tool_name) {
            Some(handler) => {
                match handler(input).await {
                    Ok(result) => ToolResult { tool_use_id, content: result, is_error: false },
                    Err(e) => ToolResult {
                        tool_use_id,
                        content: json!({"error": e.to_string()}),
                        is_error: true,
                    },
                }
            }
            None => ToolResult {
                tool_use_id,
                content: json!({"error": format!("Unknown tool: {}", tool_name)}),
                is_error: true,
            },
        }
    }

    // ------------------------------------------------------------------
    // Tool registration — each tool definition + HTTP handler
    // ------------------------------------------------------------------

    fn register_all_tools(&mut self) {
        let http = self.http.clone();
        let urls = self.base_urls.clone();

        // ── get_available_drivers ─────────────────────────────────────
        self.register(
            ToolDefinition {
                name: "get_available_drivers".into(),
                description: "Find available drivers near a pickup location. Returns scored list by proximity and current workload.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "lat": {"type": "number", "description": "Pickup latitude"},
                        "lng": {"type": "number", "description": "Pickup longitude"},
                        "radius_km": {"type": "number", "description": "Search radius in km (default 10)"}
                    },
                    "required": ["lat", "lng"]
                }),
            },
            {
                let http = http.clone();
                let url = urls.dispatch.clone();
                move |input: Value| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let resp = http
                            .get(format!("{}/internal/drivers/available", url))
                            .query(&[
                                ("lat",       input["lat"].to_string()),
                                ("lng",       input["lng"].to_string()),
                                ("radius_km", input.get("radius_km").and_then(|v| v.as_f64()).unwrap_or(10.0).to_string()),
                            ])
                            .send().await?
                            .json::<Value>().await?;
                        Ok(resp)
                    })
                }
            },
        );

        // ── assign_driver ─────────────────────────────────────────────
        self.register(
            ToolDefinition {
                name: "assign_driver".into(),
                description: "Assign a specific driver to a shipment. Triggers route creation and notifies the driver.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "shipment_id": {"type": "string", "format": "uuid"},
                        "driver_id":  {"type": "string", "format": "uuid", "description": "Leave null for auto-assignment"},
                    },
                    "required": ["shipment_id"]
                }),
            },
            {
                let http = http.clone();
                let url = urls.dispatch.clone();
                move |input: Value| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let shipment_id = input["shipment_id"].as_str().unwrap_or("");
                        let resp = http
                            .post(format!("{}/v1/assignments/{}/auto-assign", url, shipment_id))
                            .json(&input)
                            .send().await?
                            .json::<Value>().await?;
                        Ok(resp)
                    })
                }
            },
        );

        // ── get_shipment ──────────────────────────────────────────────
        self.register(
            ToolDefinition {
                name: "get_shipment".into(),
                description: "Retrieve full shipment details including current status, history, and driver information.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "shipment_id": {"type": "string", "format": "uuid"}
                    },
                    "required": ["shipment_id"]
                }),
            },
            {
                let http = http.clone();
                let url = urls.order_intake.clone();
                move |input: Value| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let id = input["shipment_id"].as_str().unwrap_or("");
                        let resp = http
                            .get(format!("{}/v1/shipments/{}", url, id))
                            .send().await?
                            .json::<Value>().await?;
                        Ok(resp)
                    })
                }
            },
        );

        // ── reschedule_delivery ───────────────────────────────────────
        self.register(
            ToolDefinition {
                name: "reschedule_delivery".into(),
                description: "Reschedule a failed delivery to the next available time slot.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "shipment_id":    {"type": "string", "format": "uuid"},
                        "preferred_date": {"type": "string", "description": "ISO 8601 date, e.g. 2026-03-20"}
                    },
                    "required": ["shipment_id"]
                }),
            },
            {
                let http = http.clone();
                let url = urls.order_intake.clone();
                move |input: Value| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let id = input["shipment_id"].as_str().unwrap_or("");
                        let resp = http
                            .post(format!("{}/v1/shipments/{}/reschedule", url, id))
                            .json(&input)
                            .send().await?
                            .json::<Value>().await?;
                        Ok(resp)
                    })
                }
            },
        );

        // ── send_notification ─────────────────────────────────────────
        self.register(
            ToolDefinition {
                name: "send_notification".into(),
                description: "Send a notification to a customer via WhatsApp, SMS, or email using a template.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "customer_id":  {"type": "string", "format": "uuid"},
                        "channel":      {"type": "string", "enum": ["whatsapp", "sms", "email", "push"]},
                        "template_id":  {"type": "string"},
                        "variables":    {"type": "object"}
                    },
                    "required": ["customer_id", "channel", "template_id"]
                }),
            },
            {
                let http = http.clone();
                let url = urls.engagement.clone();
                move |input: Value| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let resp = http
                            .post(format!("{}/v1/notifications", url))
                            .json(&input)
                            .send().await?
                            .json::<Value>().await?;
                        Ok(resp)
                    })
                }
            },
        );

        // ── get_delivery_metrics ──────────────────────────────────────
        self.register(
            ToolDefinition {
                name: "get_delivery_metrics".into(),
                description: "Get delivery KPIs for a tenant over a date range: success rate, on-time %, COD collection rate.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "from": {"type": "string", "description": "Start date ISO 8601"},
                        "to":   {"type": "string", "description": "End date ISO 8601"}
                    },
                    "required": ["from", "to"]
                }),
            },
            {
                let http = http.clone();
                let url = urls.analytics.clone();
                move |input: Value| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let resp = http
                            .get(format!("{}/v1/analytics/kpis", url))
                            .query(&[
                                ("from", input["from"].as_str().unwrap_or("")),
                                ("to",   input["to"].as_str().unwrap_or("")),
                            ])
                            .send().await?
                            .json::<Value>().await?;
                        Ok(resp)
                    })
                }
            },
        );

        // ── reconcile_cod ─────────────────────────────────────────────
        self.register(
            ToolDefinition {
                name: "reconcile_cod".into(),
                description: "Trigger COD reconciliation for a shipment that has been delivered but not credited to the merchant wallet.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "shipment_id": {"type": "string", "format": "uuid"}
                    },
                    "required": ["shipment_id"]
                }),
            },
            {
                let http = http.clone();
                let url = urls.payments.clone();
                move |input: Value| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let resp = http
                            .post(format!("{}/v1/cod/reconcile", url))
                            .json(&input)
                            .send().await?
                            .json::<Value>().await?;
                        Ok(resp)
                    })
                }
            },
        );

        // ── get_driver_performance ────────────────────────────────────
        self.register(
            ToolDefinition {
                name: "get_driver_performance".into(),
                description: "Get delivery performance stats for a specific driver: success rate, average delivery time, recent failures.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "driver_id": {"type": "string", "format": "uuid"},
                        "days":      {"type": "integer", "description": "Lookback window in days (default 30)"}
                    },
                    "required": ["driver_id"]
                }),
            },
            {
                let http = http.clone();
                let url = urls.analytics.clone();
                move |input: Value| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let resp = http
                            .get(format!("{}/v1/analytics/driver-performance", url))
                            .query(&[("driver_id", input["driver_id"].as_str().unwrap_or(""))])
                            .send().await?
                            .json::<Value>().await?;
                        Ok(resp)
                    })
                }
            },
        );

        // ── get_customer_profile ─────────────────────────────────────
        self.register(
            ToolDefinition {
                name: "get_customer_profile".into(),
                description: "Retrieve a unified customer profile from the CDP including contact info, shipment history, preferences, and loyalty tier.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "customer_id": {"type": "string", "format": "uuid", "description": "Customer UUID"},
                        "phone":       {"type": "string", "description": "Phone number for lookup when customer_id is unknown"}
                    }
                }),
            },
            {
                let http = http.clone();
                let url = urls.cdp.clone();
                move |input: Value| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let resp = if let Some(id) = input["customer_id"].as_str() {
                            http.get(format!("{}/v1/profiles/{}", url, id))
                                .send().await?
                                .json::<Value>().await?
                        } else {
                            http.get(format!("{}/v1/profiles/lookup", url))
                                .query(&[("phone", input["phone"].as_str().unwrap_or(""))])
                                .send().await?
                                .json::<Value>().await?
                        };
                        Ok(resp)
                    })
                }
            },
        );

        // ── get_churn_score ───────────────────────────────────────────
        self.register(
            ToolDefinition {
                name: "get_churn_score".into(),
                description: "Get the ML-predicted churn probability for a customer (0.0–1.0). Scores above 0.7 indicate high churn risk.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "customer_id": {"type": "string", "format": "uuid"}
                    },
                    "required": ["customer_id"]
                }),
            },
            {
                let http = http.clone();
                let url = urls.cdp.clone();
                move |input: Value| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let id = input["customer_id"].as_str().unwrap_or("");
                        let resp = http
                            .get(format!("{}/v1/profiles/{}/churn-score", url, id))
                            .send().await?
                            .json::<Value>().await?;
                        Ok(resp)
                    })
                }
            },
        );

        // ── get_customer_preferences ──────────────────────────────────
        self.register(
            ToolDefinition {
                name: "get_customer_preferences".into(),
                description: "Get a customer's communication preferences: preferred channel, opt-in status per channel, language, delivery time window preferences.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "customer_id": {"type": "string", "format": "uuid"}
                    },
                    "required": ["customer_id"]
                }),
            },
            {
                let http = http.clone();
                let url = urls.engagement.clone();
                move |input: Value| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let id = input["customer_id"].as_str().unwrap_or("");
                        let resp = http
                            .get(format!("{}/v1/preferences/{}", url, id))
                            .send().await?
                            .json::<Value>().await?;
                        Ok(resp)
                    })
                }
            },
        );

        // ── generate_invoice ──────────────────────────────────────────
        self.register(
            ToolDefinition {
                name: "generate_invoice".into(),
                description: "Generate or retrieve the invoice for a shipment. Returns invoice PDF URL and line items.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "shipment_id": {"type": "string", "format": "uuid"},
                        "force":       {"type": "boolean", "description": "Re-generate even if invoice already exists (default false)"}
                    },
                    "required": ["shipment_id"]
                }),
            },
            {
                let http = http.clone();
                let url = urls.payments.clone();
                move |input: Value| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let shipment_id = input["shipment_id"].as_str().unwrap_or("");
                        let resp = http
                            .post(format!("{}/v1/invoices", url))
                            .json(&json!({"shipment_id": shipment_id, "force": input.get("force").and_then(|v| v.as_bool()).unwrap_or(false)}))
                            .send().await?
                            .json::<Value>().await?;
                        Ok(resp)
                    })
                }
            },
        );

        // ── get_cod_balance ───────────────────────────────────────────
        self.register(
            ToolDefinition {
                name: "get_cod_balance".into(),
                description: "Get the current COD (Cash on Delivery) wallet balance for a merchant, including pending remittances and last payout date.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "merchant_id": {"type": "string", "format": "uuid"}
                    },
                    "required": ["merchant_id"]
                }),
            },
            {
                let http = http.clone();
                let url = urls.payments.clone();
                move |input: Value| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let merchant_id = input["merchant_id"].as_str().unwrap_or("");
                        let resp = http
                            .get(format!("{}/v1/cod/balance/{}", url, merchant_id))
                            .send().await?
                            .json::<Value>().await?;
                        Ok(resp)
                    })
                }
            },
        );

        // ── get_zone_demand_forecast ──────────────────────────────────
        self.register(
            ToolDefinition {
                name: "get_zone_demand_forecast".into(),
                description: "Get AI-predicted shipment volume forecast for a delivery zone over the next N days. Used for proactive driver and vehicle allocation.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "zone_id":   {"type": "string", "description": "Zone identifier (e.g. 'MM-QC-01' for Metro Manila Quezon City zone 1)"},
                        "days_ahead": {"type": "integer", "description": "Forecast horizon in days (default 7, max 30)"}
                    },
                    "required": ["zone_id"]
                }),
            },
            {
                let http = http.clone();
                let url = urls.analytics.clone();
                move |input: Value| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let resp = http
                            .get(format!("{}/v1/analytics/demand-forecast", url))
                            .query(&[
                                ("zone_id",    input["zone_id"].as_str().unwrap_or("")),
                                ("days_ahead", &input.get("days_ahead").and_then(|v| v.as_i64()).unwrap_or(7).to_string()),
                            ])
                            .send().await?
                            .json::<Value>().await?;
                        Ok(resp)
                    })
                }
            },
        );

        // ── get_hub_capacity ──────────────────────────────────────────
        self.register(
            ToolDefinition {
                name: "get_hub_capacity".into(),
                description: "Get current capacity utilisation for a hub: total bays, occupied bays, inbound queue length, and estimated processing time.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "hub_id": {"type": "string", "format": "uuid"}
                    },
                    "required": ["hub_id"]
                }),
            },
            {
                let http = http.clone();
                let url = urls.hub_ops.clone();
                move |input: Value| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let id = input["hub_id"].as_str().unwrap_or("");
                        let resp = http
                            .get(format!("{}/v1/hubs/{}/capacity", url, id))
                            .send().await?
                            .json::<Value>().await?;
                        Ok(resp)
                    })
                }
            },
        );

        // ── schedule_dock ─────────────────────────────────────────────
        self.register(
            ToolDefinition {
                name: "schedule_dock".into(),
                description: "Schedule a loading dock slot at a hub for an inbound or outbound vehicle. Returns confirmed slot time and dock number.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "hub_id":         {"type": "string", "format": "uuid"},
                        "vehicle_id":     {"type": "string", "format": "uuid"},
                        "direction":      {"type": "string", "enum": ["inbound", "outbound"]},
                        "requested_at":   {"type": "string", "description": "Preferred arrival ISO 8601 datetime"}
                    },
                    "required": ["hub_id", "vehicle_id", "direction"]
                }),
            },
            {
                let http = http.clone();
                let url = urls.hub_ops.clone();
                move |input: Value| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let id = input["hub_id"].as_str().unwrap_or("");
                        let resp = http
                            .post(format!("{}/v1/hubs/{}/dock-slots", url, id))
                            .json(&input)
                            .send().await?
                            .json::<Value>().await?;
                        Ok(resp)
                    })
                }
            },
        );

        // ── get_vehicle_status ────────────────────────────────────────
        self.register(
            ToolDefinition {
                name: "get_vehicle_status".into(),
                description: "Get real-time status of a specific vehicle: availability, assigned driver, current location, fuel level, and next maintenance due date.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "vehicle_id": {"type": "string", "format": "uuid"}
                    },
                    "required": ["vehicle_id"]
                }),
            },
            {
                let http = http.clone();
                let url = urls.fleet.clone();
                move |input: Value| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let id = input["vehicle_id"].as_str().unwrap_or("");
                        let resp = http
                            .get(format!("{}/v1/vehicles/{}", url, id))
                            .send().await?
                            .json::<Value>().await?;
                        Ok(resp)
                    })
                }
            },
        );

        // ── get_fleet_availability ────────────────────────────────────
        self.register(
            ToolDefinition {
                name: "get_fleet_availability".into(),
                description: "Get available vehicles in the fleet filtered by type, zone, and time window. Returns vehicles sorted by proximity to a given location.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "vehicle_type": {"type": "string", "enum": ["motorcycle", "van", "truck", "bicycle"], "description": "Filter by vehicle type"},
                        "lat":          {"type": "number", "description": "Reference latitude for proximity sorting"},
                        "lng":          {"type": "number", "description": "Reference longitude for proximity sorting"},
                        "at_time":      {"type": "string", "description": "ISO 8601 datetime to check availability (default: now)"}
                    }
                }),
            },
            {
                let http = http.clone();
                let url = urls.fleet.clone();
                move |input: Value| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let mut query: Vec<(&str, String)> = Vec::new();
                        if let Some(vt) = input["vehicle_type"].as_str() {
                            query.push(("vehicle_type", vt.to_string()));
                        }
                        if let Some(lat) = input["lat"].as_f64() {
                            query.push(("lat", lat.to_string()));
                        }
                        if let Some(lng) = input["lng"].as_f64() {
                            query.push(("lng", lng.to_string()));
                        }
                        if let Some(at) = input["at_time"].as_str() {
                            query.push(("at_time", at.to_string()));
                        }
                        let resp = http
                            .get(format!("{}/v1/vehicles", url))
                            .query(&query)
                            .send().await?
                            .json::<Value>().await?;
                        Ok(resp)
                    })
                }
            },
        );

        // ── get_driver_location ───────────────────────────────────────
        self.register(
            ToolDefinition {
                name: "get_driver_location".into(),
                description: "Get the last known GPS location and heading of a driver. Location is updated every 30 seconds by the driver app.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "driver_id": {"type": "string", "format": "uuid"}
                    },
                    "required": ["driver_id"]
                }),
            },
            {
                let http = http.clone();
                let url = urls.driver_ops.clone();
                move |input: Value| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let id = input["driver_id"].as_str().unwrap_or("");
                        let resp = http
                            .get(format!("{}/v1/drivers/{}/location", url, id))
                            .send().await?
                            .json::<Value>().await?;
                        Ok(resp)
                    })
                }
            },
        );

        // ── send_driver_instruction ───────────────────────────────────
        self.register(
            ToolDefinition {
                name: "send_driver_instruction".into(),
                description: "Send an operational instruction to a driver's app. Use for route changes, priority reorders, pickup additions, or urgent alerts.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "driver_id":   {"type": "string", "format": "uuid"},
                        "instruction": {"type": "string", "enum": ["route_change", "add_stop", "remove_stop", "priority_change", "return_to_hub", "urgent_alert"]},
                        "payload":     {"type": "object", "description": "Instruction-specific data (e.g. new stop address, shipment_id to add)"},
                        "message":     {"type": "string", "description": "Human-readable message shown to driver in the app"}
                    },
                    "required": ["driver_id", "instruction", "message"]
                }),
            },
            {
                let http = http.clone();
                let url = urls.driver_ops.clone();
                move |input: Value| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let id = input["driver_id"].as_str().unwrap_or("");
                        let resp = http
                            .post(format!("{}/v1/drivers/{}/instructions", url, id))
                            .json(&input)
                            .send().await?
                            .json::<Value>().await?;
                        Ok(resp)
                    })
                }
            },
        );

        // ── escalate_to_human ─────────────────────────────────────────
        self.register(
            ToolDefinition {
                name: "escalate_to_human".into(),
                description: "Escalate this case to a human operator. Use when you cannot resolve the situation autonomously with sufficient confidence.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "reason":   {"type": "string", "description": "Clear explanation of why human intervention is needed"},
                        "urgency":  {"type": "string", "enum": ["low", "medium", "high", "critical"]},
                        "context":  {"type": "object", "description": "Relevant data for the human to review"}
                    },
                    "required": ["reason", "urgency"]
                }),
            },
            {
                // This is a special tool — the agent loop intercepts it directly.
                // Handler returns a marker so the loop knows to escalate.
                move |input: Value| {
                    Box::pin(async move {
                        Ok(json!({
                            "__escalate": true,
                            "reason":  input["reason"],
                            "urgency": input["urgency"],
                            "context": input.get("context"),
                        }))
                    })
                }
            },
        );
    }

    fn register<F, Fut>(&mut self, def: ToolDefinition, handler: F)
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = anyhow::Result<Value>> + Send + 'static,
    {
        let name = def.name.clone();
        self.definitions.push(def);
        self.handlers.insert(
            name,
            Arc::new(move |input| Box::pin(handler(input))),
        );
    }
}
