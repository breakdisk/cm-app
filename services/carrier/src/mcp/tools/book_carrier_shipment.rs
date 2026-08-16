//! Tool: book_carrier_shipment
//!
//! Book a shipment with a specific 3PL carrier.
//! Returns the carrier booking reference, tracking number, and label URL.

use serde_json::{json, Value};
use std::sync::Arc;
use crate::AppState;
use crate::mcp::context::McpContext;
use crate::infrastructure::adapters::{Address, BookingRequest};
use crate::infrastructure::db::CarrierBookingRecord;

pub async fn handle(args: &Value, ctx: &McpContext, state: &Arc<AppState>) -> Result<Value, String> {
    let carrier_code = args["carrier_code"]
        .as_str()
        .ok_or("carrier_code is required")?
        .to_uppercase();

    let adapter = state.adapter_registry
        .get(&carrier_code)
        .ok_or_else(|| format!("Carrier '{carrier_code}' is not configured"))?;

    let req = BookingRequest {
        service_code:         args["service_code"].as_str().ok_or("service_code is required")?.into(),
        shipper:              parse_address(args, "shipper")?,
        consignee:            parse_address(args, "consignee")?,
        weight_kg:            args["weight_kg"].as_f64().ok_or("weight_kg is required")?,
        length_cm:            args["length_cm"].as_f64(),
        width_cm:             args["width_cm"].as_f64(),
        height_cm:            args["height_cm"].as_f64(),
        description:          args["description"].as_str().unwrap_or("General Cargo").into(),
        declared_value_cents: args["declared_value_cents"].as_i64().unwrap_or(0),
        currency:             args["currency"].as_str().unwrap_or("USD").into(),
        reference:            args["reference"].as_str().map(String::from),
    };

    let req_payload = serde_json::to_value(&req).unwrap_or_default();

    let mut confirmation = adapter.book_shipment(&req).await
        .map_err(|e| format!("{carrier_code} booking failed: {e}"))?;

    // G2 — Upload inline label bytes to R2/S3 and return a presigned download URL.
    if let Some(bytes) = confirmation.label_bytes.take() {
        if let Some(storage) = &state.storage {
            let key = format!("labels/{}/{}.pdf", carrier_code, confirmation.booking_ref);
            if let Err(e) = storage.put_bytes(&key, bytes, "application/pdf").await {
                tracing::warn!(key, "Label upload failed in MCP tool: {e}");
            } else {
                match storage.presign_download(&key, 3600).await {
                    Ok(url) => confirmation.label_url = Some(url),
                    Err(e)  => tracing::warn!(key, "Label presign failed: {e}"),
                }
            }
        }
    }

    let resp_payload = json!({
        "booking_ref":        confirmation.booking_ref,
        "tracking_number":    confirmation.tracking_number,
        "label_url":          confirmation.label_url,
        "estimated_delivery": confirmation.estimated_delivery,
    });

    // G1 — Write audit row to carrier.carrier_bookings.
    if let Some(booking_repo) = &state.booking_repo {
        let shipment_id = args["shipment_id"].as_str()
            .and_then(|s| uuid::Uuid::parse_str(s).ok());
        let awb = args["reference"].as_str().map(String::from);

        if let Err(e) = booking_repo.insert(&CarrierBookingRecord {
            tenant_id:        ctx.tenant_id,
            carrier_code:     carrier_code.clone(),
            shipment_id,
            awb,
            service_code:     req.service_code.clone(),
            booking_ref:      confirmation.booking_ref.clone(),
            tracking_number:  confirmation.tracking_number.clone(),
            label_url:        confirmation.label_url.clone(),
            request_payload:  req_payload,
            response_payload: resp_payload.clone(),
            booked_by_actor:  Some(ctx.actor_uid),
        }).await {
            tracing::warn!("carrier_bookings insert failed: {e}");
        }
    }

    Ok(json!({
        "carrier_code":       carrier_code,
        "booking_ref":        confirmation.booking_ref,
        "tracking_number":    confirmation.tracking_number,
        "label_url":          confirmation.label_url,
        "estimated_delivery": confirmation.estimated_delivery,
    }))
}

fn parse_address(args: &Value, field: &str) -> Result<Address, String> {
    let a = &args[field];
    if a.is_null() {
        return Err(format!("'{field}' address is required"));
    }
    Ok(Address {
        name:        a["name"].as_str().unwrap_or("").into(),
        company:     a["company"].as_str().map(String::from),
        street1:     a["street1"].as_str().ok_or(format!("{field}.street1 required"))?.into(),
        street2:     a["street2"].as_str().map(String::from),
        city:        a["city"].as_str().ok_or(format!("{field}.city required"))?.into(),
        state:       a["state"].as_str().map(String::from),
        postal_code: a["postal_code"].as_str().ok_or(format!("{field}.postal_code required"))?.into(),
        country:     a["country"].as_str().ok_or(format!("{field}.country required"))?.into(),
        phone:       a["phone"].as_str().map(String::from),
        email:       a["email"].as_str().map(String::from),
    })
}

pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "carrier_code":  {
                "type": "string",
                "enum": ["DHL", "FEDEX", "UPS", "TNT", "DPD", "ARAMEX"],
                "description": "Target carrier for this booking"
            },
            "service_code":  { "type": "string", "description": "Carrier-specific service code from get_carrier_rates" },
            "shipper":       { "$ref": "#/definitions/Address" },
            "consignee":     { "$ref": "#/definitions/Address" },
            "weight_kg":     { "type": "number" },
            "length_cm":     { "type": "number" },
            "width_cm":      { "type": "number" },
            "height_cm":     { "type": "number" },
            "description":   { "type": "string", "description": "Contents description for customs" },
            "declared_value_cents": { "type": "integer", "description": "Declared value in minor currency units" },
            "currency":      { "type": "string", "description": "ISO 4217" },
            "reference":     { "type": "string", "description": "Platform AWB — sent as customer reference to carrier" },
            "shipment_id":   { "type": "string", "format": "uuid", "description": "Platform shipment UUID for audit log" }
        },
        "required": ["carrier_code", "service_code", "shipper", "consignee", "weight_kg"],
        "definitions": {
            "Address": {
                "type": "object",
                "properties": {
                    "name":        { "type": "string" },
                    "company":     { "type": "string" },
                    "street1":     { "type": "string" },
                    "street2":     { "type": "string" },
                    "city":        { "type": "string" },
                    "state":       { "type": "string" },
                    "postal_code": { "type": "string" },
                    "country":     { "type": "string" },
                    "phone":       { "type": "string" },
                    "email":       { "type": "string" }
                },
                "required": ["name", "street1", "city", "postal_code", "country"]
            }
        }
    })
}
