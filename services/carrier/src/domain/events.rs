use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Domain events emitted by the Carrier aggregate. These are distinct from
/// the Kafka payloads in `infrastructure::messaging` — they represent state
/// transitions within the domain model and are the source of truth from which
/// Kafka events are derived.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CarrierDomainEvent {
    /// A new carrier partner was onboarded to the tenant.
    CarrierOnboarded {
        carrier_id:    Uuid,
        tenant_id:     Uuid,
        name:          String,
        code:          String,
        contact_email: String,
        occurred_at:   DateTime<Utc>,
    },

    /// Admin approved the carrier — status transitions to Active.
    CarrierActivated {
        carrier_id:  Uuid,
        tenant_id:   Uuid,
        occurred_at: DateTime<Utc>,
    },

    /// Carrier was suspended by admin (e.g. SLA breach, compliance failure).
    CarrierSuspended {
        carrier_id:  Uuid,
        tenant_id:   Uuid,
        reason:      String,
        occurred_at: DateTime<Utc>,
    },

    /// A new (or rotated) integration API key was generated.
    /// The plaintext key is never carried in domain events — only the
    /// fact that the hash was updated.
    ApiKeyRotated {
        carrier_id:  Uuid,
        tenant_id:   Uuid,
        occurred_at: DateTime<Utc>,
    },

    /// A per-shipment SLA commitment record was created by dispatch.
    SlaRecordCreated {
        carrier_id:       Uuid,
        tenant_id:        Uuid,
        shipment_id:      Uuid,
        zone:             String,
        service_level:    String,
        promised_by:      DateTime<Utc>,
        total_cost_cents: i64,
        occurred_at:      DateTime<Utc>,
    },

    /// A shipment was delivered; on_time flag updates the carrier's performance grade.
    DeliveryOutcomeRecorded {
        carrier_id:   Uuid,
        shipment_id:  Uuid,
        on_time:      bool,
        /// `None` for failures; `Some(ts)` for successful deliveries.
        delivered_at: Option<DateTime<Utc>>,
        occurred_at:  DateTime<Utc>,
    },

    /// Carrier profile fields were updated (name, contact, SLA commitment, rate cards).
    CarrierProfileUpdated {
        carrier_id:  Uuid,
        tenant_id:   Uuid,
        /// JSON-serialised diff of changed top-level fields (name, contact_email, etc.)
        /// Used for audit logging; not interpreted by downstream services.
        changed_fields: Vec<String>,
        occurred_at: DateTime<Utc>,
    },

    /// Admin updated the KYB/compliance status of a carrier.
    ComplianceStatusChanged {
        carrier_id:     Uuid,
        tenant_id:      Uuid,
        old_status:     String,
        new_status:     String,
        occurred_at:    DateTime<Utc>,
    },
}

impl CarrierDomainEvent {
    pub fn carrier_id(&self) -> Uuid {
        match self {
            Self::CarrierOnboarded    { carrier_id, .. } => *carrier_id,
            Self::CarrierActivated    { carrier_id, .. } => *carrier_id,
            Self::CarrierSuspended    { carrier_id, .. } => *carrier_id,
            Self::ApiKeyRotated       { carrier_id, .. } => *carrier_id,
            Self::SlaRecordCreated    { carrier_id, .. } => *carrier_id,
            Self::DeliveryOutcomeRecorded { carrier_id, .. } => *carrier_id,
            Self::CarrierProfileUpdated   { carrier_id, .. } => *carrier_id,
            Self::ComplianceStatusChanged { carrier_id, .. } => *carrier_id,
        }
    }

    pub fn occurred_at(&self) -> DateTime<Utc> {
        match self {
            Self::CarrierOnboarded    { occurred_at, .. } => *occurred_at,
            Self::CarrierActivated    { occurred_at, .. } => *occurred_at,
            Self::CarrierSuspended    { occurred_at, .. } => *occurred_at,
            Self::ApiKeyRotated       { occurred_at, .. } => *occurred_at,
            Self::SlaRecordCreated    { occurred_at, .. } => *occurred_at,
            Self::DeliveryOutcomeRecorded { occurred_at, .. } => *occurred_at,
            Self::CarrierProfileUpdated   { occurred_at, .. } => *occurred_at,
            Self::ComplianceStatusChanged { occurred_at, .. } => *occurred_at,
        }
    }

    pub fn event_type(&self) -> &'static str {
        match self {
            Self::CarrierOnboarded          { .. } => "carrier_onboarded",
            Self::CarrierActivated          { .. } => "carrier_activated",
            Self::CarrierSuspended          { .. } => "carrier_suspended",
            Self::ApiKeyRotated             { .. } => "api_key_rotated",
            Self::SlaRecordCreated          { .. } => "sla_record_created",
            Self::DeliveryOutcomeRecorded   { .. } => "delivery_outcome_recorded",
            Self::CarrierProfileUpdated     { .. } => "carrier_profile_updated",
            Self::ComplianceStatusChanged   { .. } => "compliance_status_changed",
        }
    }
}
