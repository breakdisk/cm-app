use std::time::Instant;
use crate::mcp::context::McpContext;

/// Call this at the end of every `tools/call` handler.
/// `success`: true if the tool returned a result, false if it returned an error.
/// `start`: the `Instant` captured at the beginning of the handler.
pub fn audit_tool_call(ctx: &McpContext, tool: &str, success: bool, start: Instant) {
    let duration_ms = start.elapsed().as_millis();
    tracing::info!(
        event = "mcp_tool_called",
        tool  = tool,
        actor_uid  = %ctx.actor_uid,
        tenant_id  = %ctx.tenant_id,
        trace_id   = %ctx.trace_id,
        success    = success,
        duration_ms = duration_ms,
    );
}
