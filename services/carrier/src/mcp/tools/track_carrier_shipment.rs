//! Tool: track_carrier_shipment
//!
//! Retrieve live tracking events for a carrier tracking number.
//! If carrier_code is omitted the platform tries all configured carriers until
//! one returns a result — useful when the tracking number is known but the
//! carrier is not.

use serde_json::{json, Value};
use std::sync::Arc;
use crate::AppState;
use crate::mcp::context::McpContext;

pub async fn handle(args: &Value, _ctx: &McpContext, state: &Arc<AppState>) -> Result<Value, String> {
    let tracking_number = args["tracking_number"]
        .as_str()
        .ok_or("tracking_number is required")?;

    let carrier_code = args["carrier_code"].as_str().map(str::to_uppercase);

    if let Some(code) = carrier_code {
        let adapter = state.adapter_registry
            .get(&code)
            .ok_or_else(|| format!("Carrier '{code}' is not configured"))?;

        let info = adapter.track_shipment(tracking_number).await
            .map_err(|e| format!("{code} tracking failed: {e}"))?;

        Ok(json!({
            "carrier_code":      code,
            "tracking_number":   info.tracking_number,
            "current_status":    info.current_status,
            "current_location":  info.current_location,
            "estimated_delivery": info.estimated_delivery,
            "events":            info.events.iter().map(|e| json!({
                "timestamp":   e.timestamp,
                "status":      e.status,
                "description": e.description,
                "location":    e.location,
            })).collect::<Vec<_>>(),
        }))
    } else {
        // Fan-out: try all carriers, return first success
        for adapter in state.adapter_registry.all() {
            if let Ok(info) = adapter.track_shipment(tracking_number).await {
                return Ok(json!({
                    "carrier_code":      adapter.carrier_code(),
                    "tracking_number":   info.tracking_number,
                    "current_status":    info.current_status,
                    "current_location":  info.current_location,
                    "estimated_delivery": info.estimated_delivery,
                    "events":            info.events.iter().map(|e| json!({
                        "timestamp":   e.timestamp,
                        "status":      e.status,
                        "description": e.description,
                        "location":    e.location,
                    })).collect::<Vec<_>>(),
                }));
            }
        }
        Err(format!("No configured carrier recognised tracking number: {tracking_number}"))
    }
}

pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "tracking_number": {
                "type": "string",
                "description": "Carrier-issued tracking number"
            },
            "carrier_code": {
                "type": "string",
                "enum": ["DHL", "FEDEX", "UPS", "TNT", "DPD", "ARAMEX"],
                "description": "Optional — omit to auto-detect carrier from tracking number"
            }
        },
        "required": ["tracking_number"]
    })
}
