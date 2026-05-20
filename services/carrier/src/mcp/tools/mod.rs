pub mod book_carrier_shipment;
pub mod cancel_carrier_shipment;
pub mod get_carrier_label;
pub mod get_carrier_rates;
pub mod track_carrier_shipment;

use serde_json::Value;
use std::sync::Arc;
use crate::AppState;
use crate::mcp::context::McpContext;

pub async fn dispatch(
    name: &str,
    args: &Value,
    ctx: &McpContext,
    state: &Arc<AppState>,
) -> Result<Value, String> {
    match name {
        "get_carrier_rates"       => get_carrier_rates::handle(args, ctx, state).await,
        "book_carrier_shipment"   => book_carrier_shipment::handle(args, ctx, state).await,
        "track_carrier_shipment"  => track_carrier_shipment::handle(args, ctx, state).await,
        "cancel_carrier_shipment" => cancel_carrier_shipment::handle(args, ctx, state).await,
        "get_carrier_label"       => get_carrier_label::handle(args, ctx, state).await,
        other                     => Err(format!("Unknown carrier MCP tool: {other}")),
    }
}

pub fn list() -> Value {
    serde_json::json!([
        {
            "name":        "get_carrier_rates",
            "description": "Shop rates across all configured 3PL carriers (DHL, FedEx, UPS, TNT, DPD, Aramex). Optionally filter by a single carrier or service type. Returns quotes sorted by price ascending.",
            "inputSchema": get_carrier_rates::schema()
        },
        {
            "name":        "book_carrier_shipment",
            "description": "Book a shipment with a specific 3PL carrier using a service code from get_carrier_rates. Returns booking reference, tracking number, and label URL.",
            "inputSchema": book_carrier_shipment::schema()
        },
        {
            "name":        "track_carrier_shipment",
            "description": "Retrieve live tracking events for a carrier tracking number. Optionally specify carrier_code; if omitted the platform tries all configured carriers.",
            "inputSchema": track_carrier_shipment::schema()
        },
        {
            "name":        "cancel_carrier_shipment",
            "description": "Cancel a booked shipment using the booking_ref from book_carrier_shipment. Note: DHL requires manual cancellation via support.",
            "inputSchema": cancel_carrier_shipment::schema()
        },
        {
            "name":        "get_carrier_label",
            "description": "Retrieve the shipping label as base64-encoded bytes (PDF/ZPL/PNG) for printing or storage.",
            "inputSchema": get_carrier_label::schema()
        }
    ])
}
