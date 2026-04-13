pub mod get_available_drivers;
// remaining tools added in Tasks 4–7

use serde_json::Value;
use std::sync::Arc;
use crate::api::http::AppState;
use crate::mcp::context::McpContext;

/// Dispatches a `tools/call` to the correct handler.
pub async fn dispatch(
    name: &str,
    args: &Value,
    ctx: &McpContext,
    state: &Arc<AppState>,
) -> Result<Value, String> {
    match name {
        "get_available_drivers" => get_available_drivers::handle(args, ctx, state).await,
        other => Err(format!("Unknown tool: {other}")),
    }
}

/// Returns the `tools/list` schema array.
pub fn list() -> Value {
    serde_json::json!([
        {
            "name": "get_available_drivers",
            "description": "List drivers currently available for assignment. Optionally filter by zone and vehicle type.",
            "inputSchema": get_available_drivers::schema()
        }
    ])
}
