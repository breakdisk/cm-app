use std::sync::Arc;

use serde_json::{json, Value};

use logisticos_auth::rbac::permissions;
use logisticos_types::ContainerId;

use crate::application::services::DeconsolidateCommand;
use crate::mcp::tools::req_uuid;
use crate::mcp::{context::McpContext, McpState};

pub async fn handle(args: &Value, ctx: &McpContext, state: &Arc<McpState>) -> Result<Value, String> {
    if !ctx.has_permission(permissions::SHIPMENT_UPDATE) {
        return Err("permission denied: shipments:update required".into());
    }
    let container_id = req_uuid(args, "container_id")?;
    let destination_zone = args
        .get("destination_zone")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    let container = state
        .hub_transfer_svc
        .deconsolidate(DeconsolidateCommand {
            container_id:     ContainerId::from_uuid(container_id),
            destination_zone,
            // Routing fan-out shipments/enrichment are supplied via the HTTP
            // deconsolidate path; an agent-triggered deconsolidation records the
            // container break-bulk and emits the deconsolidated event.
            shipment_ids:     Vec::new(),
            service_level:    String::new(),
            sla_hours:        0,
        })
        .await
        .map_err(|e| format!("deconsolidation failed: {e}"))?;

    Ok(json!({ "container_id": container_id, "status": format!("{:?}", container.status) }))
}

pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "container_id":     { "type": "string", "format": "uuid" },
            "destination_zone": { "type": "string", "description": "Optional last-mile zone for routing" }
        },
        "required": ["container_id"]
    })
}
