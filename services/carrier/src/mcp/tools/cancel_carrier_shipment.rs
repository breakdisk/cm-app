/// Tool: cancel_carrier_shipment
///
/// Cancel a booked shipment with a carrier using the booking_ref returned by
/// book_carrier_shipment.  Note: DHL does not support REST cancellation;
/// that carrier will return an instructive error.

use serde_json::{json, Value};
use std::sync::Arc;
use crate::AppState;
use crate::mcp::context::McpContext;

pub async fn handle(args: &Value, _ctx: &McpContext, state: &Arc<AppState>) -> Result<Value, String> {
    let carrier_code = args["carrier_code"]
        .as_str()
        .ok_or("carrier_code is required")?
        .to_uppercase();

    let booking_ref = args["booking_ref"]
        .as_str()
        .ok_or("booking_ref is required")?;

    let adapter = state.adapter_registry
        .get(&carrier_code)
        .ok_or_else(|| format!("Carrier '{carrier_code}' is not configured"))?;

    adapter.cancel_shipment(booking_ref).await
        .map_err(|e| format!("{carrier_code} cancellation failed: {e}"))?;

    Ok(json!({ "cancelled": true, "carrier_code": carrier_code, "booking_ref": booking_ref }))
}

pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "carrier_code": {
                "type": "string",
                "enum": ["DHL", "FEDEX", "UPS", "TNT", "DPD", "ARAMEX"]
            },
            "booking_ref": {
                "type": "string",
                "description": "Booking reference returned by book_carrier_shipment"
            }
        },
        "required": ["carrier_code", "booking_ref"]
    })
}
