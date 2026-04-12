use serde_json::{json, Value};
use std::sync::Arc;
use logisticos_types::{Coordinates, TenantId};
use crate::api::http::AppState;
use crate::mcp::context::McpContext;
use crate::domain::value_objects::DEFAULT_DRIVER_SEARCH_RADIUS_KM;

pub async fn handle(
    args: &Value,
    ctx: &McpContext,
    state: &Arc<AppState>,
) -> Result<Value, String> {
    let vehicle_type = args.get("vehicle_type").and_then(|v| v.as_str()).map(String::from);

    // Default to Metro Manila anchor when no zone specified.
    // TODO: look up zone centroid from zone_id when zone registry exists.
    let coords = Coordinates { lat: 14.5995, lng: 120.9842 };

    let tenant_id = TenantId::from_uuid(ctx.tenant_id);
    let drivers = state.dispatch_service
        .list_available_drivers(&tenant_id, coords, DEFAULT_DRIVER_SEARCH_RADIUS_KM)
        .await
        .map_err(|e| format!("Failed to list drivers: {e}"))?;

    let filtered: Vec<Value> = drivers.into_iter()
        .filter(|d| {
            vehicle_type.as_deref()
                .map(|vt| d.vehicle_type.as_deref() == Some(vt))
                .unwrap_or(true)
        })
        .map(|d| json!({
            "id": d.driver_id.inner(),
            "name": d.name,
            "vehicle_type": d.vehicle_type,
            "distance_km": d.distance_km,
            "active_stop_count": d.active_stop_count,
        }))
        .collect();

    Ok(json!({ "drivers": filtered }))
}

pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "zone_id": {
                "type": "string",
                "format": "uuid",
                "description": "Optional: filter by zone (future use)"
            },
            "vehicle_type": {
                "type": "string",
                "enum": ["motorcycle", "van", "truck"],
                "description": "Optional: filter by vehicle type"
            }
        },
        "required": []
    })
}
