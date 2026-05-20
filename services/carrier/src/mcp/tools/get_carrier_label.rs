/// Tool: get_carrier_label
///
/// Retrieve the shipping label for a booked shipment as base64-encoded bytes.
/// The caller can decode and write directly to a PDF/ZPL printer stream.

use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::sync::Arc;
use crate::AppState;
use crate::mcp::context::McpContext;
use crate::infrastructure::adapters::LabelFormat;

pub async fn handle(args: &Value, _ctx: &McpContext, state: &Arc<AppState>) -> Result<Value, String> {
    let carrier_code = args["carrier_code"]
        .as_str()
        .ok_or("carrier_code is required")?
        .to_uppercase();

    let booking_ref = args["booking_ref"]
        .as_str()
        .ok_or("booking_ref is required")?;

    let format = match args["format"].as_str().unwrap_or("PDF").to_uppercase().as_str() {
        "ZPL" => LabelFormat::Zpl,
        "PNG" => LabelFormat::Png,
        _     => LabelFormat::Pdf,
    };

    let adapter = state.adapter_registry
        .get(&carrier_code)
        .ok_or_else(|| format!("Carrier '{carrier_code}' is not configured"))?;

    let bytes = adapter.get_label(booking_ref, format).await
        .map_err(|e| format!("{carrier_code} label fetch failed: {e}"))?;

    let content_type = match format {
        LabelFormat::Pdf => "application/pdf",
        LabelFormat::Zpl => "application/zpl",
        LabelFormat::Png => "image/png",
    };

    Ok(json!({
        "carrier_code":   carrier_code,
        "booking_ref":    booking_ref,
        "format":         format.to_string(),
        "content_type":   content_type,
        "size_bytes":     bytes.len(),
        "label_base64":   STANDARD.encode(&bytes),
    }))
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
            },
            "format": {
                "type": "string",
                "enum": ["PDF", "ZPL", "PNG"],
                "description": "Label format (default: PDF)"
            }
        },
        "required": ["carrier_code", "booking_ref"]
    })
}
