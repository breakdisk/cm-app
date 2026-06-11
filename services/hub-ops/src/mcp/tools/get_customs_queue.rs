use std::sync::Arc;

use serde_json::{json, Value};

use logisticos_auth::rbac::permissions;

use crate::mcp::tools::{req_uuid, to_value};
use crate::mcp::{context::McpContext, McpState};

pub async fn handle(args: &Value, ctx: &McpContext, state: &Arc<McpState>) -> Result<Value, String> {
    if !ctx.has_permission(permissions::SHIPMENT_READ) {
        return Err("permission denied: shipments:read required".into());
    }
    let hub_id = req_uuid(args, "hub_id")?;
    let queue = state
        .hub_transfer_svc
        .list_customs_queue(hub_id, ctx.tenant_id)
        .await
        .map_err(|e| format!("failed to load customs queue: {e}"))?;
    Ok(json!({ "hub_id": hub_id, "containers": to_value(&queue)? }))
}

pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "hub_id": { "type": "string", "format": "uuid" }
        },
        "required": ["hub_id"]
    })
}
