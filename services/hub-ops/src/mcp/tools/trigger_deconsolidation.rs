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

    let service_level = args
        .get("service_level")
        .and_then(|v| v.as_str())
        .unwrap_or("standard")
        .to_string();

    let sla_hours = args
        .get("sla_hours")
        .and_then(|v| v.as_i64())
        .unwrap_or(0); // 0 → carrier consumer defaults to 48 h

    // shipment_ids is intentionally empty here: the service auto-resolves from
    // the container's master_awbs via ContainerRepository::resolve_awbs_to_shipment_ids
    // when the slice is empty (see HubTransferService::deconsolidate).
    let container = state
        .hub_transfer_svc
        .deconsolidate(DeconsolidateCommand {
            container_id:     ContainerId::from_uuid(container_id),
            destination_zone,
            shipment_ids:     Vec::new(), // auto-resolved inside service
            service_level,
            sla_hours,
        })
        .await
        .map_err(|e| format!("deconsolidation failed: {e}"))?;

    Ok(json!({
        "container_id": container_id,
        "status": format!("{:?}", container.status),
    }))
}

pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "container_id": {
                "type": "string",
                "format": "uuid",
                "description": "Container to break-bulk"
            },
            "destination_zone": {
                "type": "string",
                "description": "Last-mile zone for routing-rule lookup (optional)"
            },
            "service_level": {
                "type": "string",
                "enum": ["standard", "express", "economy"],
                "description": "Last-mile service level forwarded to dispatch/carrier (default: standard)"
            },
            "sla_hours": {
                "type": "integer",
                "description": "SLA window in hours forwarded to carrier booking. 0 = carrier service default (48 h)."
            }
        },
        "required": ["container_id"]
    })
}
