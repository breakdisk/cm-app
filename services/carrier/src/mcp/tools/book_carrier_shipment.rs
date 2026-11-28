//! Tool: book_carrier_shipment
//!
//! Book a shipment with a specific 3PL carrier.
//! Returns the carrier booking reference, tracking number, and label URL.

use serde_json::{json, Value};
use std::sync::Arc;
use crate::AppState;
use crate::mcp::context::McpContext;
use crate::mcp::ids::optional_uuid;
use crate::infrastructure::adapters::{Address, BookingRequest};
use crate::infrastructure::db::CarrierBookingRecord;

pub async fn handle(args: &Value, ctx: &McpContext, state: &Arc<AppState>) -> Result<Value, String> {
    let carrier_code = args["carrier_code"]
        .as_str()
        .ok_or("carrier_code is required")?
        .to_uppercase();

    // Validate every id before anything is sent to the carrier: a booking that
    // succeeds upstream and then loses its audit link is worse than a refusal.
    let shipment_id = optional_uuid(args, "shipment_id")?;

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
        let rec = booking_record(ctx, &carrier_code, shipment_id, &req, &confirmation, resp_payload);
        if let Err(e) = booking_repo.insert(&rec).await {
            tracing::error!("carrier_bookings insert failed; booking {} cannot be cancelled, labelled or tracked over MCP: {e}", rec.booking_ref);
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

/// The audit row for a successful booking. `tenant_id` and the actor come from
/// the validated token (`ctx`) and from nowhere in `args`: a client-supplied
/// `tenant_id` is never read.
fn booking_record(
    ctx:              &McpContext,
    carrier_code:     &str,
    shipment_id:      Option<uuid::Uuid>,
    req:              &BookingRequest,
    confirmation:     &crate::infrastructure::adapters::BookingConfirmation,
    response_payload: Value,
) -> CarrierBookingRecord {
    CarrierBookingRecord {
        tenant_id:        ctx.tenant_id,
        carrier_code:     carrier_code.to_owned(),
        shipment_id,
        awb:              req.reference.clone(),
        service_code:     req.service_code.clone(),
        booking_ref:      confirmation.booking_ref.clone(),
        tracking_number:  confirmation.tracking_number.clone(),
        label_url:        confirmation.label_url.clone(),
        request_payload:  serde_json::to_value(req).unwrap_or_default(),
        response_payload,
        booked_by_actor:  Some(ctx.actor_uid),
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::adapters::BookingConfirmation;
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn audit_row_takes_tenant_and_actor_from_the_token_never_from_arguments() {
        let ctx = McpContext {
            tenant_id: Uuid::new_v4(), actor_uid: Uuid::new_v4(),
            permissions: vec![], partner_id: None, trace_id: "t".into(),
        };
        let other_tenant = Uuid::new_v4();
        let args = json!({
            "tenant_id": other_tenant.to_string(), "actor_uid": other_tenant.to_string(),
            "carrier_code": "DHL", "service_code": "S", "weight_kg": 1.0,
        });
        let addr = json!({"name":"A","street1":"1","city":"C","postal_code":"1","country":"PH"});
        let mut a = args.clone();
        a["shipper"] = addr.clone();
        a["consignee"] = addr;
        let req = BookingRequest {
            service_code: "S".into(),
            shipper:   parse_address(&a, "shipper").unwrap(),
            consignee: parse_address(&a, "consignee").unwrap(),
            weight_kg: 1.0, length_cm: None, width_cm: None, height_cm: None,
            description: "d".into(), declared_value_cents: 0, currency: "USD".into(),
            reference: None,
        };
        let conf = BookingConfirmation {
            booking_ref: "BK".into(), tracking_number: "TN".into(),
            label_url: None, label_bytes: None, estimated_delivery: None,
        };
        let rec = booking_record(&ctx, "DHL", None, &req, &conf, json!({}));
        assert_eq!(rec.tenant_id, ctx.tenant_id);
        assert_ne!(rec.tenant_id, other_tenant);
        assert_eq!(rec.booked_by_actor, Some(ctx.actor_uid));
    }
}
