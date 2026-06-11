use std::sync::Arc;

use serde_json::{json, Value};

use logisticos_auth::rbac::permissions;
use logisticos_types::ContainerId;

use crate::mcp::tools::{req_uuid, to_value};
use crate::mcp::{context::McpContext, McpState};

pub async fn handle(args: &Value, ctx: &McpContext, state: &Arc<McpState>) -> Result<Value, String> {
    if !ctx.has_permission(permissions::SHIPMENT_READ) {
        return Err("permission denied: shipments:read required".into());
    }
    let container_id = req_uuid(args, "container_id")?;
    let (container, manifest) = state
        .hub_transfer_svc
        .get_container_status(ContainerId::from_uuid(container_id))
        .await
        .map_err(|e| format!("failed to load container: {e}"))?;
    Ok(json!({
        "container": to_value(&container)?,
        "manifest":  manifest.map(|m| to_value(&m)).transpose()?,
    }))
}

pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "container_id": { "type": "string", "format": "uuid" }
        },
        "required": ["container_id"]
    })
}
