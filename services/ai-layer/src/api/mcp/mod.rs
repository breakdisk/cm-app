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

use logisticos_auth::claims::Claims;
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
            AgentType::OnDemand,
            json!({
                "source":    "remote_mcp",
                "user_id":   claims.user_id,
                "email":     claims.email,
                "client_ip": client_ip,
            }),
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
        Self::require_enterprise_mcp(&context)?;

        let tools = self
            .tools
            .definitions()
            .iter()
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
