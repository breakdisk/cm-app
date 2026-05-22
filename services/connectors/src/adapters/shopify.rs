//! Shopify webhook payload types and mapping to LogisticOS commands.

use logisticos_errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};

use crate::{
    domain::entities::ConnectorCredentials,
    infrastructure::order_intake_client::{AddressPayload, InternalCreateShipmentRequest},
};

// ── Shopify payload types ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ShopifyOrder {
    pub id:                     u64,
    pub name:                   String,          // "#1001"
    pub gateway:                Option<String>,  // "cash_on_delivery" | "stripe" | ...
    #[serde(default)]
    pub payment_gateway_names:  Vec<String>,
    #[serde(default)]
    pub financial_status:       String,          // "pending" | "paid"
    pub total_price:            String,          // "1500.00"
    pub contact_email:          Option<String>,
    pub customer:               Option<ShopifyCustomer>,
    pub shipping_address:       Option<ShopifyAddress>,
    #[serde(default)]
    pub line_items:             Vec<ShopifyLineItem>,
}

#[derive(Debug, Deserialize)]
pub struct ShopifyCustomer {
    pub email: Option<String>,
    pub phone: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ShopifyAddress {
    pub first_name:   Option<String>,
    pub last_name:    Option<String>,
    pub address1:     Option<String>,
    pub address2:     Option<String>,
    pub city:         Option<String>,
    pub province:     Option<String>,
    pub zip:          Option<String>,
    pub country_code: Option<String>,
    pub phone:        Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ShopifyLineItem {
    pub quantity: u64,
    #[serde(default)]
    pub grams:    u64,
}

// ── Fulfillment push-back ─────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct FulfillmentRequest {
    fulfillment: FulfillmentBody,
}

#[derive(Debug, Serialize)]
struct FulfillmentBody {
    tracking_company: String,
    tracking_number:  String,
    tracking_url:     String,
    notify_customer:  bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    note:             Option<String>,
}

// ── Mapping ───────────────────────────────────────────────────────────────────

/// Map a Shopify `orders/create` webhook payload to an order-intake command.
pub fn map_to_command(
    order: &ShopifyOrder,
    creds: &ConnectorCredentials,
) -> AppResult<InternalCreateShipmentRequest> {
    let shipping = order
        .shipping_address
        .as_ref()
        .ok_or_else(|| AppError::Validation("Shopify order has no shipping_address".into()))?;

    // Customer name from shipping address
    let customer_name = format!(
        "{} {}",
        shipping.first_name.as_deref().unwrap_or(""),
        shipping.last_name.as_deref().unwrap_or(""),
    )
    .trim()
    .to_string();
    if customer_name.is_empty() {
        return Err(AppError::Validation("Shopify order has no customer name".into()));
    }

    // Phone: shipping address first, then customer profile
    let customer_phone = shipping
        .phone
        .as_deref()
        .or_else(|| order.customer.as_ref().and_then(|c| c.phone.as_deref()))
        .filter(|p| !p.is_empty())
        .ok_or_else(|| AppError::Validation("Shopify order has no customer phone".into()))?
        .to_string();

    // Email: customer profile first, then contact_email
    let customer_email = order
        .customer
        .as_ref()
        .and_then(|c| c.email.as_deref())
        .or(order.contact_email.as_deref())
        .filter(|e| !e.is_empty() && e.contains('@'))
        .map(str::to_string);

    // Destination from shipping address
    let destination = AddressPayload {
        line1:        shipping.address1.clone().unwrap_or_default(),
        line2:        shipping.address2.clone().filter(|s| !s.is_empty()),
        barangay:     None,
        city:         shipping.city.clone().unwrap_or_default(),
        province:     shipping.province.clone().unwrap_or_default(),
        postal_code:  shipping.zip.clone().unwrap_or_default(),
        country_code: shipping.country_code.clone().unwrap_or_else(|| "PH".into()),
    };

    // Origin from connector config
    let origin = build_origin_address(creds)?;

    // Weight: sum of (line_item.grams × quantity); minimum 1g
    let weight_grams: u64 = order
        .line_items
        .iter()
        .map(|i| i.grams.saturating_mul(i.quantity))
        .sum();
    let weight_grams = (weight_grams.max(1) as u32)
        .max(creds.default_weight_grams().min(1));

    // Declared value
    let declared_value_cents = parse_price_cents(&order.total_price);

    // COD detection: check gateway against configured list
    let is_cod = order
        .gateway
        .as_deref()
        .map(|gw| creds.shopify_cod_gateways().iter().any(|&g| g == gw))
        .unwrap_or(false);
    let cod_amount_cents = if is_cod { declared_value_cents } else { None };

    Ok(InternalCreateShipmentRequest {
        tenant_id:         creds.tenant_id,
        merchant_id:       creds.merchant_id,
        tenant_slug:       creds.tenant_slug.clone(),
        source_platform:   "shopify".into(),
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
        merchant_reference: Some(order.name.clone()),
        description:       None,
        special_instructions: None,
    })
}

/// Push the assigned AWB and tracking URL back to the Shopify order as a fulfillment.
/// Fire-and-forget: errors are logged but never propagate.
pub async fn push_tracking_back(
    client: &reqwest::Client,
    creds: &ConnectorCredentials,
    order_id: u64,
    awb: &str,
    tracking_url: &str,
    is_cod: bool,
) -> AppResult<()> {
    let shop_domain = creds.shopify_shop_domain().ok_or_else(|| AppError::ExternalService {
        service: "shopify".into(),
        message: "shop_domain not configured in connector credentials".into(),
    })?;
    let admin_token = creds.shopify_admin_token().ok_or_else(|| AppError::ExternalService {
        service: "shopify".into(),
        message: "admin_api_token not configured in connector credentials".into(),
    })?;

    let url = format!(
        "https://{}/admin/api/2024-01/orders/{}/fulfillments.json",
        shop_domain, order_id
    );

    let note = if is_cod {
        Some(format!(
            "AWB: {}. Driver will collect cash payment of the declared value upon delivery.",
            awb
        ))
    } else {
        None
    };

    let body = FulfillmentRequest {
        fulfillment: FulfillmentBody {
            tracking_company: "LogisticOS".into(),
            tracking_number:  awb.to_string(),
            tracking_url:     tracking_url.to_string(),
            notify_customer:  true,
            note,
        },
    };

    let resp = client
        .post(&url)
        .header("X-Shopify-Access-Token", admin_token)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::ExternalService {
            service: "shopify".into(),
            message: e.to_string(),
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text   = resp.text().await.unwrap_or_default();
        return Err(AppError::ExternalService {
            service: "shopify".into(),
            message: format!("fulfillment push-back failed: HTTP {status} — {text}"),
        });
    }

    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn build_origin_address(creds: &ConnectorCredentials) -> AppResult<AddressPayload> {
    let cfg = creds.origin_address().ok_or_else(|| {
        AppError::Validation(
            "Connector credentials missing origin_address configuration. \
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

/// Parse Shopify price string "1500.00" → centavos 150000.
/// Returns None if the string is missing or unparseable.
fn parse_price_cents(price: &str) -> Option<i64> {
    price.trim().parse::<f64>().ok().map(|f| (f * 100.0).round() as i64)
}
