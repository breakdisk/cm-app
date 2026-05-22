use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
pub struct CreateShipmentCommand {
    #[serde(default)]
    pub tenant_id: uuid::Uuid,
    #[serde(default)]
    pub merchant_id: uuid::Uuid,

    #[validate(length(min = 1, max = 200))]
    pub customer_name: String,
    #[validate(length(min = 7, max = 20))]
    pub customer_phone: String,
    pub customer_email: Option<String>,

    pub origin: AddressInput,
    pub destination: AddressInput,

    pub service_type: String,       // "standard" | "express" | "same_day" | "balikbayan"
    #[validate(range(min = 1, max = 70000))]
    pub weight_grams: u32,
    pub length_cm: Option<u32>,
    pub width_cm:  Option<u32>,
    pub height_cm: Option<u32>,

    pub declared_value_cents: Option<i64>,
    pub cod_amount_cents: Option<i64>,
    pub special_instructions: Option<String>,
    pub merchant_reference: Option<String>,  // Merchant's own order ID
    pub description: Option<String>,         // Contents description e.g. "Clothes, Electronics"

    /// E-commerce platform that originated this shipment (set by connector, ignored for direct API calls).
    #[serde(default)]
    pub source_platform: Option<String>,
    /// Platform-native order ID for deduplication (set by connector).
    #[serde(default)]
    pub external_order_id: Option<String>,

    /// Number of physical pieces in this shipment (1..=999). Defaults to 1.
    /// For Balikbayan: number of boxes. For standard: usually 1.
    pub piece_count: Option<u16>,

    /// 3-char tenant code for AWB generation (e.g. "PH1").
    /// Populated from JWT claims in the HTTP handler.
    #[serde(default)]
    pub tenant_code: String,

    /// True when the booking originates from the customer app (B2C self-service).
    /// Set by the API handler based on the JWT `role` claim ("customer").
    /// When true, a payment receipt is issued at POD instead of a merchant invoice.
    #[serde(default)]
    pub booked_by_customer: bool,

    /// True when dispatch should auto-assign a driver immediately on creation.
    /// Orthogonal to `booked_by_customer` (billing). Wraps as `Option` so the
    /// HTTP handler can distinguish "client didn't set it" from "client explicitly
    /// set false" — useful for admin-role callers who may want manual dispatch.
    #[serde(default)]
    pub auto_dispatch: Option<bool>,
}

#[derive(Debug, Deserialize, Validate, Serialize)]
pub struct AddressInput {
    #[validate(length(min = 5))]
    pub line1: String,
    pub line2: Option<String>,
    pub barangay: Option<String>,
    #[validate(length(min = 2))]
    pub city: String,
    pub province: String,
    pub postal_code: String,
    #[validate(length(min = 2, max = 2))]
    pub country_code: String,   // "PH"
}

/// Command issued by the connectors service via the internal (Istio mTLS) endpoint.
/// Carries all fields needed to create a shipment without a JWT — tenant identity
/// is asserted by the connector service which verified the platform HMAC first.
#[derive(Debug, Deserialize, Validate)]
pub struct InternalCreateShipmentCommand {
    pub tenant_id:         uuid::Uuid,
    pub merchant_id:       uuid::Uuid,
    /// Used to derive the 3-char AWB tenant code (same algorithm as the HTTP handler).
    pub tenant_slug:       String,
    pub source_platform:   String,    // "shopify" | "woocommerce"
    pub external_order_id: Option<String>,

    #[validate(length(min = 1, max = 200))]
    pub customer_name:    String,
    #[validate(length(min = 7, max = 20))]
    pub customer_phone:   String,
    pub customer_email:   Option<String>,

    pub origin:           AddressInput,
    pub destination:      AddressInput,

    pub service_type:     String,
    #[validate(range(min = 1, max = 70000))]
    pub weight_grams:     u32,
    pub length_cm:        Option<u32>,
    pub width_cm:         Option<u32>,
    pub height_cm:        Option<u32>,

    pub declared_value_cents: Option<i64>,
    pub cod_amount_cents:     Option<i64>,
    pub merchant_reference:   Option<String>,
    pub description:          Option<String>,
    pub special_instructions: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RescheduleShipmentCommand {
    pub shipment_id: uuid::Uuid,
    pub preferred_date: chrono::NaiveDate,
    pub preferred_time_slot: Option<String>, // "morning" | "afternoon" | "anytime"
    pub reason: String,
}

#[derive(Debug, Deserialize)]
pub struct CancelShipmentCommand {
    pub shipment_id: uuid::Uuid,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
pub struct BulkCreateShipmentCommand {
    pub tenant_id: uuid::Uuid,
    pub merchant_id: uuid::Uuid,
    pub rows: Vec<CreateShipmentCommand>,
}

#[derive(Debug, Serialize)]
pub struct BulkCreateResult {
    pub created: Vec<uuid::Uuid>,
    pub failed: Vec<BulkRowError>,
}

#[derive(Debug, Serialize)]
pub struct BulkRowError {
    pub row_index: usize,
    pub merchant_reference: Option<String>,
    pub error: String,
}
