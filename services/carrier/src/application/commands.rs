use chrono::{DateTime, Utc};
use uuid::Uuid;

// OnboardCarrierCommand and UpdateCarrierCommand live in application::services
// (where the HTTP handler already imports them from). New commands that don't
// exist there are defined here and re-exported from services where needed.

// ── Carrier lifecycle ─────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct ActivateCarrierCommand {
    pub carrier_id: Uuid,
}

#[derive(Debug)]
pub struct SuspendCarrierCommand {
    pub carrier_id: Uuid,
    pub reason:     String,
}

// ── API key ───────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct GenerateApiKeyCommand {
    pub carrier_id: Uuid,
}

// ── SLA record ────────────────────────────────────────────────────────────────

/// Emitted by the dispatch service when it allocates a carrier to a shipment.
/// Consumed by `POST /v1/internal/sla-records`.
#[derive(Debug)]
pub struct CreateSlaRecordCommand {
    pub tenant_id:        Uuid,
    pub carrier_id:       Uuid,
    pub shipment_id:      Uuid,
    pub zone:             String,
    pub service_level:    String,
    pub promised_by:      DateTime<Utc>,
    pub total_cost_cents: i64,
    pub method:           String,
}

// ── Delivery outcome ──────────────────────────────────────────────────────────

/// Emitted by the Kafka consumer when a `delivery.completed` or
/// `delivery.failed` event arrives from the driver-ops service.
#[derive(Debug)]
pub struct RecordDeliveryOutcomeCommand {
    pub shipment_id:    Uuid,
    pub on_time:        bool,
    pub delivered_at:   Option<DateTime<Utc>>,
    pub failure_reason: Option<String>,
}
