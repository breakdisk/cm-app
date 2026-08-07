//! WooCommerce webhook payload types and mapping to LogisticOS commands.

use base64::Engine as _;
use logisticos_errors::{AppError, AppResult};
use serde::Deserialize;

use crate::{
    domain::entities::ConnectorCredentials,
    infrastructure::order_intake_client::{AddressPayload, InternalCreateShipmentRequest},
};

// ── WooCommerce payload types ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct WooCommerceOrder {
    pub id:                   u64,
    pub number:               String,          // "1001"
    pub status:               String,          // "processing" | "pending"
    pub payment_method:       String,          // "cod" | "stripe" | ...
    pub payment_method_title: Option<String>,  // "Cash on Delivery"
    pub total:                String,          // "1500.00"
    pub billing:              WooAddress,
    pub shipping:             WooAddress,
    #[serde(default)]
    pub line_items:           Vec<WooLineItem>,
    #[serde(default)]
    pub customer_note:        String,
}

#[derive(Debug, Deserialize)]
pub struct WooAddress {
    pub first_name: Option<String>,
    pub last_name:  Option<String>,
    pub address_1:  Option<String>,
    pub address_2:  Option<String>,
    pub city:       Option<String>,
    pub state:      Option<String>,  // short code e.g. "MM" for Metro Manila
    pub postcode:   Option<String>,
    pub country:    Option<String>,  // "PH"
    pub phone:      Option<String>,
    pub email:      Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WooLineItem {
    pub quantity: u32,
    pub total:    String,
}

// ── Mapping ───────────────────────────────────────────────────────────────────

/// Map a WooCommerce `order.created` webhook payload to an order-intake command.
pub fn map_to_command(
    order: &WooCommerceOrder,
    creds: &ConnectorCredentials,
) -> AppResult<InternalCreateShipmentRequest> {
    // Customer name from shipping address (fallback to billing)
    let first = order
        .shipping
        .first_name
        .as_deref()
        .or(order.billing.first_name.as_deref())
        .unwrap_or("");
    let last = order
        .shipping
        .last_name
        .as_deref()
        .or(order.billing.last_name.as_deref())
        .unwrap_or("");
    let customer_name = format!("{first} {last}").trim().to_string();
    if customer_name.is_empty() {
        return Err(AppError::Validation("WooCommerce order has no customer name".into()));
    }

    // Phone from billing
    let customer_phone = order
        .billing
        .phone
        .as_deref()
        .filter(|p| !p.is_empty())
        .ok_or_else(|| AppError::Validation("WooCommerce order has no billing phone".into()))?
        .to_string();

    // Email from billing
    let customer_email = order
        .billing
        .email
        .as_deref()
        .filter(|e| !e.is_empty() && e.contains('@'))
        .map(str::to_string);

    // Use shipping address as destination (fallback to billing for city/province/postcode)
    let addr = &order.shipping;
    let destination = AddressPayload {
        line1:        addr.address_1.clone().unwrap_or_default(),
        line2:        addr.address_2.clone().filter(|s| !s.is_empty()),
        barangay:     None,
        city:         addr.city.clone()
            .or_else(|| order.billing.city.clone())
            .unwrap_or_default(),
        province:     addr.state.clone()
            .or_else(|| order.billing.state.clone())
            .unwrap_or_default(),
        postal_code:  addr.postcode.clone()
            .or_else(|| order.billing.postcode.clone())
            .unwrap_or_default(),
        country_code: addr.country.clone()
            .or_else(|| order.billing.country.clone())
            .unwrap_or_else(|| "PH".into()),
    };

    // Origin from connector config
    let origin = build_origin_address(creds)?;

    // Weight: WooCommerce order webhooks don't include product weight per line item.
    // Use the configured default (merchant sets this in LogisticOS connector settings).
    let weight_grams = creds.default_weight_grams();

    // Declared value from order total
    let declared_value_cents = parse_price_cents(&order.total);

    // COD detection: payment_method in configured COD list
    let is_cod = creds
        .woo_cod_payment_methods().contains(&order.payment_method.as_str());
    let cod_amount_cents = if is_cod { declared_value_cents } else { None };

    // Special instructions: include customer note if present
    let special_instructions = if order.customer_note.is_empty() {
        None
    } else {
        Some(order.customer_note.clone())
    };

    Ok(InternalCreateShipmentRequest {
        tenant_id:         creds.tenant_id,
        merchant_id:       creds.merchant_id,
        tenant_slug:       creds.tenant_slug.clone(),
        source_platform:   "woocommerce".into(),
        external_order_id: Some(order.id.to_string()),
        customer_name,
        customer_phone,
        customer_email,
        origin,
        destination,
        service_type:      creds.default_service_type().to_string(),
        weight_grams,
        length_cm:         None,
        width_cm:          None,
        height_cm:         None,
        declared_value_cents,
        cod_amount_cents,
        merchant_reference: Some(order.number.clone()),
        description:        None,
        special_instructions,
    })
}

/// Push AWB and tracking URL back to WooCommerce via order meta_data.
/// Uses HTTP Basic Auth (consumer_key:consumer_secret).
pub async fn push_tracking_back(
    client: &reqwest::Client,
    creds: &ConnectorCredentials,
    order_id: u64,
    awb: &str,
    tracking_url: &str,
    is_cod: bool,
) -> AppResult<()> {
    let store_url = creds.woo_store_url().ok_or_else(|| AppError::ExternalService {
        service: "woocommerce".into(),
        message: "store_url not configured in connector credentials".into(),
    })?;
    let consumer_key = creds.woo_consumer_key().ok_or_else(|| AppError::ExternalService {
        service: "woocommerce".into(),
        message: "consumer_key not configured".into(),
    })?;
    let consumer_secret = creds.woo_consumer_secret().ok_or_else(|| AppError::ExternalService {
        service: "woocommerce".into(),
        message: "consumer_secret not configured".into(),
    })?;

    let url = format!(
        "{}/wp-json/wc/v3/orders/{}",
        store_url.trim_end_matches('/'),
        order_id
    );

    // Build meta_data payload
    let meta: Vec<serde_json::Value> = vec![
        serde_json::json!({ "key": "_tracking_number", "value": awb }),
        serde_json::json!({ "key": "_tracking_url",    "value": tracking_url }),
    ];

    let mut body = serde_json::json!({ "meta_data": meta });

    // For COD orders, also add a customer-facing note
    if is_cod {
        body["customer_note"] = serde_json::json!(
            format!("Your shipment AWB is {}. Our driver will collect the payment upon delivery.", awb)
        );
    }

    // Basic Auth: base64("consumer_key:consumer_secret")
    let credentials = base64::engine::general_purpose::STANDARD
        .encode(format!("{consumer_key}:{consumer_secret}"));

    let resp = client
        .put(&url)
        .header("Authorization", format!("Basic {credentials}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::ExternalService {
            service: "woocommerce".into(),
            message: e.to_string(),
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text   = resp.text().await.unwrap_or_default();
        return Err(AppError::ExternalService {
            service: "woocommerce".into(),
            message: format!("tracking push-back failed: HTTP {status} — {text}"),
        });
    }

    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn build_origin_address(creds: &ConnectorCredentials) -> AppResult<AddressPayload> {
    let cfg = creds.origin_address().ok_or_else(|| {
        AppError::Validation(
            "Connector credentials missing origin_address. \
             Please configure your pickup address in the LogisticOS connector settings.".into(),
        )
    })?;

    let get = |key: &str| -> AppResult<String> {
        cfg.get(key)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .ok_or_else(|| AppError::Validation(format!("origin_address.{key} is required")))
    };

    Ok(AddressPayload {
        line1:        get("line1")?,
        line2:        cfg.get("line2").and_then(|v| v.as_str()).map(str::to_string),
        barangay:     cfg.get("barangay").and_then(|v| v.as_str()).map(str::to_string),
        city:         get("city")?,
        province:     get("province")?,
        postal_code:  get("postal_code")?,
        country_code: cfg.get("country_code").and_then(|v| v.as_str()).unwrap_or("PH").to_string(),
    })
}

fn parse_price_cents(price: &str) -> Option<i64> {
    price.trim().parse::<f64>().ok().map(|f| (f * 100.0).round() as i64)
}
