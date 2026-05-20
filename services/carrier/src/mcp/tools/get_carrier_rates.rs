/// Tool: get_carrier_rates
///
/// Fan-out rate shop across all configured carriers (or a single one).
/// Returns a flat array of rate quotes sorted ascending by price.

use serde_json::{json, Value};
use std::sync::Arc;
use crate::AppState;
use crate::mcp::context::McpContext;
use crate::infrastructure::adapters::{Address, RateRequest};

pub async fn handle(args: &Value, _ctx: &McpContext, state: &Arc<AppState>) -> Result<Value, String> {
    let origin      = parse_address(args, "origin")?;
    let destination = parse_address(args, "destination")?;
    let weight_kg   = args["weight_kg"].as_f64().ok_or("weight_kg is required")?;
    let length_cm   = args["length_cm"].as_f64();
    let width_cm    = args["width_cm"].as_f64();
    let height_cm   = args["height_cm"].as_f64();
    let service_type = args["service_type"].as_str().map(String::from);
    let currency    = args["currency"].as_str().unwrap_or("USD").to_string();
    let carrier_code = args["carrier_code"].as_str().map(str::to_uppercase);

    let req = RateRequest { origin, destination, weight_kg, length_cm, width_cm, height_cm, service_type, currency };

    let adapters: Vec<_> = match &carrier_code {
        Some(code) => {
            let a = state.adapter_registry
                .get(code)
                .ok_or_else(|| format!("Carrier '{code}' is not configured"))?;
            vec![a]
        }
        None => state.adapter_registry.all(),
    };

    let mut all_quotes = vec![];
    let mut errors     = vec![];

    for adapter in adapters {
        match adapter.get_rates(&req).await {
            Ok(quotes) => all_quotes.extend(quotes),
            Err(e) => errors.push(json!({ "carrier": adapter.carrier_code(), "error": e.to_string() })),
        }
    }

    // Sort by price ascending
    all_quotes.sort_by_key(|q| q.price_cents);

    Ok(json!({
        "quotes": all_quotes.iter().map(|q| json!({
            "carrier_code":  q.carrier_code,
            "service_code":  q.service_code,
            "service_name":  q.service_name,
            "price_cents":   q.price_cents,
            "currency":      q.currency,
            "transit_days":  q.transit_days,
            "valid_until":   q.valid_until,
        })).collect::<Vec<_>>(),
        "errors": errors,
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
            "origin": {
                "type": "object",
                "description": "Shipper / pickup address",
                "properties": {
                    "name":        { "type": "string" },
                    "company":     { "type": "string" },
                    "street1":     { "type": "string" },
                    "street2":     { "type": "string" },
                    "city":        { "type": "string" },
                    "state":       { "type": "string" },
                    "postal_code": { "type": "string" },
                    "country":     { "type": "string", "description": "ISO 3166-1 alpha-2" },
                    "phone":       { "type": "string" },
                    "email":       { "type": "string" }
                },
                "required": ["street1", "city", "postal_code", "country"]
            },
            "destination": {
                "type": "object",
                "description": "Consignee / delivery address",
                "properties": {
                    "name":        { "type": "string" },
                    "company":     { "type": "string" },
                    "street1":     { "type": "string" },
                    "street2":     { "type": "string" },
                    "city":        { "type": "string" },
                    "state":       { "type": "string" },
                    "postal_code": { "type": "string" },
                    "country":     { "type": "string", "description": "ISO 3166-1 alpha-2" },
                    "phone":       { "type": "string" },
                    "email":       { "type": "string" }
                },
                "required": ["street1", "city", "postal_code", "country"]
            },
            "weight_kg":    { "type": "number", "description": "Shipment weight in kilograms" },
            "length_cm":    { "type": "number" },
            "width_cm":     { "type": "number" },
            "height_cm":    { "type": "number" },
            "service_type": { "type": "string", "description": "Optional carrier-specific service code filter" },
            "currency":     { "type": "string", "description": "ISO 4217 currency for returned prices (default: USD)" },
            "carrier_code": {
                "type": "string",
                "enum": ["DHL", "FEDEX", "UPS", "TNT", "DPD", "ARAMEX"],
                "description": "Optional: restrict to a single carrier. Omit to shop all configured carriers."
            }
        },
        "required": ["origin", "destination", "weight_kg"]
    })
}
