pub mod assign_piece_to_pallet;
pub mod get_container_status;
pub mod get_customs_queue;
pub mod get_hub_inventory;
pub mod trigger_deconsolidation;

use std::sync::Arc;
use std::time::Instant;

use serde_json::{json, Value};

use crate::mcp::{audit::audit_tool_call, context::McpContext, McpState};

/// Dispatches a `tools/call` to the correct hub-ops handler, auditing every call.
pub async fn dispatch(
    name:  &str,
    args:  &Value,
    ctx:   &McpContext,
    state: &Arc<McpState>,
) -> Result<Value, String> {
    let start = Instant::now();
    let result = match name {
        "get_hub_inventory"      => get_hub_inventory::handle(args, ctx, state).await,
        "get_container_status"   => get_container_status::handle(args, ctx, state).await,
        "get_customs_queue"      => get_customs_queue::handle(args, ctx, state).await,
        "assign_piece_to_pallet" => assign_piece_to_pallet::handle(args, ctx, state).await,
        "trigger_deconsolidation"=> trigger_deconsolidation::handle(args, ctx, state).await,
        other => Err(format!("Unknown tool: {other}")),
    };
    audit_tool_call(ctx, name, result.is_ok(), start);
    result
}

/// Returns the `tools/list` schema array.
pub fn list() -> Value {
    json!([
        { "name": "get_hub_inventory",
          "description": "Current HubInventory at a warehouse location.",
          "inputSchema": get_hub_inventory::schema() },
        { "name": "get_container_status",
          "description": "Container state + its customs/freight transfer manifest.",
          "inputSchema": get_container_status::schema() },
        { "name": "get_customs_queue",
          "description": "Containers currently under customs hold at a hub.",
          "inputSchema": get_customs_queue::schema() },
        { "name": "assign_piece_to_pallet",
          "description": "Record a PalletAssign scan binding a piece to a pallet (chain of custody).",
          "inputSchema": assign_piece_to_pallet::schema() },
        { "name": "trigger_deconsolidation",
          "description": "Break a Released container into pieces and fan out last-mile routing.",
          "inputSchema": trigger_deconsolidation::schema() },
    ])
}

/// Shared helper: parse a required UUID argument.
pub(crate) fn req_uuid(args: &Value, key: &str) -> Result<uuid::Uuid, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("missing required argument '{key}'"))?
        .parse()
        .map_err(|_| format!("argument '{key}' is not a valid UUID"))
}

/// Shared helper: serialise a domain value to JSON.
pub(crate) fn to_value<T: serde::Serialize>(v: &T) -> Result<Value, String> {
    serde_json::to_value(v).map_err(|e| format!("serialization error: {e}"))
}
