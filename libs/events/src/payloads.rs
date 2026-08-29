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

/// Emitted by identity when an OTP is generated for passwordless login.
/// Consumed by: engagement (delivers the code via email or SMS).
/// tenant_id is Uuid::nil() — OTP send happens before tenant resolution.
/// Exactly one of `email` / `phone_number` is non-empty.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtpRequested {
    /// The email address the OTP should be delivered to.
    /// Empty when phone_number is set.
    pub email: String,
    /// The 6-digit OTP code.
    pub otp_code: String,
    /// E.164 phone number for SMS OTP delivery (e.g. "+639171234567").
    /// Empty when email is set.
    #[serde(default)]
    pub phone_number: String,
}

/// Emitted by identity when a draft tenant completes onboarding setup.
/// Consumed by: engagement (send welcome email), billing (create billing account).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantFinalized {
    pub tenant_id:    Uuid,
    pub name:         String,
    pub owner_email:  String,
    pub finalized_at: String,
    /// ISO 4217 currency code selected during onboarding (e.g. "PHP", "AED").
    #[serde(default)]
    pub currency:     String,
    /// ISO 3166-1 alpha-2 region code selected during onboarding (e.g. "PH", "AE").
    #[serde(default)]
    pub region:       String,
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
    #[serde(default)]
    pub destination_province:     String,
    #[serde(default)]
    pub destination_postal_code:  String,
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
    /// Free-text delivery notes from the merchant (e.g. "Leave at reception").
    /// Passed through to the dispatch task and shown to the driver.
    #[serde(default)]
    pub special_instructions: Option<String>,
    /// Sender identity — the person handing the parcel to the courier.
    /// Populated when the merchant portal sends sender_name/phone in the booking form.
    #[serde(default)]
    pub sender_name:          Option<String>,
    #[serde(default)]
    pub sender_phone:         Option<String>,
    #[serde(default)]
    pub sender_email:         Option<String>,
    /// Merchant / sender display name. Event-time passthrough from the booking
    /// request — persisted downstream in dispatch_queue and driver_ops.tasks so
    /// the driver app can show who the pickup party is. Empty when unknown.
    #[serde(default)]
    pub merchant_name:        String,
    /// Shipment category driving the driver-app task icon and vehicle matching:
    /// "food" | "parcel" | "grocery" | "medicine" | "heavy" | "large".
    /// Derived by order-intake when not supplied at booking.
    #[serde(default = "default_delivery_category")]
    pub delivery_category:    String,
}

fn default_currency() -> String { "PHP".into() }
fn default_task_type() -> String { "delivery".into() }
fn default_delivery_category() -> String { "parcel".into() }

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
    #[serde(default)]
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
    /// Customer / recipient name — serialised as "recipient_name" by the pod service
    /// (the confirmed name of the person who received the parcel).  The alias lets
    /// this canonical struct deserialize both legacy "recipient_name" events and newer
    /// "customer_name" events without field duplication.
    #[serde(default, alias = "recipient_name")]
    pub customer_name:      String,
    /// Customer phone — for engagement to send COD receipt / delivery WhatsApp.
    /// Populated from SubmitPodCommand.customer_phone (driver app carries it from
    /// the task summary screen).
    #[serde(default)]
    pub customer_phone:     String,
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

/// Published by `services/payments` when a Network-International-backed
/// `payment_intents` row transitions to `captured`. `reference_id` is
/// whatever the intent's `purpose` says it is — for `purpose = "shipping_fee"`
/// it is the order-intake `shipment_id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentIntentCaptured {
    pub intent_id: Uuid,
    pub purpose: String,
    pub reference_type: String,
    pub reference_id: Uuid,
    pub amount_cents: i64,
    pub currency: String,
}

/// Published by `services/payments` when a `PaymentAction::Authorize`
/// intent's `AUTHORISED` webhook lands — funds are ring-fenced on the
/// customer's card but NOT yet taken. Distinct from `PaymentIntentCaptured`
/// on purpose: a consumer that reacted to this the same way it reacts to a
/// capture would be treating a hold as if the money had already moved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentIntentAuthorized {
    pub intent_id: Uuid,
    pub purpose: String,
    pub reference_type: String,
    pub reference_id: Uuid,
    pub amount_cents: i64,
    pub currency: String,
}

/// Published when an intent is marked `failed` (declined at the gateway) —
/// distinct from `expired`, which the consumer treats identically but which
/// is driven by the sweep, not a webhook.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentIntentFailed {
    pub intent_id: Uuid,
    pub purpose: String,
    pub reference_type: String,
    pub reference_id: Uuid,
    pub reason: String,
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
    /// Customer UUID — forwarded to driver-ops so TaskCompleted can carry it
    /// to engagement for delivery receipt notification routing.
    #[serde(default)]
    pub customer_id:          Option<Uuid>,
    /// Merchant / sender display name — shown on the driver task card.
    #[serde(default)]
    pub merchant_name:        String,
    /// "food" | "parcel" | "grocery" | "medicine" | "heavy" | "large" — drives
    /// the task-card icon in the driver app and vehicle matching at dispatch.
    #[serde(default = "default_delivery_category")]
    pub delivery_category:    String,
    /// Declared shipment weight in grams (0 = unknown).
    #[serde(default)]
    pub weight_grams:         u32,
    // Full route ends (both legs carry both ends so the driver app can render
    // the pickup→delivery route sketch on every task card).
    #[serde(default)]
    pub pickup_lat:           Option<f64>,
    #[serde(default)]
    pub pickup_lng:           Option<f64>,
    #[serde(default)]
    pub delivery_lat:         Option<f64>,
    #[serde(default)]
    pub delivery_lng:         Option<f64>,
    /// Contractual gig payout in cents, snapshotted at assignment/claim time
    /// from the driver's per_delivery_rate_cents. None for full-time drivers —
    /// the app must never show them a price. This snapshot is the earnings
    /// record: later rate changes never alter accepted work.
    #[serde(default)]
    pub payout_cents:         Option<i64>,
}

/// Broadcast gig offer — fanned out to N candidate drivers simultaneously.
/// Emitted by dispatch when a shipment is broadcast to the gig pool;
/// driver-ops consumes it to send a per-candidate FCM `task_offer` push.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskOfferCreated {
    pub offer_id:     Uuid,
    pub tenant_id:    Uuid,
    pub shipment_id:  Uuid,
    pub wave:         i32,
    pub expires_at:   chrono::DateTime<chrono::Utc>,
    /// Payout snapshotted per candidate from their per_delivery_rate_cents at
    /// offer creation — rates are per-driver, so each candidate sees their own
    /// contractual price; the winner's is copied to the task on claim.
    pub candidates:   Vec<OfferCandidate>,
    // Card display fields (mirror the TaskAssigned card enrichment)
    #[serde(default)]
    pub merchant_name:     String,
    #[serde(default = "default_delivery_category")]
    pub delivery_category: String,
    #[serde(default)]
    pub weight_grams:      u32,
    #[serde(default)]
    pub tracking_number:   String,
    #[serde(default)]
    pub customer_name:     String,
    #[serde(default)]
    pub pickup_address:    String,
    #[serde(default)]
    pub delivery_address:  String,
    #[serde(default)]
    pub cod_amount_cents:  Option<i64>,
    #[serde(default)]
    pub pickup_lat:        Option<f64>,
    #[serde(default)]
    pub pickup_lng:        Option<f64>,
    #[serde(default)]
    pub delivery_lat:      Option<f64>,
    #[serde(default)]
    pub delivery_lng:      Option<f64>,
}

/// One candidate of a broadcast task offer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfferCandidate {
    pub driver_id:    Uuid,
    pub payout_cents: Option<i64>,
}

/// Offer reached a terminal state (claimed / expired / cancelled).
/// driver-ops fans an FCM `offer_closed` to every losing candidate so their
/// grab cards flip to "Taken" and dismiss in real time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskOfferClosed {
    pub offer_id:    Uuid,
    pub tenant_id:   Uuid,
    pub shipment_id: Uuid,
    /// "claimed" | "expired" | "cancelled"
    pub reason:      String,
    pub claimed_by:  Option<Uuid>,
    /// Everyone who was ever offered it (all waves) — FCM fan-in targets.
    pub candidate_driver_ids: Vec<Uuid>,
}

/// Emitted by dispatch when a driver rejects (declines) an assignment.
/// Consumed by driver-ops to maintain the driver's decline counter and apply
/// the automatic ban at the decline threshold; also the future hook for
/// automatic re-dispatch of the declined shipment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignmentRejected {
    pub assignment_id: Uuid,
    pub driver_id:     Uuid,
    pub tenant_id:     Uuid,
    pub route_id:      Uuid,
    pub reason:        String,
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

/// Emitted by pod service when a driver submits a Proof of Pickup.
/// Opens the chain of custody. Consumed by:
///   - payments (driver ledger debit for Track A balikbayan pickups)
///   - driver-ops (mark task IN_PROGRESS)
///   - analytics (chain-of-custody audit trail)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PickupCaptured {
    pub pop_id:             Uuid,
    pub shipment_id:        Uuid,
    pub task_id:            Uuid,
    pub tenant_id:          Uuid,
    pub driver_id:          Uuid,
    pub geofence_verified:  bool,
    /// Soft audit flag — driver was > 50m from pickup address.
    #[serde(default)]
    pub out_of_bounds_handover: bool,
    pub barcode_scanned:    bool,
    /// Service code from the shipment booking:
    /// "standard" | "express" | "same_day" | "balikbayan" | "international"
    /// Payments uses this to select the billing strategy for ledger debit.
    #[serde(default)]
    pub service_code:       String,
    /// Declared cargo value in cents — used for Track A (balikbayan) driver
    /// custody liability debit. Absent when not provided at booking.
    #[serde(default)]
    pub declared_value_cents: Option<i64>,
    #[serde(default)]
    pub actual_weight_g:    Option<i64>,
    #[serde(default)]
    pub declared_weight_g:  Option<i64>,
    /// Weight overage ratio (actual−declared)/declared — positive = over declared.
    #[serde(default)]
    pub weight_overage_ratio: Option<f64>,
    #[serde(default)]
    pub photo_s3_key:       Option<String>,
    /// Server receipt timestamp (ISO-8601).
    pub captured_at:        String,
    /// Driver device clock timestamp (ISO-8601). Use for SLA / chain-of-custody.
    #[serde(default)]
    pub device_timestamp:   Option<String>,
    /// 3-char tenant code for invoice number generation.
    #[serde(default)]
    pub tenant_code:        String,
}

/// Emitted by driver-ops when a driver taps "Go Online" in the app.
/// Consumed by: dispatch (retry any pending queue items for this tenant).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverAvailable {
    pub driver_id:    Uuid,
    pub tenant_id:    Uuid,
    pub available_at: String,  // ISO-8601
}

// ── Carrier events ────────────────────────────────────────────────────────────

/// Emitted by carrier service when a new carrier is onboarded (saved with
/// status = pending_verification). Consumed by: analytics, compliance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CarrierOnboarded {
    pub carrier_id:    Uuid,
    pub tenant_id:     Uuid,
    pub name:          String,
    pub code:          String,
    pub contact_email: String,
}

/// Emitted by carrier service when a carrier's status changes
/// (activate, suspend, deactivate). Consumed by: dispatch (cache invalidation),
/// analytics, compliance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CarrierStatusChanged {
    pub carrier_id:  Uuid,
    pub tenant_id:   Uuid,
    pub old_status:  String,
    pub new_status:  String,
    /// Human-readable reason, e.g. suspension reason. Empty string if none.
    pub reason:      String,
}

// ── Marketplace events ────────────────────────────────────────────────────────

/// Emitted by carrier service when a carrier accepts a marketplace booking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceBookingAccepted {
    pub booking_id:  Uuid,
    pub listing_id:  Uuid,
    pub carrier_id:  Uuid,
    pub tenant_id:   Uuid,
    pub accepted_at: String,
}

/// Emitted by carrier service when a carrier rejects a marketplace booking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceBookingRejected {
    pub booking_id:   Uuid,
    pub listing_id:   Uuid,
    pub carrier_id:   Uuid,
    pub tenant_id:    Uuid,
    pub rejected_at:  String,
}

/// Emitted by carrier service when a carrier confirms a cargo pickup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplacePickupRecorded {
    pub booking_id:    Uuid,
    pub listing_id:    Uuid,
    pub carrier_id:    Uuid,
    pub tenant_id:     Uuid,
    pub picked_up_by:  Option<String>,
    pub pickup_notes:  Option<String>,
    pub picked_up_at:  String,
}

/// Emitted by carrier service when an inbound 3PL tracking webhook is received
/// and authenticated. Consumed by: delivery-experience (status update),
/// engagement (customer notification), analytics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CarrierTrackingEvent {
    pub carrier_id:      Uuid,
    pub tenant_id:       Uuid,
    /// Carrier's own shipment/tracking reference.
    pub carrier_ref:     Option<String>,
    /// LogisticOS shipment UUID (preferred; may be absent for pure-tracking events).
    pub shipment_id:     Option<Uuid>,
    /// Carrier status code — e.g. "picked_up", "in_transit", "delivered", "failed".
    pub event:           String,
    /// Human-readable message from the 3PL.
    pub message:         Option<String>,
    /// ISO-8601 receipt timestamp (server clock).
    pub received_at:     String,
}

/// Emitted by carrier service when dispatch records a carrier allocation for a
/// shipment (POST /v1/internal/sla-records). Consumed by: analytics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CarrierAllocated {
    pub carrier_id:       Uuid,
    pub tenant_id:        Uuid,
    pub shipment_id:      Uuid,
    pub zone:             String,
    pub service_level:    String,
    pub total_cost_cents: i64,
    /// ISO-8601 datetime string — when delivery is expected by.
    pub promised_by:      String,
    /// "rate_shop" (auto-selected cheapest) | "manual" (operator chose).
    pub method:           String,
}

// ── Cross-Border Hub Transfer events ──────────────────────────────────────────

/// Per-shipment detail attached to container **milestone** events, supplied by the
/// arrive/customs HTTP requests (the "enrich from request" approach). A container
/// covers many shipments, so milestone events carry a list of these.
///
/// Engagement uses the `customer_*` fields to notify recipients; payments uses the
/// `merchant_*` / `duty_cents` fields (on customs-cleared) to bill duties.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ShipmentMilestoneDetail {
    pub shipment_id:     Uuid,
    #[serde(default)] pub master_awb:      String,
    #[serde(default)] pub tracking_number: String,
    // Recipient (engagement)
    #[serde(default)] pub customer_id:     Option<Uuid>,
    #[serde(default)] pub customer_name:   String,
    #[serde(default)] pub customer_phone:  String,
    #[serde(default)] pub customer_email:  String,
    // Payer (payments duties — customs-cleared only)
    #[serde(default)] pub merchant_id:     Option<Uuid>,
    #[serde(default)] pub merchant_email:  String,
    #[serde(default)] pub duty_cents:      i64,
}


/// Emitted by hub-ops on an `InboundReceive` scan at the origin hub.
/// Consumed by: order-intake (ShipmentStatus::AtHub), analytics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PieceScannedInbound {
    pub piece_awb:        String,
    pub master_awb:       String,
    pub shipment_id:      Uuid,
    pub tenant_id:        Uuid,
    pub hub_id:           Uuid,
    pub scanned_by:       Uuid,
    /// Hardware clock at scan moment (ISO-8601). Primary basis for SLA.
    #[serde(default)]
    pub device_timestamp: Option<String>,
    /// Backend receipt time (ISO-8601).
    pub server_timestamp: String,
}

/// Emitted by hub-ops when a sea/air container reaches the destination
/// port/airport. Consumed by: engagement (customer "at port" notification).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerArrivedAtPort {
    pub container_id: Uuid,
    pub tenant_id:    Uuid,
    pub port_hub_id:  Option<Uuid>,
    pub master_awbs:  Vec<String>,
    /// Per-shipment recipient detail for customer notifications.
    #[serde(default)]
    pub details:      Vec<ShipmentMilestoneDetail>,
    pub arrived_at:   String,
}

/// Emitted by hub-ops when a container enters customs hold.
/// Consumed by: order-intake (ShipmentStatus::CustomsHold), engagement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerCustomsHold {
    pub container_id: Uuid,
    pub tenant_id:    Uuid,
    pub master_awbs:  Vec<String>,
    /// Per-shipment recipient detail for customer notifications.
    #[serde(default)]
    pub details:      Vec<ShipmentMilestoneDetail>,
    pub held_at:      String,
}

/// Emitted by hub-ops when a hub agent approves customs clearance (human gate).
/// Consumed by: order-intake (ShipmentStatus::InTransit), payments (duties
/// invoice if `duties_total_cents` > 0), engagement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerCustomsCleared {
    pub container_id:       Uuid,
    pub tenant_id:          Uuid,
    pub master_awbs:        Vec<String>,
    /// Hub agent who approved clearance (mandatory human gate).
    pub cleared_by:         Uuid,
    #[serde(default)]
    pub duties_total_cents: Option<i64>,
    #[serde(default)]
    pub customs_filing_ref: Option<String>,
    /// 3-char tenant code for duties invoice number generation (e.g. "PH1").
    #[serde(default)]
    pub tenant_code:        String,
    /// Per-shipment detail: recipients (engagement) + payer/duty (payments).
    #[serde(default)]
    pub details:            Vec<ShipmentMilestoneDetail>,
    pub cleared_at:         String,
}

/// Emitted by hub-ops when a domestic road container is released at destination
/// (skips port/customs). Consumed by: analytics, hub-ops internal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerReleasedDomestic {
    pub container_id: Uuid,
    pub tenant_id:    Uuid,
    pub master_awbs:  Vec<String>,
    pub released_at:  String,
}

/// Emitted by hub-ops when a container is broken back into individual pieces at
/// the destination hub. Consumed by: order-intake (ShipmentStatus::AtHub dest).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerDeconsolidated {
    pub container_id:        Uuid,
    pub tenant_id:           Uuid,
    pub destination_hub_id:  Uuid,
    pub master_awbs:         Vec<String>,
    pub child_awbs:          Vec<String>,
    pub deconsolidated_at:   String,
}

/// Emitted by hub-ops when a shipment should be dispatched to an own driver for
/// last-mile (HubRoutingConfig = OwnDriver or Auto). Consumed by: dispatch.
///
/// Enrichment fields (`service_level`, `sla_hours`, `total_cost_cents`) are
/// supplied via the deconsolidate request and forwarded to the carrier booking
/// event on Auto fallback so the carrier SLA record is accurate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubDispatchRequested {
    pub shipment_id:               Uuid,
    pub tenant_id:                 Uuid,
    pub hub_id:                    Uuid,
    pub destination_zone:          String,
    /// Auto-mode fallback window; 0 for OwnDriver (no fallback).
    #[serde(default)]
    pub auto_fallback_window_mins: i32,
    /// "standard" | "next_day" | "same_day" — defaults to "standard" when empty.
    #[serde(default)]
    pub service_level:             String,
    /// SLA window in hours; 0 means use the consumer default.
    #[serde(default)]
    pub sla_hours:                 i64,
    #[serde(default)]
    pub total_cost_cents:          i64,
    pub requested_at:              String,
}

/// Emitted by hub-ops (Carrier routing) or dispatch (Auto fallback) when a
/// shipment should be booked with a 3PL carrier. Consumed by: carrier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubCarrierBookingRequested {
    pub shipment_id:      Uuid,
    pub tenant_id:        Uuid,
    pub hub_id:           Uuid,
    pub destination_zone: String,
    #[serde(default)]
    pub carrier_id:       Option<Uuid>,
    /// "standard" | "next_day" | "same_day" — defaults to "standard" when empty.
    #[serde(default)]
    pub service_level:    String,
    /// SLA window in hours; 0 means use the consumer default.
    #[serde(default)]
    pub sla_hours:        i64,
    #[serde(default)]
    pub total_cost_cents: i64,
    pub requested_at:     String,
}

#[cfg(test)]
mod cross_border_tests {
    use super::*;
    use crate::Event;

    #[test]
    fn piece_scanned_inbound_roundtrips_in_envelope() {
        let sid = Uuid::new_v4();
        let evt = Event::new(
            "logisticos/hub-ops",
            "hub.piece.scanned_inbound",
            Uuid::new_v4(),
            PieceScannedInbound {
                piece_awb: "CM-PH1-B0009012Z-002".into(),
                master_awb: "CM-PH1-B0009012Z".into(),
                shipment_id: sid,
                tenant_id: Uuid::new_v4(),
                hub_id: Uuid::new_v4(),
                scanned_by: Uuid::new_v4(),
                device_timestamp: Some("2026-06-03T08:00:00Z".into()),
                server_timestamp: "2026-06-03T08:00:01Z".into(),
            },
        );
        let json = serde_json::to_string(&evt).unwrap();
        let back: Event<PieceScannedInbound> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.data.shipment_id, sid);
    }

    #[test]
    fn customs_cleared_carries_duties_and_clearer() {
        let by = Uuid::new_v4();
        let p = ContainerCustomsCleared {
            container_id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            master_awbs: vec!["CM-PH1-B0009012Z".into()],
            cleared_by: by,
            duties_total_cents: Some(15_000),
            customs_filing_ref: Some("BRK-123".into()),
            tenant_code: "PH1".into(),
            details: Vec::new(),
            cleared_at: "2026-06-03T09:00:00Z".into(),
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: ContainerCustomsCleared = serde_json::from_str(&json).unwrap();
        assert_eq!(back.cleared_by, by);
        assert_eq!(back.duties_total_cents, Some(15_000));
        assert_eq!(back.master_awbs.len(), 1);
    }

    #[test]
    fn deconsolidated_carries_child_awbs() {
        let p = ContainerDeconsolidated {
            container_id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            destination_hub_id: Uuid::new_v4(),
            master_awbs: vec!["CM-PH1-B0009012Z".into()],
            child_awbs: vec!["CM-PH1-B0009012Z-001".into(), "CM-PH1-B0009012Z-002".into()],
            deconsolidated_at: "2026-06-03T10:00:00Z".into(),
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: ContainerDeconsolidated = serde_json::from_str(&json).unwrap();
        assert_eq!(back.child_awbs.len(), 2);
    }
}
