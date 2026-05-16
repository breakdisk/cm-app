use std::time::Instant;
use crate::mcp::context::McpContext;

/// Emits a structured audit trace event at the end of every `tools/call` handler.
pub fn audit_tool_call(ctx: &McpContext, tool: &str, success: bool, start: Instant) {
    tracing::info!(
        event        = "mcp_tool_called",
        tool         = tool,
        actor_uid    = %ctx.actor_uid,
        tenant_id    = %ctx.tenant_id,
        trace_id     = %ctx.trace_id,
        success      = success,
        duration_ms  = start.elapsed().as_millis(),
    );
}
