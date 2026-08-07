//! The tools mesh agents may call.
//!
//! This is the only place the mesh touches product data. Each tool is a thin,
//! auditable wrapper over an application service — no business logic lives here,
//! so a tool call is always traceable to a service method in the audit log.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use uuid::Uuid;

use logisticos_agent_runtime::tools::{ToolBox, ToolContext, ToolDefinition, ToolResult};

/// What the mesh needs from the host service. A trait rather than a direct
/// dependency on `services/omnideliv` types, so the mesh crate stays testable
/// in isolation and the split seam holds.
#[async_trait]
pub trait MeshCatalog: Send + Sync {
    /// Items matching `query` at `vendor_id`, excluding allergen clashes.
    /// Each hit carries `warrants_substitute`.
    async fn search(
        &self,
        tenant_id: Uuid,
        vendor_id: Uuid,
        query: &str,
        avoid_allergens: &[String],
        limit: i64,
    ) -> anyhow::Result<serde_json::Value>;

    /// Orderable vendors of a vertical near the customer.
    async fn vendors_near(
        &self,
        tenant_id: Uuid,
        vertical: &str,
        lat: f64,
        lng: f64,
        radius_km: f64,
        limit: i64,
    ) -> anyhow::Result<serde_json::Value>;

    /// Courier supply near a point. Backed by field-ops.
    async fn courier_supply(
        &self,
        tenant_id: Uuid,
        lat: f64,
        lng: f64,
        radius_km: f64,
    ) -> anyhow::Result<serde_json::Value>;
}

/// What the mesh needs in order to persist a run's result.
///
/// A trait rather than a dependency on the host service's `BasketService`:
/// the mesh crate is the split seam, and a concrete dependency across it would
/// make the later two-deployable split a refactor again.
#[async_trait]
pub trait MeshBasket: Send + Sync {
    /// Create the basket a run writes into.
    async fn create(&self, tenant_id: Uuid, customer_id: Uuid) -> anyhow::Result<Uuid>;

    /// Persist one specialist's lines. Scoped by sub-intent — this is the
    /// single-writer path, called serially by the Concierge after the join.
    async fn write_delta(
        &self,
        tenant_id: Uuid,
        basket_id: Uuid,
        sub_intent_id: Uuid,
        vertical: &str,
        raw_text: &str,
        lines: Vec<crate::transition::ProposedLine>,
    ) -> anyhow::Result<()>;

    /// How many lines still need a customer decision. Drives `needs_review`.
    async fn lines_awaiting_review(&self, tenant_id: Uuid, basket_id: Uuid) -> anyhow::Result<usize>;
}

pub struct MeshToolBox {
    catalog:   Arc<dyn MeshCatalog>,
    tenant_id: Uuid,
    /// The delivery address for this run. Held here rather than accepted as a
    /// tool argument: the model has no independent knowledge of where the
    /// customer is, so asking it to supply coordinates invites it to invent
    /// them, and a search centred on a hallucinated point fails silently by
    /// returning plausible vendors in the wrong city.
    lat:       f64,
    lng:       f64,
    defs:      Vec<ToolDefinition>,
}

fn def(name: &str, description: &str, schema: serde_json::Value) -> ToolDefinition {
    ToolDefinition {
        name:         name.to_string(),
        description:  description.to_string(),
        input_schema: schema,
    }
}

impl MeshToolBox {
    pub fn new(catalog: Arc<dyn MeshCatalog>, tenant_id: Uuid, lat: f64, lng: f64) -> Self {
        // Every tool any mesh role may call. Per-role filtering happens in the
        // runner via the role's allowlist — a role never sees the others.
        let defs = vec![
            def(
                "decompose_intent",
                "Split the customer's message into one sub-intent per vertical. Call exactly once.",
                json!({
                    "type": "object",
                    "properties": {
                        "sub_intents": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "vertical":    {"type": "string", "enum": ["restaurant","grocery","pharmacy","florist","retail"]},
                                    "vendor_hint": {"type": ["string","null"], "description": "Vendor the customer named, if any"},
                                    "raw_text":    {"type": "string", "description": "The slice of the message this covers"},
                                    "constraints": {"type": "object", "description": "Budget, dietary and timing constraints that apply"}
                                },
                                "required": ["vertical", "raw_text"]
                            }
                        }
                    },
                    "required": ["sub_intents"]
                }),
            ),
            def(
                "find_vendors",
                "Orderable vendors of a vertical near the delivery address, nearest first. Call this \
                 first: every catalog tool needs a vendor_id, and the vendor_hint in your sub-intent \
                 is a name the customer used, not an id. Match the hint against the names returned \
                 here; if nothing matches, the customer named a vendor that is closed or not on the \
                 platform — use the nearest suitable one and say so in your note.",
                json!({
                    "type": "object",
                    "properties": {
                        "vertical":  {"type": "string", "enum": ["restaurant","grocery","pharmacy","florist","retail"]},
                        "radius_km": {"type": "number", "default": 5},
                        "limit":     {"type": "integer", "default": 10}
                    },
                    "required": ["vertical"]
                }),
            ),
            def(
                "search_catalog",
                "Search a vendor's catalog. Each result carries warrants_substitute: when true the \
                 item is out of stock, nearly out, or last confirmed present too long ago to rely on.",
                json!({
                    "type": "object",
                    "properties": {
                        "vendor_id":       {"type": "string"},
                        "query":           {"type": "string"},
                        "avoid_allergens": {"type": "array", "items": {"type": "string"}},
                        "limit":           {"type": "integer", "default": 20}
                    },
                    "required": ["vendor_id", "query"]
                }),
            ),
            def(
                "check_availability",
                "Current availability and freshness for one item.",
                json!({
                    "type": "object",
                    "properties": { "item_id": {"type": "string"} },
                    "required": ["item_id"]
                }),
            ),
            def(
                "propose_substitution",
                "Find replacements for an item that warrants one.",
                json!({
                    "type": "object",
                    "properties": {
                        "vendor_id":       {"type": "string"},
                        "original_item_id":{"type": "string"},
                        "query":           {"type": "string"},
                        "avoid_allergens": {"type": "array", "items": {"type": "string"}}
                    },
                    "required": ["vendor_id", "original_item_id", "query"]
                }),
            ),
            def(
                "propose_lines",
                "Submit the lines for your sub-intent. Call exactly once. An empty list with a \
                 note is correct when nothing satisfies the sub-intent.",
                json!({
                    "type": "object",
                    "properties": {
                        "lines": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "vendor_id":        {"type": "string"},
                                    "item_id":          {"type": "string"},
                                    "qty":              {"type": "integer", "minimum": 1},
                                    "unit_price_cents": {"type": "integer", "minimum": 0},
                                    "substitutes":      {"type": ["string","null"]}
                                },
                                "required": ["vendor_id", "item_id", "qty", "unit_price_cents"]
                            }
                        },
                        "note": {"type": ["string","null"]}
                    },
                    "required": ["lines"]
                }),
            ),
            def(
                "get_available_couriers",
                "Courier supply near the delivery address.",
                json!({
                    "type": "object",
                    "properties": { "radius_km": {"type": "number", "default": 5} }
                }),
            ),
            def(
                "estimate_route",
                "Distance and duration for an ordered list of vendor stops.",
                json!({
                    "type": "object",
                    "properties": { "vendor_ids": {"type": "array", "items": {"type": "string"}} },
                    "required": ["vendor_ids"]
                }),
            ),
            def(
                "compute_flat_fee",
                "The single delivery fee for a route. Flat regardless of stop count.",
                json!({
                    "type": "object",
                    "properties": { "distance_km": {"type": "number"} },
                    "required": ["distance_km"]
                }),
            ),
            def(
                "plan_route",
                "Submit the pickup sequence and flat fee. Call exactly once.",
                json!({
                    "type": "object",
                    "properties": {
                        "vendor_order":   {"type": "array", "items": {"type": "string"}},
                        "flat_fee_cents": {"type": "integer", "minimum": 0},
                        "total_minutes":  {"type": "integer", "minimum": 0}
                    },
                    "required": ["vendor_order", "flat_fee_cents", "total_minutes"]
                }),
            ),
            def(
                "get_customer_profile",
                "Dietary tags, allergens and taste preferences for the current customer.",
                json!({ "type": "object", "properties": {} }),
            ),
            def(
                "present_bundle",
                "Hand the assembled bundle to the customer for review.",
                json!({
                    "type": "object",
                    "properties": { "summary": {"type": "string"} },
                    "required": ["summary"]
                }),
            ),
        ];

        Self { catalog, tenant_id, lat, lng, defs }
    }
}

/// Tools whose call *is* the agent's answer. Executing one is a no-op
/// acknowledgement — the runner reads the arguments off the recorded action and
/// turns them into a `MeshTransition`. The value is that the handoff is
/// schema-validated and audited rather than parsed out of prose.
pub const TERMINAL_TOOLS: [&str; 4] =
    ["decompose_intent", "propose_lines", "plan_route", "present_bundle"];

#[async_trait]
impl ToolBox for MeshToolBox {
    fn definitions(&self) -> &[ToolDefinition] { &self.defs }

    async fn execute(
        &self,
        name: String,
        input: serde_json::Value,
        tool_use_id: String,
        _ctx: ToolContext,
    ) -> ToolResult {
        let ok = |content: serde_json::Value| ToolResult {
            tool_use_id: tool_use_id.clone(),
            content,
            is_error: false,
        };
        let err = |msg: String| ToolResult {
            tool_use_id: tool_use_id.clone(),
            content: json!({ "error": msg }),
            is_error: true,
        };

        if TERMINAL_TOOLS.contains(&name.as_str()) {
            return ok(json!({ "accepted": true }));
        }

        let parse_uuid = |v: Option<&serde_json::Value>, field: &str| -> Result<Uuid, String> {
            v.and_then(|x| x.as_str())
                .ok_or_else(|| format!("{field} is required"))
                .and_then(|s| Uuid::parse_str(s).map_err(|_| format!("{field} is not a uuid")))
        };

        match name.as_str() {
            "find_vendors" => {
                let Some(vertical) = input.get("vertical").and_then(|v| v.as_str()) else {
                    return err("vertical is required".into());
                };
                let radius = input.get("radius_km").and_then(|v| v.as_f64()).unwrap_or(5.0);
                let limit = input.get("limit").and_then(|v| v.as_i64()).unwrap_or(10);

                match self
                    .catalog
                    .vendors_near(self.tenant_id, vertical, self.lat, self.lng, radius, limit)
                    .await
                {
                    Ok(v) => ok(v),
                    Err(e) => err(format!("vendor lookup failed: {e}")),
                }
            }

            "search_catalog" | "propose_substitution" => {
                let vendor_id = match parse_uuid(input.get("vendor_id"), "vendor_id") {
                    Ok(v) => v,
                    Err(e) => return err(e),
                };
                let query = input.get("query").and_then(|v| v.as_str()).unwrap_or("");
                let avoid: Vec<String> = input
                    .get("avoid_allergens")
                    .and_then(|v| v.as_array())
                    .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect())
                    .unwrap_or_default();
                let limit = input.get("limit").and_then(|v| v.as_i64()).unwrap_or(20);

                match self.catalog.search(self.tenant_id, vendor_id, query, &avoid, limit).await {
                    Ok(v) => ok(v),
                    Err(e) => err(format!("catalog search failed: {e}")),
                }
            }

            "get_available_couriers" => {
                let radius = input.get("radius_km").and_then(|v| v.as_f64()).unwrap_or(5.0);
                match self.catalog.courier_supply(self.tenant_id, self.lat, self.lng, radius).await {
                    Ok(v) => ok(v),
                    Err(e) => err(format!("courier supply lookup failed: {e}")),
                }
            }

            // Deterministic tools — no service call needed.
            "estimate_route" => {
                let stops = input.get("vendor_ids").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
                ok(json!({ "stops": stops, "note": "distance is computed at consolidation time" }))
            }
            "compute_flat_fee" => {
                let km = input.get("distance_km").and_then(|v| v.as_f64()).unwrap_or(0.0);
                // Placeholder tariff until Plan 5 owns pricing. Deliberately
                // simple and visible rather than hidden behind a stub service.
                let fee = 4_900 + (km.max(0.0) * 600.0) as i64;
                ok(json!({ "flat_fee_cents": fee }))
            }
            "check_availability" => {
                match parse_uuid(input.get("item_id"), "item_id") {
                    Ok(_) => ok(json!({ "note": "use search_catalog — it returns availability inline" })),
                    Err(e) => err(e),
                }
            }
            "get_customer_profile" => {
                // Constraints are passed into each sub-intent by the Concierge;
                // specialists do not get PII access. Plan 4 leaves this to the
                // Concierge only, and it returns what the CDP extension provides.
                ok(json!({ "dietary_tags": [], "allergens": [], "taste_preferences": [] }))
            }

            other => err(format!("unknown tool: {other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct SpyCatalog {
        calls: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl MeshCatalog for SpyCatalog {
        async fn search(&self, _: Uuid, vendor: Uuid, q: &str, _: &[String], _: i64)
            -> anyhow::Result<serde_json::Value> {
            self.calls.lock().unwrap().push(format!("search:{vendor}:{q}"));
            Ok(json!({ "items": [] }))
        }
        async fn vendors_near(&self, _: Uuid, vertical: &str, lat: f64, lng: f64, _: f64, _: i64)
            -> anyhow::Result<serde_json::Value> {
            self.calls.lock().unwrap().push(format!("vendors:{vertical}:{lat}:{lng}"));
            Ok(json!({ "vendors": [] }))
        }
        async fn courier_supply(&self, _: Uuid, lat: f64, lng: f64, _: f64)
            -> anyhow::Result<serde_json::Value> {
            self.calls.lock().unwrap().push(format!("couriers:{lat}:{lng}"));
            Ok(json!({ "available": 0 }))
        }
    }

    fn toolbox() -> (MeshToolBox, Arc<SpyCatalog>) {
        let spy = Arc::new(SpyCatalog::default());
        (MeshToolBox::new(spy.clone(), Uuid::new_v4(), 14.5995, 120.9842), spy)
    }

    async fn call(tb: &MeshToolBox, name: &str, input: serde_json::Value) -> ToolResult {
        tb.execute(name.to_string(), input, "tu_1".into(), ToolContext::default()).await
    }

    /// The gap this tool closes: a specialist is handed `vendor_hint`, a name,
    /// while every catalog tool needs a `vendor_id`. Without find_vendors the
    /// Nutritionist cannot search at all, and its empty result would look
    /// exactly like honest degradation.
    #[tokio::test]
    async fn find_vendors_resolves_a_vertical_at_the_delivery_address() {
        let (tb, spy) = toolbox();
        let r = call(&tb, "find_vendors", json!({ "vertical": "grocery" })).await;

        assert!(!r.is_error);
        assert_eq!(spy.calls.lock().unwrap().as_slice(), ["vendors:grocery:14.5995:120.9842"]);
    }

    /// Location is construction-time state, not a tool argument. The model has
    /// no independent knowledge of where the customer is, so letting it supply
    /// coordinates invites a confident search around the wrong point.
    #[tokio::test]
    async fn location_arguments_from_the_model_are_ignored() {
        let (tb, spy) = toolbox();
        let _ = call(&tb, "find_vendors", json!({ "vertical": "grocery", "lat": 0.0, "lng": 0.0 })).await;
        let _ = call(&tb, "get_available_couriers", json!({ "lat": 51.5, "lng": -0.12 })).await;

        let calls = spy.calls.lock().unwrap();
        assert_eq!(calls[0], "vendors:grocery:14.5995:120.9842");
        assert_eq!(calls[1], "couriers:14.5995:120.9842");
    }

    /// Terminal tools are the agent's answer, not a request for data. They
    /// acknowledge without touching the catalog.
    #[tokio::test]
    async fn terminal_tools_acknowledge_without_calling_the_catalog() {
        let (tb, spy) = toolbox();
        for t in TERMINAL_TOOLS {
            let r = call(&tb, t, json!({})).await;
            assert!(!r.is_error, "{t} must not error");
            assert_eq!(r.content, json!({ "accepted": true }));
        }
        assert!(spy.calls.lock().unwrap().is_empty(), "terminal tools must not hit the catalog");
    }

    #[tokio::test]
    async fn a_malformed_vendor_id_is_an_error_not_a_panic() {
        let (tb, spy) = toolbox();
        let r = call(&tb, "search_catalog", json!({ "vendor_id": "not-a-uuid", "query": "eggs" })).await;

        assert!(r.is_error);
        assert!(spy.calls.lock().unwrap().is_empty(), "a bad argument must not reach the catalog");
    }

    #[tokio::test]
    async fn an_unknown_tool_is_an_error() {
        let (tb, _) = toolbox();
        assert!(call(&tb, "improvise", json!({})).await.is_error);
    }

    /// Every tool named in a role's allowlist must exist in the box, or that
    /// role holds authority over something unreachable — the kind of drift that
    /// only shows up as an agent looping on a tool it was promised.
    #[test]
    fn every_tool_a_role_may_call_exists() {
        let (tb, _) = toolbox();
        let names: Vec<&str> = tb.definitions().iter().map(|d| d.name.as_str()).collect();

        for role in [crate::roles::concierge(), crate::roles::nutritionist(), crate::roles::fleet()] {
            for tool in role.allowed_tools().expect("restricted") {
                assert!(names.contains(&tool.as_str()), "{} may call {tool}, which does not exist", role.key());
            }
        }
    }
}
