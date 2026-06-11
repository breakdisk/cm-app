use std::sync::Arc;

use serde_json::{json, Value};

use logisticos_auth::rbac::permissions;

use crate::mcp::tools::{req_uuid, to_value};
use crate::mcp::{context::McpContext, McpState};

pub async fn handle(args: &Value, ctx: &McpContext, state: &Arc<McpState>) -> Result<Value, String> {
    if !ctx.has_permission(permissions::SHIPMENT_READ) {
        return Err("permission denied: shipments:read required".into());
    }
    let location_id = req_uuid(args, "location_id")?;
    let items = state
        .hub_transfer_svc
        .list_inventory_at(location_id)
        .await
        .map_err(|e| format!("failed to list inventory: {e}"))?;
    Ok(json!({ "location_id": location_id, "inventory": to_value(&items)? }))
}

pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "location_id": { "type": "string", "format": "uuid",
                "description": "HubLocation id to list inventory for" }
        },
        "required": ["location_id"]
    })
}
