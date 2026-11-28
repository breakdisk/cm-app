use std::time::Instant;
use super::context::McpContext;

pub fn audit_tool_call(ctx: &McpContext, tool: &str, success: bool, start: Instant) {
    tracing::info!(
        event       = "mcp_tool_called",
        tool        = tool,
        actor_uid   = %ctx.actor_uid,
        tenant_id   = %ctx.tenant_id,
        trace_id    = %ctx.trace_id,
        success     = success,
        duration_ms = start.elapsed().as_millis(),
    );
}

/// A call refused before it ran. Logged at `warn` so a probing caller stands
/// out from ordinary failures. `tool` is bounded: it is attacker-supplied.
pub fn audit_tool_denied(ctx: &McpContext, tool: &str, reason: &str) {
    let tool: String = tool.chars().take(64).collect();
    tracing::warn!(
        event     = "mcp_tool_denied",
        tool      = %tool,
        actor_uid = %ctx.actor_uid,
        tenant_id = %ctx.tenant_id,
        trace_id  = %ctx.trace_id,
        reason    = reason,
    );
}
