//! Remote MCP (Model Context Protocol) server.
//!
//! Exposes the existing `ToolRegistry` (see `infrastructure::tools`) over the
//! MCP Streamable HTTP transport so external MCP clients — an Enterprise
//! tenant's own AI workflows, Claude Desktop's remote-server connector, etc. —
//! can call LogisticOS operational tools directly, per ADR-0004's Enterprise
//! Extension. This is the same tool registry the Python LangGraph sidecar
//! already calls via `/internal/tools/execute`; this module is a second,
//! externally-reachable transport in front of it, not a second tool system.
//!
//! Mounted at `/mcp` inside the same router as `/v1/agents/*`, so it inherits
//! the `require_auth` JWT middleware applied in `bootstrap.rs`. That
//! middleware inserts `Claims` into the request's extensions *before* this
//! service ever sees the request; `RequestContext::extensions` carries the
//! raw `axum::http::request::Parts` for every MCP call (confirmed via the
//! rmcp SDK's own streamable-http examples), which is where we read them
//! back out.
use std::sync::Arc;

use rmcp::{
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, Implementation, ListToolsResult,
        PaginatedRequestParams, ProtocolVersion, ServerCapabilities, ServerInfo, Tool,
    },
    service::RequestContext,
    transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    },
    ErrorData as McpError, RoleServer, ServerHandler,
};
use serde_json::{json, Value};

use logisticos_agent_runtime::tools::ToolDefinition;
use logisticos_auth::{claims::Claims, rbac::permissions};
use logisticos_types::TenantId;

use crate::{
    domain::entities::{AgentAction, AgentSession, AgentType},
    infrastructure::{
        db::SessionRepository,
        tools::{ToolContext, ToolRegistry, ToolResult},
    },
};

/// Pricing-feature-matrix key gating remote MCP access (already defined in
/// `Claims::has_feature` / `identity` migration `0016_pricing_feature_matrix.sql`;
/// this is the first consumer of it).
const ENTERPRISE_MCP_FEATURE: &str = "enterprise_mcp";

// ── Per-tool access control ────────────────────────────────────────────────
//
// The `enterprise_mcp` feature only says the *tenant* may use MCP. What a given
// caller may do is decided per tool by the permissions on their own token, the
// same permissions the downstream service would demand. One table, one
// function (`mcp_tool_access`), used by both `list_tools` and `call_tool`.

/// Who may call one tool over MCP.
pub struct McpToolRule {
    pub tool: &'static str,
    /// The caller needs at least one of these (`*` satisfies any).
    pub any_of: &'static [&'static str],
    /// The tool reaches an unauthenticated `/v1/internal/*` route that cannot
    /// tell callers apart, so a partner-scoped token must never get there.
    pub internal_route: bool,
}

pub const MCP_TOOL_RULES: &[McpToolRule] = &[
    McpToolRule { tool: "get_shipment",             any_of: &[permissions::SHIPMENT_READ],      internal_route: false },
    McpToolRule { tool: "reschedule_delivery",      any_of: &[permissions::SHIPMENT_UPDATE],    internal_route: false },
    McpToolRule { tool: "get_available_drivers",    any_of: &[permissions::DISPATCH_VIEW],      internal_route: true  },
    McpToolRule { tool: "assign_driver",            any_of: &[permissions::DISPATCH_ASSIGN],    internal_route: true  },
    McpToolRule { tool: "send_notification",        any_of: &[permissions::ENGAGEMENT_SEND],    internal_route: false },
    McpToolRule { tool: "get_delivery_metrics",     any_of: &[permissions::ANALYTICS_VIEW],     internal_route: false },
    McpToolRule { tool: "get_driver_performance",   any_of: &[permissions::ANALYTICS_VIEW],     internal_route: false },
    McpToolRule { tool: "reconcile_cod",            any_of: &[permissions::PAYMENTS_RECONCILE], internal_route: false },
    // payments' `generate_invoice` handler requires BILLING_MANAGE.
    McpToolRule { tool: "generate_invoice",         any_of: &[permissions::BILLING_MANAGE],     internal_route: false },
    McpToolRule { tool: "get_cod_balance",          any_of: &[permissions::PAYMENTS_READ],      internal_route: true  },
    McpToolRule { tool: "get_customer_profile",     any_of: &[permissions::CUSTOMERS_VIEW],     internal_route: false },
    McpToolRule { tool: "get_churn_score",          any_of: &[permissions::CUSTOMERS_VIEW],     internal_route: false },
    McpToolRule { tool: "get_customer_preferences", any_of: &[permissions::CUSTOMERS_VIEW],     internal_route: false },
    McpToolRule { tool: "get_hub_capacity",         any_of: &[permissions::FLEET_READ],         internal_route: false },
    McpToolRule { tool: "get_driver_location",      any_of: &[permissions::FLEET_READ],         internal_route: false },
    McpToolRule { tool: "send_driver_instruction",  any_of: &[permissions::FLEET_MANAGE],       internal_route: false },
    McpToolRule { tool: "get_vehicle_status",       any_of: &[permissions::FLEET_READ],         internal_route: false },
    McpToolRule { tool: "get_fleet_availability",   any_of: &[permissions::FLEET_READ],         internal_route: false },
];

/// Registered tools deliberately NOT exposed over MCP. Every registry tool must
/// be in `MCP_TOOL_RULES` or here (enforced by a test), so adding a tool forces
/// a decision. Anything in neither is fail-closed: unlisted and uncallable.
pub const MCP_HIDDEN: &[&str] = &[
    // No downstream route exists.
    "get_zone_demand_forecast",
    "schedule_dock",
    // Support-agent loop marker; meaningless to a remote caller.
    "escalate_to_human",
];

/// Why a caller may not use a tool.
#[derive(Debug, PartialEq, Eq)]
pub enum Denied {
    /// Not in the table (hidden, unknown, or newly added without a decision).
    NotExposed { tool: String },
    /// Lacks every permission in `any_of`.
    MissingPermission { tool: &'static str, any_of: &'static [&'static str] },
    /// Partner-scoped token against a tool that reaches an unscoped internal route.
    PartnerScoped { tool: &'static str },
}

impl Denied {
    pub fn message(&self) -> String {
        match self {
            Denied::NotExposed { tool } => format!("Tool '{tool}' is not available over MCP"),
            Denied::MissingPermission { tool, any_of } => format!(
                "Tool '{tool}' requires one of the permissions: {}",
                any_of.join(", ")
            ),
            Denied::PartnerScoped { tool } => {
                format!("Tool '{tool}' is not available to partner-scoped tokens")
            }
        }
    }
}

/// The single access decision for `tool`. Used by `list_tools` and `call_tool`.
pub fn mcp_tool_access(claims: &Claims, tool: &str) -> Result<(), Denied> {
    let rule = MCP_TOOL_RULES
        .iter()
        .find(|r| r.tool == tool)
        .ok_or_else(|| Denied::NotExposed { tool: tool.to_string() })?;

    if !rule.any_of.iter().any(|p| claims.has_permission(p)) {
        return Err(Denied::MissingPermission { tool: rule.tool, any_of: rule.any_of });
    }
    if rule.internal_route && claims.pid.is_some() {
        return Err(Denied::PartnerScoped { tool: rule.tool });
    }
    Ok(())
}

/// The definitions this caller may see: exactly those `mcp_tool_access` allows.
fn accessible_definitions<'a>(claims: &Claims, defs: &'a [ToolDefinition]) -> Vec<&'a ToolDefinition> {
    defs.iter().filter(|d| mcp_tool_access(claims, &d.name).is_ok()).collect()
}

#[derive(Clone)]
pub struct LogisticOsMcpServer {
    tools:        Arc<ToolRegistry>,
    session_repo: Arc<dyn SessionRepository>,
}

impl LogisticOsMcpServer {
    pub fn new(tools: Arc<ToolRegistry>, session_repo: Arc<dyn SessionRepository>) -> Self {
        Self { tools, session_repo }
    }

    /// Persist this tool call as an `AgentAction` on a single-action
    /// `AgentSession`, the same audit mechanism the internal LangGraph-driven
    /// tool path relies on (`AgentAction` is documented as "immutable audit
    /// log entry for each tool call") — so remote MCP calls show up in the
    /// same AI Agents dashboard / session history as everything else,
    /// instead of only a tracing log line.
    ///
    /// Captures actor (user_id/email), tenant, timestamp, and client IP —
    /// the four fields CLAUDE.md's audit-logging non-negotiable requires.
    async fn record_audit(
        &self,
        claims: &Claims,
        client_ip: Option<&str>,
        tool_name: &str,
        input: &Value,
        result: &ToolResult,
    ) {
        let mut session = AgentSession::new(
            TenantId::from_uuid(claims.tenant_id),
            AgentType::OnDemand.into(),
            json!({
                "source":    "remote_mcp",
                "user_id":   claims.user_id,
                "email":     claims.email,
                "client_ip": client_ip,
            }),
            // A remote MCP call invokes a tool directly — no model is in the
            // loop, so recording one would misattribute the action to an LLM.
            "none",
        );

        let mut action = AgentAction::new(session.id, tool_name.to_string(), input.clone());
        action.tool_result = Some(result.content.clone());
        action.succeeded = !result.is_error;
        session.actions.push(action);

        let confidence = if result.is_error { 0.0 } else { 1.0 };
        session.complete(format!("Remote MCP tool call: {tool_name}"), confidence);

        if let Err(err) = self.session_repo.save(&session).await {
            tracing::error!(error = %err, tool = %tool_name, tenant_id = %claims.tenant_id, "failed to persist remote MCP audit record");
        }
    }

    /// Pull the validated JWT claims the `require_auth` middleware attached
    /// to this request before it reached the MCP transport.
    fn claims_from(context: &RequestContext<RoleServer>) -> Option<Claims> {
        context
            .extensions
            .get::<axum::http::request::Parts>()
            .and_then(|parts| parts.extensions.get::<Claims>())
            .cloned()
    }

    /// Read the caller's raw bearer token back off the request so tool calls can
    /// be made on their behalf. Same source as `claims_from` — the token has
    /// already been validated by `require_auth` before reaching this point.
    fn bearer_from(context: &RequestContext<RoleServer>) -> Option<String> {
        context
            .extensions
            .get::<axum::http::request::Parts>()
            .and_then(|parts| parts.headers.get(axum::http::header::AUTHORIZATION))
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::to_owned)
    }

    /// Read the caller's IP from `X-Forwarded-For`, which the API Gateway now
    /// stamps with the connecting peer's address (appending to any existing
    /// value rather than overwriting). Takes the first hop — the original
    /// client, per standard XFF ordering — trimming whitespace.
    fn client_ip_from(context: &RequestContext<RoleServer>) -> Option<String> {
        context
            .extensions
            .get::<axum::http::request::Parts>()
            .and_then(|parts| parts.headers.get("x-forwarded-for"))
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(',').next())
            .map(|ip| ip.trim().to_string())
    }

    fn require_enterprise_mcp(context: &RequestContext<RoleServer>) -> Result<Claims, McpError> {
        let claims = Self::claims_from(context).ok_or_else(|| {
            McpError::invalid_request(
                "Missing LogisticOS session — connect through the API Gateway with a valid Bearer token",
                None,
            )
        })?;

        if !claims.has_feature(ENTERPRISE_MCP_FEATURE) {
            return Err(McpError::invalid_request(
                "Remote MCP access requires an Enterprise-tier subscription",
                None,
            ));
        }

        Ok(claims)
    }
}

impl ServerHandler for LogisticOsMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::from_build_env())
            .with_protocol_version(ProtocolVersion::V_2024_11_05)
            .with_instructions(
                "LogisticOS operational tools: dispatch, shipments, notifications, analytics, \
                 payments, fleet, and hub operations. Requires an Enterprise-tier LogisticOS \
                 session token."
                    .to_string(),
            )
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let claims = Self::require_enterprise_mcp(&context)?;

        let tools = accessible_definitions(&claims, self.tools.definitions())
            .into_iter()
            .map(|def| {
                let schema = def.input_schema.as_object().cloned().unwrap_or_default();
                Tool::new(def.name.clone(), def.description.clone(), schema)
            })
            .collect();

        Ok(ListToolsResult::with_all_items(tools))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let claims = Self::require_enterprise_mcp(&context)?;
        let client_ip = Self::client_ip_from(&context);

        // Per-tool authorisation, before anything executes. A refusal is
        // audited as a failed action so probing shows up in session history.
        if let Err(denied) = mcp_tool_access(&claims, &request.name) {
            let message = denied.message();
            let refusal = ToolResult {
                tool_use_id: uuid::Uuid::new_v4().to_string(),
                content:     json!({ "error": message, "denied": true }),
                is_error:    true,
            };
            let attempted = request
                .arguments
                .clone()
                .map(Value::Object)
                .unwrap_or_else(|| Value::Object(Default::default()));
            self.record_audit(&claims, client_ip.as_deref(), &request.name, &attempted, &refusal)
                .await;
            return Err(McpError::invalid_request(message, None));
        }

        // Tenant scoping always comes from the validated JWT, never from
        // caller-supplied arguments — a remote MCP client must not be able to
        // pass an arbitrary tenant_id and reach another tenant's data.
        let mut input = request
            .arguments
            .map(Value::Object)
            .unwrap_or_else(|| Value::Object(Default::default()));
        if let Value::Object(map) = &mut input {
            map.insert(
                "tenant_id".to_string(),
                Value::String(claims.tenant_id.to_string()),
            );
        }

        // Remote MCP callers arrive with their own LogisticOS bearer token —
        // propagate it so tools that hit JWT-protected service endpoints act
        // under that caller's authority rather than anonymously.
        let ctx = Self::bearer_from(&context)
            .map(ToolContext::with_bearer)
            .unwrap_or_default();

        let tool_use_id = uuid::Uuid::new_v4().to_string();
        let result = self
            .tools
            .execute(&request.name, input.clone(), tool_use_id.clone(), ctx)
            .await;

        self.record_audit(&claims, client_ip.as_deref(), &request.name, &input, &result)
            .await;

        let response = if result.is_error {
            CallToolResult::structured_error(result.content)
        } else {
            CallToolResult::structured(result.content)
        };

        Ok(response.into())
    }
}

/// Build the Tower service mounted at `/mcp`. Stateless Streamable HTTP — no
/// session pinning — so it rolls cleanly under Istio traffic-splitting like
/// every other route on this service.
pub fn streamable_http_service(
    tools: Arc<ToolRegistry>,
    session_repo: Arc<dyn SessionRepository>,
) -> StreamableHttpService<LogisticOsMcpServer, LocalSessionManager> {
    StreamableHttpService::new(
        move || Ok(LogisticOsMcpServer::new(tools.clone(), session_repo.clone())),
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::tools::ServiceUrls;
    use logisticos_auth::rbac::default_permissions_for_role;
    use uuid::Uuid;

    const ROLES: &[&str] = &[
        "admin", "dispatcher", "merchant", "driver", "finance", "readonly", "customer",
        "partner", "tenant_admin", "hub_scanner", "store_manager", "store_clerk",
        "no_such_role",
    ];

    fn claims(perms: Vec<String>) -> Claims {
        Claims::new(
            Uuid::new_v4(), Uuid::new_v4(), "t".into(), "enterprise".into(),
            "u@example.com".into(), vec![], perms, 60,
        )
    }

    fn role_claims(role: &str) -> Claims {
        claims(default_permissions_for_role(role).iter().map(|p| p.to_string()).collect())
    }

    fn registry() -> ToolRegistry {
        let u = || "http://localhost".to_string();
        ToolRegistry::new(ServiceUrls {
            dispatch: u(), order_intake: u(), driver_ops: u(), payments: u(),
            engagement: u(), analytics: u(), cdp: u(), hub_ops: u(), fleet: u(),
        })
    }

    #[test]
    fn every_registered_tool_is_either_ruled_or_hidden() {
        let reg = registry();
        for def in reg.definitions() {
            let ruled = MCP_TOOL_RULES.iter().any(|r| r.tool == def.name);
            let hidden = MCP_HIDDEN.contains(&def.name.as_str());
            assert!(
                ruled ^ hidden,
                "tool '{}' must be in exactly one of MCP_TOOL_RULES / MCP_HIDDEN",
                def.name
            );
        }
        for r in MCP_TOOL_RULES {
            assert!(reg.definitions().iter().any(|d| d.name == r.tool), "{} not registered", r.tool);
        }
        for h in MCP_HIDDEN {
            assert!(reg.definitions().iter().any(|d| d.name == *h), "{h} not registered");
        }
    }

    #[test]
    fn merchant_cannot_assign_invoice_or_reconcile() {
        let c = role_claims("merchant");
        for t in ["assign_driver", "generate_invoice", "reconcile_cod"] {
            assert!(mcp_tool_access(&c, t).is_err(), "merchant reached {t}");
        }
    }

    #[test]
    fn partner_scoped_admin_is_refused_internal_route_tools_only() {
        let c = role_claims("admin").with_partner(Some(Uuid::new_v4()));
        let internal: Vec<&McpToolRule> = MCP_TOOL_RULES.iter().filter(|r| r.internal_route).collect();
        assert_eq!(internal.len(), 3);
        for r in internal {
            assert_eq!(mcp_tool_access(&c, r.tool), Err(Denied::PartnerScoped { tool: r.tool }));
        }
        assert!(mcp_tool_access(&c, "get_shipment").is_ok());
    }

    #[test]
    fn wildcard_reaches_every_ruled_tool_and_no_hidden_one() {
        let c = claims(vec!["*".into()]);
        for r in MCP_TOOL_RULES {
            assert!(mcp_tool_access(&c, r.tool).is_ok(), "* refused {}", r.tool);
        }
        for h in MCP_HIDDEN {
            assert_eq!(mcp_tool_access(&c, h), Err(Denied::NotExposed { tool: h.to_string() }));
        }
        assert!(mcp_tool_access(&c, "no_such_tool").is_err());
    }

    #[test]
    fn no_permissions_reaches_nothing() {
        let c = claims(vec![]);
        for r in MCP_TOOL_RULES {
            assert!(mcp_tool_access(&c, r.tool).is_err());
        }
    }

    #[test]
    fn refusal_message_names_tool_and_permission() {
        let c = role_claims("merchant");
        let msg = mcp_tool_access(&c, "assign_driver").unwrap_err().message();
        assert!(msg.contains("assign_driver") && msg.contains("dispatch:assign"), "{msg}");
    }

    #[test]
    fn list_and_call_agree_for_every_role_and_tool() {
        let reg = registry();
        let defs = reg.definitions();
        for role in ROLES {
            for pid in [None, Some(Uuid::new_v4())] {
                let c = role_claims(role).with_partner(pid);
                let listed: Vec<&str> =
                    accessible_definitions(&c, defs).iter().map(|d| d.name.as_str()).collect();
                for def in defs {
                    assert_eq!(
                        listed.contains(&def.name.as_str()),
                        mcp_tool_access(&c, &def.name).is_ok(),
                        "list/call disagree for role {role} tool {}",
                        def.name
                    );
                }
            }
        }
    }
}
