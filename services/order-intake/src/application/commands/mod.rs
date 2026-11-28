use serde::{Deserialize, Serialize};
use validator::Validate;

/// Per-piece declaration for Balikbayan / International bookings.
/// Standard/Express/SameDay bookings must NOT include this — they use `piece_count`.
#[derive(Debug, Deserialize, Validate, Serialize, Clone)]
pub struct PieceInput {
    /// Declared weight of this individual piece in grams (100 g – 150 kg).
    #[validate(range(min = 100, max = 150_000))]
    pub weight_grams: u32,
    pub length_cm:    Option<u32>,
    pub width_cm:     Option<u32>,
    pub height_cm:    Option<u32>,
    /// Short content description, e.g. "Clothes", "Electronics".
    pub description:  Option<String>,
}

/// A phone reduced to an optional leading `+` and digits. Anything from the
/// first letter, `#` or `;` on is an extension ("ext. 12", "x12") and dropped.
pub fn normalize_phone(raw: &str) -> String {
    let main = raw.split(|c: char| c.is_alphabetic() || c == '#' || c == ';').next().unwrap_or("");
    let main = main.trim();
    let mut out = String::with_capacity(main.len());
    if main.starts_with('+') {
        out.push('+');
    }
    out.extend(main.chars().filter(char::is_ascii_digit));
    out
}

/// Length bounds for a normalised phone, counting the `+`.
pub fn phone_in_range(normalized: &str) -> bool {
    (7..=20).contains(&normalized.chars().count())
}

/// The single-parcel weight cap, as `ShipmentService::create` applies it: not
/// to a per-piece Balikbayan/International declaration (whose `weight_grams`
/// is the sum of its boxes) nor to a household move (tonnes). A flat
/// `range(max = 70000)` on the field rejected those, so the cap lives here.
fn weight_within_cap(cmd: &CreateShipmentCommand) -> Result<(), validator::ValidationError> {
    if !phone_in_range(&normalize_phone(&cmd.customer_phone)) {
        return Err(validator::ValidationError::new("customer_phone"));
    }
    if cmd.weight_grams < 1 {
        return Err(validator::ValidationError::new("range"));
    }
    let uncapped = cmd.service_type == "home_move"
        || (matches!(cmd.service_type.as_str(), "balikbayan" | "international")
            && cmd.pieces.as_ref().is_some_and(|p| !p.is_empty()));
    if !uncapped && cmd.weight_grams > 70_000 {
        return Err(validator::ValidationError::new("range"));
    }
    Ok(())
}

#[derive(Debug, Deserialize, Validate)]
#[validate(schema(function = "weight_within_cap"))]
pub struct CreateShipmentCommand {
    #[serde(default)]
    pub tenant_id: uuid::Uuid,
    #[serde(default)]
    pub merchant_id: uuid::Uuid,

    #[validate(length(min = 1, max = 200))]
    pub customer_name: String,
    pub customer_phone: String,      // bounded by `weight_within_cap` after `normalize_phone`
    pub customer_email: Option<String>,

    /// Sender identity fields — the person handing parcels to the courier.
    /// Optional; when present, the CDP upserts a Sender profile for marketing use.
    #[serde(default)]
    pub sender_name:  Option<String>,
    #[serde(default)]
    pub sender_phone: Option<String>,
    #[serde(default)]
    pub sender_email: Option<String>,

    pub origin: AddressInput,
    pub destination: AddressInput,

    pub service_type: String,       // "standard" | "express" | "same_day" | "balikbayan"
    pub weight_grams: u32,          // bounded by `weight_within_cap`
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

    /// Number of physical pieces (1..=999). Used for Standard/Express/SameDay.
    /// Ignored when `pieces` is provided (Balikbayan/International).
    pub piece_count: Option<u16>,

    /// Per-piece declarations for Balikbayan and International shipments.
    /// Required when `service_type` is "balikbayan" or "international" and
    /// individual box weights/dims differ. Omit for Standard/Express/SameDay.
    #[serde(default)]
    pub pieces: Option<Vec<PieceInput>>,

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

    /// Merchant / sender display name shown on the driver's pickup task card.
    /// Sent by the merchant portal; omitted on customer-app self-bookings.
    #[serde(default)]
    pub merchant_name: Option<String>,

    /// Shipment category for the driver task icon and vehicle matching:
    /// "food" | "parcel" | "grocery" | "medicine" | "heavy" | "large".
    /// Derived from service_type/weight when omitted.
    #[serde(default)]
    pub delivery_category: Option<String>,

    /// A signed quote token from `POST /v1/shipments/quote`. When present,
    /// the shipment is created in `awaiting_payment` and dispatch is held
    /// until the corresponding payment intent captures.
    #[serde(default)]
    pub quote_token: Option<String>,
    /// Set only by the home-move booking path, never from a request body: the
    /// booked move time decides the cancellation tier, so a client that could
    /// set it could choose its own fee.
    #[serde(skip)]
    pub home_booking: Option<HomeBooking>,
    /// How the booking was described, when it came from the prompt box or
    /// voice: what was read from the sentence, before the customer corrected
    /// it. Stored for measuring the reader; never used to price or route.
    #[serde(default)]
    pub intake: Option<IntakeInput>,
    /// Client-generated idempotency key — a retry with the same key returns
    /// the shipment already created for it instead of creating a duplicate
    /// (and a duplicate charge).
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Validate, Serialize)]
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
    /// Where the customer placed the pin. Both or neither: one alone, a value
    /// out of range, or (0, 0) is refused at booking rather than dropped.
    #[serde(default)]
    pub pin_lat: Option<f64>,
    #[serde(default)]
    pub pin_lng: Option<f64>,
    /// UAE-style address parts; kept with the destination, never geocoded.
    #[serde(default)]
    #[validate(length(max = 100))]
    pub building: Option<String>,
    #[serde(default)]
    #[validate(length(max = 100))]
    pub unit: Option<String>,
    #[serde(default)]
    #[validate(length(max = 100))]
    pub area: Option<String>,
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
    /// Not length-validated: a connector order is already paid for and must
    /// never be refused over phone format. See `internal_create_shipment`.
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
    // Populated from the URL path by the handler, not the request body.
    #[serde(default)]
    pub shipment_id: uuid::Uuid,
    pub preferred_date: chrono::NaiveDate,
    pub preferred_time_slot: Option<String>, // "morning" | "afternoon" | "anytime"
    pub reason: String,
    /// Set in code by the caller, never read from the request body. Defaults to
    /// `Unset`, which the service refuses.
    #[serde(skip)]
    pub acting_as: crate::domain::value_objects::cancel_authority::ActingAs,
}

#[derive(Debug, Deserialize)]
pub struct CancelShipmentCommand {
    // Populated from the URL path by the handler, not the request body.
    #[serde(default)]
    pub shipment_id: uuid::Uuid,
    pub reason: String,
    /// Set in code by the caller, never read from the request body. Defaults to
    /// `Unset`, which the service refuses.
    #[serde(skip)]
    pub acting_as: crate::domain::value_objects::cancel_authority::ActingAs,
}

#[derive(Debug, Deserialize)]
pub struct BulkCreateShipmentCommand {
    // Populated from JWT claims by the handler, not the request body.
    #[serde(default)]
    pub tenant_id: uuid::Uuid,
    #[serde(default)]
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

/// A whole-home move being booked: the move time, already held to the
/// calendar by the home-move path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HomeBooking {
    pub move_at: chrono::DateTime<chrono::Utc>,
    /// What the lead who takes it must bring; carried on shipment.created.
    pub requirement: logisticos_events::payloads::HomeMoveRequirement,
}

/// What the prompt box read, as the app sends it with the booking.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct IntakeInput {
    /// prompt_ai | prompt_offline | voice_ai | voice_offline
    pub source: String,
    #[serde(default)]
    pub intent: Option<String>,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub extracted: serde_json::Value,
}

/// The largest `extracted` kept. A sentence's worth of items and two
/// addresses is well under this; anything bigger is not a prompt's parse.
pub const INTAKE_MAX_BYTES: usize = 8 * 1024;

impl IntakeInput {
    /// What is stored, or None for anything that is not a prompt's parse —
    /// an unknown source, an oversized payload. Never an error: intake is
    /// measurement, and a booking must not fail over it.
    pub fn to_store(&self) -> Option<(String, Option<String>, Option<f64>, serde_json::Value)> {
        let source = self.source.trim();
        if !matches!(source, "prompt_ai" | "prompt_offline" | "voice_ai" | "voice_offline") {
            return None;
        }
        if serde_json::to_vec(&self.extracted).map_or(true, |b| b.len() > INTAKE_MAX_BYTES) {
            return None;
        }
        let intent = self.intent.as_deref().filter(|i| matches!(*i, "book" | "home_move" | "support")).map(str::to_owned);
        let confidence = self.confidence.filter(|c| c.is_finite()).map(|c| c.clamp(0.0, 1.0));
        let extracted = if self.extracted.is_object() { self.extracted.clone() } else { serde_json::json!({}) };
        Some((source.to_owned(), intent, confidence, extracted))
    }
}

#[cfg(test)]
mod intake_tests {
    use super::*;

    fn input(source: &str, extracted: serde_json::Value) -> IntakeInput {
        IntakeInput { source: source.into(), intent: Some("book".into()), confidence: Some(1.4), extracted }
    }

    #[test]
    fn a_prompt_parse_is_kept_with_its_numbers_clamped() {
        let (source, intent, confidence, extracted) =
            input("prompt_ai", serde_json::json!({ "from": "BGC" })).to_store().unwrap();
        assert_eq!((source.as_str(), intent.as_deref(), confidence), ("prompt_ai", Some("book"), Some(1.0)));
        assert_eq!(extracted["from"], "BGC");
    }

    #[test]
    fn anything_else_is_dropped_not_refused() {
        assert!(input("spreadsheet", serde_json::json!({})).to_store().is_none());
        let huge = serde_json::json!({ "from": "x".repeat(INTAKE_MAX_BYTES) });
        assert!(input("prompt_ai", huge).to_store().is_none());
    }

    #[test]
    fn an_unknown_intent_is_not_recorded_as_one() {
        let mut i = input("voice_offline", serde_json::json!([1, 2]));
        i.intent = Some("launch".into());
        let (_, intent, _, extracted) = i.to_store().unwrap();
        assert!(intent.is_none());
        assert_eq!(extracted, serde_json::json!({}));
    }
}

#[cfg(test)]
mod phone_tests {
    use super::*;

    #[test]
    fn formatting_and_extensions_are_dropped_and_a_leading_plus_kept() {
        assert_eq!(normalize_phone("+971 (50) 123-4567 ext. 12"), "+971501234567");
        assert_eq!(normalize_phone("0917 123 4567 x45"), "09171234567");
        assert_eq!(normalize_phone("+63.917.123.4567 #9"), "+639171234567");
        assert!(!phone_in_range(&normalize_phone("n/a")));
        assert!(!phone_in_range(&normalize_phone("12345")));
    }
}
