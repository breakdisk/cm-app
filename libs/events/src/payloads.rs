use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantCreated {
    pub tenant_id: Uuid,
    pub name: String,
    pub slug: String,
    pub owner_email: String,
    pub owner_user_id: Uuid,
    pub subscription_tier: String,
}

// Enriched ShipmentCreated — consumed by dispatch, engagement, analytics, delivery-experience
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipmentCreated {
    pub shipment_id:          Uuid,
    pub merchant_id:          Uuid,
    pub customer_id:          Uuid,
    pub customer_name:        String,
    pub customer_phone:       String,
    /// Customer email — used by engagement for receipt delivery. Empty if not provided.
    #[serde(default)]
    pub customer_email:       String,
    pub origin_address:       String,
    /// Structured origin (added so dispatch can create a pickup task with a real
    /// street/city, not just the flattened "city, province" string above).
    #[serde(default)]
    pub origin_city:          String,
    #[serde(default)]
    pub origin_province:      String,
    #[serde(default)]
    pub origin_postal_code:   String,
    #[serde(default)]
    pub origin_lat:           Option<f64>,
    #[serde(default)]
    pub origin_lng:           Option<f64>,
    pub destination_address:  String,
    pub destination_city:     String,
    pub destination_lat:      Option<f64>,
    pub destination_lng:      Option<f64>,
    pub service_type:         String,
    pub cod_amount_cents:     Option<i64>,
    /// AWB tracking number (e.g. "CM-PH1-S0001234X"). Engagement uses this for
    /// the shipment confirmation receipt and tracking link.
    #[serde(default)]
    pub tracking_number:      String,
    /// Total fee in cents (base freight + surcharges). Shown on receipt.
    #[serde(default)]
    pub total_fee_cents:      i64,
    /// Currency code (e.g. "PHP"). Shown on receipt.
    #[serde(default = "default_currency")]
    pub currency:             String,
    /// Declared weight in grams. Shown on receipt.
    #[serde(default)]
    pub weight_grams:         u32,
    /// Estimated delivery date/range as a human-readable string.
    #[serde(default)]
    pub estimated_delivery:   String,
    /// True when the booking originated from the customer app. Drives billing
    /// semantics in payments (PaymentReceipt vs merchant invoice) — NOT used
    /// by dispatch anymore. Defaults to false for backwards compatibility.
    #[serde(default)]
    pub booked_by_customer:   bool,
    /// True when dispatch should auto-assign a driver immediately on creation.
    /// Set by the order-intake HTTP handler (customer + merchant roles default
    /// true; admin defaults false). Defaults to false for backwards
    /// compatibility with events emitted before this field existed.
    #[serde(default)]
    pub auto_dispatch:        bool,
}

fn default_currency() -> String { "PHP".into() }
fn default_task_type() -> String { "delivery".into() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverAssigned {
    pub shipment_id: Uuid,
    pub driver_id: Uuid,
    pub route_id: Uuid,
    pub estimated_pickup_time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryCompleted {
    pub shipment_id: Uuid,
    pub driver_id: Uuid,
    pub pod_id: Uuid,
    pub delivered_at: String,
    pub recipient_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryFailed {
    pub shipment_id: Uuid,
    pub driver_id: Uuid,
    pub reason: String,
    pub attempted_at: String,
    pub attempt_number: u32,
    pub next_attempt_scheduled: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationUpdated {
    pub driver_id: Uuid,
    pub lat: f64,
    pub lng: f64,
    pub timestamp: String,
    pub speed_kmh: Option<f32>,
    pub heading: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PodCaptured {
    pub pod_id:             Uuid,
    pub shipment_id:        Uuid,
    pub driver_id:          Uuid,
    pub pod_type:           String,   // "signature" | "photo" | "otp"
    pub captured_at:        String,
    pub lat:                Option<f64>,
    pub lng:                Option<f64>,
    /// 3-char tenant code for invoice number generation (e.g. "PH1").
    #[serde(default)]
    pub tenant_code:        String,
    /// True when the shipment was booked by a customer via the customer app.
    /// Payments service checks this to issue a PaymentReceipt at POD.
    #[serde(default)]
    pub booked_by_customer: bool,
    /// Customer UUID — populated when `booked_by_customer` is true.
    #[serde(default)]
    pub customer_id:        Option<Uuid>,
    /// Customer email for receipt delivery — populated when `booked_by_customer` is true.
    #[serde(default)]
    pub customer_email:     Option<String>,
    /// COD amount collected at doorstep (0 if non-COD).
    #[serde(default)]
    pub cod_amount_cents:   i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodCollected {
    pub shipment_id: Uuid,
    pub driver_id: Uuid,
    pub amount_cents: i64,
    pub currency: String,
    pub collected_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserCreated {
    pub user_id:   Uuid,
    pub tenant_id: Uuid,
    pub email:     String,
    pub roles:     Vec<String>,
}

// ── AWB / Piece events ────────────────────────────────────────────────────────

/// Emitted by order-intake when a master AWB is issued at booking.
/// Consumed by: analytics (audit log), dispatch (pre-register for route planning),
/// payments (initialise billable record).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AwbIssued {
    pub awb:          String,    // e.g. "CM-PH1-S0001234X"
    pub tenant_id:    Uuid,
    pub shipment_id:  Uuid,
    pub merchant_id:  Uuid,
    pub service_code: String,   // "standard" | "express" | "same_day" | "balikbayan" | "international"
    pub sequence:     u32,
    pub piece_count:  u16,       // total pieces declared at booking
    pub issued_at:    String,    // ISO-8601
}

/// Emitted by hub-ops when an individual piece is scanned at a hub.
/// Consumed by: delivery-experience (translate to customer-visible status),
/// payments (trigger storage fee timer), analytics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PieceScanned {
    pub piece_awb:    String,   // e.g. "CM-PH1-B0009012Z-002"
    pub master_awb:   String,   // e.g. "CM-PH1-B0009012Z"
    pub shipment_id:  Uuid,
    pub tenant_id:    Uuid,
    pub hub_id:       Uuid,
    pub scan_type:    String,   // "inbound" | "outbound" | "transfer"
    pub piece_number: u16,
    pub piece_count:  u16,      // total pieces in this shipment
    pub scanned_at:   String,   // ISO-8601
    pub scanned_by:   Uuid,     // user_id of hub operator
}

/// Emitted by hub-ops when re-weighing reveals a discrepancy vs declared weight.
/// Consumed by: payments (generate weight surcharge adjustment invoice).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightDiscrepancyFound {
    pub piece_awb:       String,
    pub master_awb:      String,
    pub shipment_id:     Uuid,
    pub tenant_id:       Uuid,
    pub merchant_id:     Uuid,
    pub hub_id:          Uuid,
    pub declared_grams:  u32,
    pub actual_grams:    u32,
    pub delta_grams:     i32,   // actual - declared (positive = underweight declared)
    pub found_at:        String,
    pub found_by:        Uuid,
}

// ── Pallet / Container events ─────────────────────────────────────────────────

/// Emitted by hub-ops when a pallet is sealed (no more pieces can be added).
/// Consumed by: fleet (include in container planning), analytics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PalletSealed {
    pub pallet_id:        Uuid,
    pub tenant_id:        Uuid,
    pub hub_id:           Uuid,
    pub destination_hub:  Option<Uuid>,
    pub piece_count:      u16,
    pub total_weight_kg:  f32,
    pub sealed_at:        String,
    pub sealed_by:        Uuid,
}

/// Emitted by fleet/hub-ops when a container departs its origin hub.
/// Consumed by: delivery-experience (update shipment status to InTransit with ETA),
/// analytics, carrier management.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerDeparted {
    pub container_id:     Uuid,
    pub tenant_id:        Uuid,
    pub origin_hub_id:    Uuid,
    pub destination_hub:  Uuid,
    pub transport_mode:   String, // "road" | "sea_fcl" | "sea_lcl" | "air_uld" | "air_loose"
    pub pallet_count:     u16,
    pub loose_piece_count: u16,
    pub carrier_ref:      Option<String>, // carrier's own manifest number
    pub departed_at:      String,
    pub eta:              Option<String>,
    /// All master AWBs in this container (for bulk status update).
    pub master_awbs:      Vec<String>,
}

/// Emitted by fleet/hub-ops when a container arrives at destination hub.
/// Consumed by: delivery-experience (trigger OutForDelivery flow once unloaded),
/// analytics, hub-ops (trigger pallet break-up workflow).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerArrived {
    pub container_id:     Uuid,
    pub tenant_id:        Uuid,
    pub destination_hub:  Uuid,
    pub arrived_at:       String,
    pub master_awbs:      Vec<String>,
}

// ── Invoice events ────────────────────────────────────────────────────────────

/// Emitted by payments when any invoice document is finalised.
/// Consumed by: engagement (send invoice notification to merchant),
/// analytics, merchant portal (refresh invoice list).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceFinalized {
    pub invoice_number:  String,    // e.g. "IN-PH1-2026-04-00001"
    pub invoice_id:      Uuid,
    pub invoice_type:    String,    // "shipment_charges" | "cod_remittance" | etc.
    pub tenant_id:       Uuid,
    pub merchant_id:     Option<Uuid>,
    pub carrier_id:      Option<Uuid>,
    pub total_cents:     i64,
    pub currency:        String,
    pub due_date:        Option<String>,
    pub finalized_at:    String,
}

/// Emitted by payments when a COD remittance is ready to be paid out.
/// Consumed by: engagement (notify merchant of incoming funds),
/// payments (initiate bank transfer).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodRemittanceReady {
    pub remittance_number: String,  // e.g. "REM-PH1-2026-04-00001"
    pub invoice_id:        Uuid,
    pub tenant_id:         Uuid,
    pub merchant_id:       Uuid,
    pub net_amount_cents:  i64,
    pub currency:          String,
    pub shipment_count:    u16,
    pub settlement_date:   String,
}

/// Emitted by payments when a weight discrepancy adjustment invoice is created.
/// Consumed by: engagement (notify merchant of surcharge), analytics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightAdjustmentInvoiced {
    pub invoice_number:    String,   // CN or IN type
    pub invoice_id:        Uuid,
    pub master_awb:        String,
    pub piece_awb:         String,
    pub shipment_id:       Uuid,
    pub tenant_id:         Uuid,
    pub merchant_id:       Uuid,
    pub surcharge_cents:   i64,
    pub currency:          String,
    pub declared_grams:    u32,
    pub actual_grams:      u32,
}

/// Emitted by dispatch when a shipment is assigned to a driver.
/// Contains all data driver-ops needs to create a DriverTask row
/// without querying other services.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAssigned {
    pub task_id:              Uuid,   // Pre-generated UUID for the task
    pub assignment_id:        Uuid,
    pub shipment_id:          Uuid,
    pub route_id:             Uuid,
    pub driver_id:            Uuid,
    pub tenant_id:            Uuid,
    pub sequence:             i32,
    /// "pickup" | "delivery". Defaults to "delivery" so events emitted before
    /// this field existed (single-leg dispatch) keep their original meaning.
    #[serde(default = "default_task_type")]
    pub task_type:            String,
    // Stop address — for pickup tasks this is the origin (sender), for
    // delivery tasks this is the destination (recipient).
    pub address_line1:        String,
    pub address_city:         String,
    pub address_province:     String,
    pub address_postal_code:  String,
    pub address_lat:          Option<f64>,
    pub address_lng:          Option<f64>,
    // Customer (denormalized for driver app display)
    pub customer_name:        String,
    pub customer_phone:       String,
    pub cod_amount_cents:     Option<i64>,
    pub special_instructions: Option<String>,
    /// AWB tracking number — carried through to TaskCompleted so engagement
    /// can send delivery receipt without querying order-intake.
    #[serde(default)]
    pub tracking_number:      String,
    /// Customer email — forwarded to engagement for delivery receipt email.
    #[serde(default)]
    pub customer_email:       String,
}

/// Emitted by delivery-experience when a customer taps "Email Receipt" on the
/// ReceiptScreen / CollectionScreen. Engagement consumes this topic and sends
/// a single email to `recipient_email` using the shipment_confirmation
/// template. Unlike ShipmentCreated (which fans out to WhatsApp + Email on
/// initial booking), this is an explicit one-off email-only re-send triggered
/// by the customer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptEmailRequested {
    pub shipment_id:         Uuid,
    pub tracking_number:     String,
    /// Email address the customer typed into the "Email Receipt" input.
    /// May differ from the customer_email captured at booking time.
    pub recipient_email:     String,
    pub origin_address:      String,
    pub destination_address: String,
    /// Optional: tracking projection doesn't capture customer_id today, so
    /// engagement falls back to shipment_id for the notification audit record.
    #[serde(default)]
    pub customer_id:         Option<Uuid>,
    #[serde(default)]
    pub customer_name:       String,
}
