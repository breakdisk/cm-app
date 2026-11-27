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

use logisticos_agent_runtime::tools::{ToolBox, ToolDefinition};

// `ToolResult` and `ToolContext` now live in `logisticos-agent-runtime` so the
// shared loop can construct and read them. Re-exported here because this
// module's own signatures — and every call site — refer to them unqualified.
//
// `ToolContext.bearer` carries the *originating caller's* access token so a tool
// can call a JWT-protected downstream endpoint as that user rather than as an
// anonymous internal caller. Autonomous (Kafka-triggered) agents have no human
// caller and therefore no bearer — those runs can only reach `/v1/internal/*`
// endpoints, which is the correct blast radius for an unattended agent.
pub use logisticos_agent_runtime::tools::{ToolContext, ToolResult};

/// Attach the caller's bearer token to an outbound request when one is present.
fn authed(req: reqwest::RequestBuilder, ctx: &ToolContext) -> reqwest::RequestBuilder {
    match &ctx.bearer {
        Some(token) => req.bearer_auth(token),
        None => req,
    }
}

/// Parse a caller-supplied id that will be interpolated into a URL path.
///
/// Anything that is not a canonical UUID is refused before a request is built:
/// a raw string such as `<uuid>?tenant_id=<victim>`, `<uuid>/requeue#` or
/// `../internal/...` would otherwise be re-interpreted by URL parsing (query,
/// fragment, dot-segment) and could reach a different, unscoped route.
fn path_uuid(input: &Value, field: &str) -> anyhow::Result<uuid::Uuid> {
    let raw = input
        .get(field)
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("`{field}` is required and must be a UUID string"))?;
    uuid::Uuid::parse_str(raw.trim())
        .map_err(|_| anyhow::anyhow!("`{field}` must be a valid UUID"))
}

/// Max characters of a downstream error body carried into the tool error.
const ERROR_BODY_MAX: usize = 512;

/// Pure status/body mapping for a downstream response: non-2xx is an error
/// carrying the status and a truncated body; an empty 2xx body is `null`;
/// otherwise the parsed JSON.
fn interpret_response(status: reqwest::StatusCode, body: &[u8]) -> anyhow::Result<Value> {
    if !status.is_success() {
        let text = String::from_utf8_lossy(body);
        let snippet: String = text.chars().take(ERROR_BODY_MAX).collect();
        anyhow::bail!("downstream returned HTTP {}: {}", status.as_u16(), snippet);
    }
    if body.iter().all(|b| b.is_ascii_whitespace()) {
        return Ok(Value::Null);
    }
    serde_json::from_slice(body)
        .map_err(|e| anyhow::anyhow!("downstream returned a non-JSON body: {e}"))
}

/// Send a request and judge success by HTTP status, not by whether the body
/// happens to parse as JSON.
async fn send_json(req: reqwest::RequestBuilder) -> anyhow::Result<Value> {
    let resp = req.send().await?;
    let status = resp.status();
    let body = resp.bytes().await?;
    interpret_response(status, &body)
}

trait SendJson {
    async fn send_json(self) -> anyhow::Result<Value>;
}

impl SendJson for reqwest::RequestBuilder {
    async fn send_json(self) -> anyhow::Result<Value> {
        send_json(self).await
    }
}

/// Map the `send_notification` tool input onto engagement's
/// `SendNotificationRequest` body: `variables` become `template_vars` (all
/// values stringified, as that map is `String -> String`) and `recipient` is
/// carried inside `template_vars`, which is where the route reads it from.
fn notification_request_body(input: &Value) -> Value {
    let mut vars = serde_json::Map::new();
    if let Some(obj) = input.get("variables").and_then(|v| v.as_object()) {
        for (k, v) in obj {
            let text = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            vars.insert(k.clone(), Value::String(text));
        }
    }
    if let Some(r) = input.get("recipient").and_then(|v| v.as_str()) {
        vars.insert("recipient".into(), Value::String(r.to_string()));
    }
    let mut body = json!({
        "customer_id":   input["customer_id"],
        "channel":       input["channel"],
        "template_id":   input["template_id"],
        "template_vars": Value::Object(vars),
    });
    if let Some(p) = input.get("priority").and_then(|v| v.as_str()) {
        body["priority"] = Value::String(p.to_string());
    }
    body
}

/// Registered tool with its handler.
type ToolHandler = Arc<dyn Fn(Value, ToolContext) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<Value>> + Send>> + Send + Sync>;

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

    pub async fn execute(&self, tool_name: &str, input: Value, tool_use_id: String, ctx: ToolContext) -> ToolResult {
        match self.handlers.get(tool_name) {
            Some(handler) => {
                match handler(input, ctx).await {
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
                        "lat":       {"type": "number", "description": "Pickup latitude"},
                        "lng":       {"type": "number", "description": "Pickup longitude"},
                        "radius_km": {"type": "number", "description": "Search radius in km (default 10)"},
                        "tenant_id": {"type": "string", "format": "uuid"}
                    },
                    "required": ["lat", "lng", "tenant_id"]
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
                            .get(format!("{}/v1/internal/drivers/available", url))
                            .query(&[
                                ("lat",       input["lat"].to_string()),
                                ("lng",       input["lng"].to_string()),
                                ("radius_km", input.get("radius_km").and_then(|v| v.as_f64()).unwrap_or(10.0).to_string()),
                                ("tenant_id", input["tenant_id"].as_str().unwrap_or("").to_string()),
                            ])
                            .send_json().await?;
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
                        "driver_id":  {"type": "string", "format": "uuid", "description": "Preferred driver; service selects best if omitted"},
                        "tenant_id":  {"type": "string", "format": "uuid"},
                    },
                    "required": ["shipment_id", "tenant_id"]
                }),
            },
            {
                let http = http.clone();
                let url = urls.dispatch.clone();
                move |input: Value| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let shipment_id = path_uuid(&input, "shipment_id")?;
                        let resp = http
                            .post(format!("{}/v1/internal/shipments/{}/assign", url, shipment_id))
                            .json(&input)
                            .send_json().await?;
                        Ok(resp)
                    })
                }
            },
        );

        // ── get_shipment ──────────────────────────────────────────────
        // `/v1/shipments/:id` is JWT-protected (SHIPMENT_READ), so this tool
        // presents the caller's own token. An unattended agent with no bearer
        // gets a 401 from order-intake rather than silently reading data it has
        // no authority for.
        self.register_ctx(
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
                move |input: Value, ctx: ToolContext| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let id = path_uuid(&input, "shipment_id")?;
                        let resp = authed(http.get(format!("{}/v1/shipments/{}", url, id)), &ctx)
                            .send_json().await?;
                        Ok(resp)
                    })
                }
            },
        );

        // ── reschedule_delivery ───────────────────────────────────────
        // Mutating and JWT-protected (SHIPMENT_UPDATE) — same reasoning as
        // get_shipment above.
        self.register_ctx(
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
                move |input: Value, ctx: ToolContext| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let id = path_uuid(&input, "shipment_id")?;
                        let resp = authed(
                            http.post(format!("{}/v1/shipments/{}/reschedule", url, id)).json(&input),
                            &ctx,
                        )
                            .send_json().await?;
                        Ok(resp)
                    })
                }
            },
        );

        // ── send_notification ─────────────────────────────────────────
        self.register_ctx(
            ToolDefinition {
                name: "send_notification".into(),
                description: "Send a notification to a customer via WhatsApp, SMS, or email using a template.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "customer_id":  {"type": "string", "format": "uuid"},
                        "channel":      {"type": "string", "enum": ["whatsapp", "sms", "email", "push"]},
                        "template_id":  {"type": "string", "format": "uuid", "description": "UUID of the notification template (must match the channel)"},
                        "recipient":    {"type": "string", "description": "Phone number or email address to deliver to"},
                        "variables":    {"type": "object", "description": "Template variables (string values)"},
                        "priority":     {"type": "string", "enum": ["high", "normal", "low"]}
                    },
                    "required": ["customer_id", "channel", "template_id", "recipient"]
                }),
            },
            {
                let http = http.clone();
                let url = urls.engagement.clone();
                move |input: Value, ctx: ToolContext| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let body = notification_request_body(&input);
                        let resp = authed(http.post(format!("{}/v1/notifications/send", url)), &ctx)
                            .json(&body)
                            .send_json().await?;
                        Ok(resp)
                    })
                }
            },
        );

        // ── get_delivery_metrics ──────────────────────────────────────
        self.register_ctx(
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
                move |input: Value, ctx: ToolContext| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let resp = authed(http.get(format!("{}/v1/analytics/kpis", url)), &ctx)
                            .query(&[
                                ("from", input["from"].as_str().unwrap_or("")),
                                ("to",   input["to"].as_str().unwrap_or("")),
                            ])
                            .send_json().await?;
                        Ok(resp)
                    })
                }
            },
        );

        // ── reconcile_cod ─────────────────────────────────────────────
        self.register_ctx(
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
                move |input: Value, ctx: ToolContext| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let resp = authed(http.post(format!("{}/v1/cod/reconcile", url)), &ctx)
                            .json(&input)
                            .send_json().await?;
                        Ok(resp)
                    })
                }
            },
        );

        // ── get_driver_performance ────────────────────────────────────
        self.register_ctx(
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
                move |input: Value, ctx: ToolContext| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let resp = authed(http.get(format!("{}/v1/analytics/driver-performance", url)), &ctx)
                            .query(&[("driver_id", input["driver_id"].as_str().unwrap_or(""))])
                            .send_json().await?;
                        Ok(resp)
                    })
                }
            },
        );

        // ── get_customer_profile ─────────────────────────────────────
        self.register_ctx(
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
                move |input: Value, ctx: ToolContext| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let resp = if input.get("customer_id").is_some_and(|v| !v.is_null()) {
                            let id = path_uuid(&input, "customer_id")?;
                            authed(http.get(format!("{}/v1/customers/{}", url, id)), &ctx)
                                .send_json().await?
                        } else {
                            authed(http.get(format!("{}/v1/customers", url)), &ctx)
                                .query(&[("phone", input["phone"].as_str().unwrap_or(""))])
                                .send_json().await?
                        };
                        Ok(resp)
                    })
                }
            },
        );

        // ── get_churn_score ───────────────────────────────────────────
        self.register_ctx(
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
                move |input: Value, ctx: ToolContext| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let id = path_uuid(&input, "customer_id")?;
                        let resp = authed(http.get(format!("{}/v1/customers/{}/churn-score", url, id)), &ctx)
                            .send_json().await?;
                        Ok(resp)
                    })
                }
            },
        );

        // ── get_customer_preferences ──────────────────────────────────
        self.register_ctx(
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
                let url = urls.cdp.clone();
                move |input: Value, ctx: ToolContext| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let id = path_uuid(&input, "customer_id")?;
                        let resp = authed(http.get(format!("{}/v1/customers/{}/preferences", url, id)), &ctx)
                            .send_json().await?;
                        Ok(resp)
                    })
                }
            },
        );

        // ── generate_invoice ──────────────────────────────────────────
        self.register_ctx(
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
                move |input: Value, ctx: ToolContext| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let shipment_id = path_uuid(&input, "shipment_id")?;
                        let resp = authed(http.post(format!("{}/v1/invoices", url)), &ctx)
                            .json(&json!({"shipment_id": shipment_id, "force": input.get("force").and_then(|v| v.as_bool()).unwrap_or(false)}))
                            .send_json().await?;
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
                        "merchant_id": {"type": "string", "format": "uuid"},
                        "tenant_id":   {"type": "string", "format": "uuid"}
                    },
                    "required": ["merchant_id", "tenant_id"]
                }),
            },
            {
                let http = http.clone();
                let url = urls.payments.clone();
                move |input: Value| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let merchant_id = path_uuid(&input, "merchant_id")?;
                        let tenant_id   = input["tenant_id"].as_str().unwrap_or("");
                        let resp = http
                            .get(format!("{}/v1/internal/cod/balance/{}", url, merchant_id))
                            .query(&[("tenant_id", tenant_id)])
                            .send_json().await?;
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
                            .send_json().await?;
                        Ok(resp)
                    })
                }
            },
        );

        // ── get_hub_capacity ──────────────────────────────────────────
        self.register_ctx(
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
                move |input: Value, ctx: ToolContext| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let id = path_uuid(&input, "hub_id")?;
                        let resp = authed(http.get(format!("{}/v1/hubs/{}/capacity", url, id)), &ctx)
                            .send_json().await?;
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
                        let id = path_uuid(&input, "hub_id")?;
                        let resp = http
                            .post(format!("{}/v1/hubs/{}/dock-slots", url, id))
                            .json(&input)
                            .send_json().await?;
                        Ok(resp)
                    })
                }
            },
        );

        // ── get_vehicle_status ────────────────────────────────────────
        self.register_ctx(
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
                move |input: Value, ctx: ToolContext| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let id = path_uuid(&input, "vehicle_id")?;
                        let resp = authed(http.get(format!("{}/v1/fleet/vehicles/{}", url, id)), &ctx)
                            .send_json().await?;
                        Ok(resp)
                    })
                }
            },
        );

        // ── get_fleet_availability ────────────────────────────────────
        self.register_ctx(
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
                move |input: Value, ctx: ToolContext| {
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
                        let resp = authed(http.get(format!("{}/v1/fleet/vehicles", url)), &ctx)
                            .query(&query)
                            .send_json().await?;
                        Ok(resp)
                    })
                }
            },
        );

        // ── get_driver_location ───────────────────────────────────────
        self.register_ctx(
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
                move |input: Value, ctx: ToolContext| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let id = path_uuid(&input, "driver_id")?;
                        let resp = authed(http.get(format!("{}/v1/drivers/{}/location", url, id)), &ctx)
                            .send_json().await?;
                        Ok(resp)
                    })
                }
            },
        );

        // ── send_driver_instruction ───────────────────────────────────
        self.register_ctx(
            ToolDefinition {
                name: "send_driver_instruction".into(),
                description: "Send an operational instruction to a driver's app. Use for route changes, priority reorders, pickup additions, or urgent alerts.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "driver_id":        {"type": "string", "format": "uuid"},
                        "instruction_type": {"type": "string", "enum": ["route_change", "add_stop", "remove_stop", "priority_change", "return_to_hub", "urgent_alert"]},
                        "payload":          {"type": "object", "description": "Instruction-specific data (e.g. new stop address, shipment_id to add)"},
                        "message":          {"type": "string", "description": "Human-readable message shown to driver in the app"}
                    },
                    "required": ["driver_id", "instruction_type", "message"]
                }),
            },
            {
                let http = http.clone();
                let url = urls.driver_ops.clone();
                move |input: Value, ctx: ToolContext| {
                    let http = http.clone();
                    let url = url.clone();
                    Box::pin(async move {
                        let id = path_uuid(&input, "driver_id")?;
                        let resp = authed(http.post(format!("{}/v1/drivers/{}/instructions", url, id)), &ctx)
                            .json(&input)
                            .send_json().await?;
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

    /// Register a tool whose handler does not need caller context.
    fn register<F, Fut>(&mut self, def: ToolDefinition, handler: F)
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = anyhow::Result<Value>> + Send + 'static,
    {
        let name = def.name.clone();
        self.definitions.push(def);
        self.handlers.insert(
            name,
            Arc::new(move |input, _ctx| Box::pin(handler(input))),
        );
    }

    /// Register a tool whose handler needs the caller's `ToolContext` — i.e. it
    /// calls a JWT-protected downstream endpoint and must present the caller's
    /// own token so that service's authz and tenant isolation apply.
    fn register_ctx<F, Fut>(&mut self, def: ToolDefinition, handler: F)
    where
        F: Fn(Value, ToolContext) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = anyhow::Result<Value>> + Send + 'static,
    {
        let name = def.name.clone();
        self.definitions.push(def);
        self.handlers.insert(
            name,
            Arc::new(move |input, ctx| Box::pin(handler(input, ctx))),
        );
    }
}

/// Bridges the 21-tool logistics registry to the shared runtime's contract.
///
/// The registry keeps its own inherent methods — plenty of call sites use them
/// directly — and this impl simply exposes the same behaviour under the trait
/// the agent loop depends on.
#[async_trait::async_trait]
impl ToolBox for ToolRegistry {
    fn definitions(&self) -> &[ToolDefinition] {
        ToolRegistry::definitions(self)
    }

    async fn execute(
        &self,
        name: String,
        input: Value,
        tool_use_id: String,
        ctx: ToolContext,
    ) -> ToolResult {
        ToolRegistry::execute(self, &name, input, tool_use_id, ctx).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_body_maps_to_engagement_request_shape() {
        let input = json!({
            "customer_id": "c1", "channel": "sms", "template_id": "t1",
            "recipient": "+639170000000", "variables": {"eta": 5, "name": "Ana"},
            "tenant_id": "ignored",
        });
        let body = notification_request_body(&input);
        assert_eq!(body["template_vars"]["recipient"], "+639170000000");
        assert_eq!(body["template_vars"]["eta"], "5");
        assert_eq!(body["template_vars"]["name"], "Ana");
        assert_eq!(body["channel"], "sms");
        assert!(body.get("tenant_id").is_none());
        assert!(body.get("priority").is_none());
    }

    #[test]
    fn path_uuid_accepts_a_uuid_and_refuses_url_metacharacters() {
        let ok = "123e4567-e89b-12d3-a456-426614174000";
        assert!(path_uuid(&json!({"id": ok}), "id").is_ok());
        for bad in [
            "../x",
            "123e4567-e89b-12d3-a456-426614174000?tenant_id=victim",
            "123e4567-e89b-12d3-a456-426614174000/requeue#",
            "x#",
            "../internal/shipments/123e4567-e89b-12d3-a456-426614174000/driver-contact#",
            "",
        ] {
            assert!(path_uuid(&json!({"id": bad}), "id").is_err(), "accepted {bad:?}");
        }
        assert!(path_uuid(&json!({}), "id").is_err());
        assert!(path_uuid(&json!({"id": 5}), "id").is_err());
    }

    fn registry() -> ToolRegistry {
        // Unroutable port: if a refused input ever reached the network, the
        // error text would be a connect error rather than the UUID message.
        let u = "http://127.0.0.1:1".to_string();
        ToolRegistry::new(ServiceUrls {
            dispatch: u.clone(), order_intake: u.clone(), driver_ops: u.clone(),
            payments: u.clone(), engagement: u.clone(), analytics: u.clone(),
            cdp: u.clone(), hub_ops: u.clone(), fleet: u,
        })
    }

    #[tokio::test]
    async fn tools_refuse_path_injection_before_any_request() {
        let reg = registry();
        let cases = [
            ("get_cod_balance", json!({"merchant_id": "123e4567-e89b-12d3-a456-426614174000?tenant_id=victim", "tenant_id": "123e4567-e89b-12d3-a456-426614174000"})),
            ("assign_driver", json!({"shipment_id": "123e4567-e89b-12d3-a456-426614174000/requeue#", "tenant_id": "123e4567-e89b-12d3-a456-426614174000"})),
            ("get_driver_location", json!({"driver_id": "../internal/shipments/123e4567-e89b-12d3-a456-426614174000/driver-contact#"})),
            ("get_shipment", json!({"shipment_id": "x#"})),
            ("get_customer_profile", json!({"customer_id": "../x"})),
        ];
        for (tool, input) in cases {
            let r = reg.execute(tool, input, "t".into(), ToolContext::default()).await;
            assert!(r.is_error, "{tool} should error");
            let msg = r.content["error"].as_str().unwrap_or("");
            assert!(msg.contains("UUID"), "{tool} failed for the wrong reason: {msg}");
        }
    }

    #[test]
    fn response_status_mapping() {
        use reqwest::StatusCode as S;
        assert_eq!(interpret_response(S::OK, br#"{"a":1}"#).unwrap(), json!({"a": 1}));
        assert_eq!(interpret_response(S::NO_CONTENT, b"").unwrap(), Value::Null);
        assert_eq!(interpret_response(S::OK, b"  
").unwrap(), Value::Null);
        for st in [S::UNAUTHORIZED, S::FORBIDDEN, S::UNPROCESSABLE_ENTITY, S::INTERNAL_SERVER_ERROR] {
            let e = interpret_response(st, br#"{"error":"nope"}"#).unwrap_err().to_string();
            assert!(e.contains(&st.as_u16().to_string()) && e.contains("nope"), "{e}");
        }
        let long = "x".repeat(5000);
        let e = interpret_response(S::BAD_REQUEST, long.as_bytes()).unwrap_err().to_string();
        assert!(e.len() < 600, "body not truncated: {}", e.len());
        assert!(interpret_response(S::OK, b"<html>").is_err());
    }
}
