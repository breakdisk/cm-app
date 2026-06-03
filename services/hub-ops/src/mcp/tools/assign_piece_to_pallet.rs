use std::sync::Arc;

use serde_json::{json, Value};

use logisticos_auth::rbac::permissions;
use logisticos_types::{
    awb::{Awb, ChildAwb},
    HubId, PalletId, ShipmentId, TenantId,
};

use crate::domain::entities::{HubScan, ScanType};
use crate::mcp::tools::req_uuid;
use crate::mcp::{context::McpContext, McpState};

pub async fn handle(args: &Value, ctx: &McpContext, state: &Arc<McpState>) -> Result<Value, String> {
    if !ctx.has_permission(permissions::SHIPMENT_UPDATE) {
        return Err("permission denied: shipments:update required".into());
    }

    let piece_awb_s  = args.get("piece_awb").and_then(|v| v.as_str())
        .ok_or("missing required argument 'piece_awb'")?;
    let master_awb_s = args.get("master_awb").and_then(|v| v.as_str())
        .ok_or("missing required argument 'master_awb'")?;
    let hub_id      = req_uuid(args, "hub_id")?;
    let shipment_id = req_uuid(args, "shipment_id")?;
    let pallet_id   = req_uuid(args, "pallet_id")?;

    let piece_awb  = ChildAwb::parse(piece_awb_s).map_err(|e| format!("invalid piece_awb: {e}"))?;
    let master_awb = Awb::parse(master_awb_s).map_err(|e| format!("invalid master_awb: {e}"))?;

    let scan = HubScan::record(
        TenantId::from_uuid(ctx.tenant_id),
        HubId::from_uuid(hub_id),
        piece_awb,
        master_awb,
        ShipmentId::from_uuid(shipment_id),
        ScanType::PalletAssign,
        ctx.actor_uid,
        chrono::Utc::now(),
    )
    .with_pallet(PalletId::from_uuid(pallet_id));

    let scan = state
        .hub_transfer_svc
        .record_scan(scan)
        .await
        .map_err(|e| format!("failed to record pallet-assign scan: {e}"))?;

    Ok(json!({ "scan_id": scan.id, "pallet_id": pallet_id, "scan_type": "pallet_assign" }))
}

pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "hub_id":      { "type": "string", "format": "uuid" },
            "piece_awb":   { "type": "string", "description": "Child AWB of the piece" },
            "master_awb":  { "type": "string", "description": "Master AWB of the shipment" },
            "shipment_id": { "type": "string", "format": "uuid" },
            "pallet_id":   { "type": "string", "format": "uuid" }
        },
        "required": ["hub_id", "piece_awb", "master_awb", "shipment_id", "pallet_id"]
    })
}
