use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;
use crate::domain::entities::{ProofOfDelivery, OtpCode, ProofOfPickup};

#[async_trait]
pub trait PodRepository: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<ProofOfDelivery>>;
    async fn find_by_shipment(&self, shipment_id: Uuid) -> anyhow::Result<Option<ProofOfDelivery>>;
    async fn save(&self, pod: &ProofOfDelivery) -> anyhow::Result<()>;
}

#[async_trait]
pub trait OtpRepository: Send + Sync {
    async fn find_active_by_shipment(&self, shipment_id: Uuid, tenant_id: Uuid) -> anyhow::Result<Option<OtpCode>>;
    async fn save(&self, otp: &OtpCode) -> anyhow::Result<()>;
}

// ── Proof of Pickup ────────────────────────────────────────────────────────────

#[async_trait]
pub trait PickupRepository: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<ProofOfPickup>>;
    /// Returns the active (non-submitted) POP for a shipment, if any.
    async fn find_by_shipment(&self, shipment_id: Uuid) -> anyhow::Result<Option<ProofOfPickup>>;
    /// Upsert — idempotent; uses `ON CONFLICT (shipment_id) WHERE status = 'submitted'`
    /// to ensure only one submitted POP per shipment.
    async fn save(&self, pop: &ProofOfPickup) -> anyhow::Result<()>;
}

// ── Shipment Telemetry ─────────────────────────────────────────────────────────

/// A single immutable telemetry entry in the append-only `shipment_telemetry_logs` table.
/// NEVER update rows — insert new rows for corrections (event_type = "correction").
#[derive(Debug, Clone)]
pub struct TelemetryEntry {
    pub tenant_id:        Uuid,
    pub shipment_id:      Uuid,
    pub task_id:          Option<Uuid>,
    pub driver_id:        Option<Uuid>,
    /// Event classification, e.g. "pickup_scan", "delivery_attempt", "pod_submitted",
    /// "pop_submitted", "out_of_bounds_handover", "correction".
    pub event_type:       String,
    /// Device clock timestamp — canonical event time for SLA calculations.
    pub device_timestamp: DateTime<Utc>,
    /// Backend receipt time.
    pub server_timestamp: DateTime<Utc>,
    pub lat:              Option<f64>,
    pub lng:              Option<f64>,
    /// Freeform JSON payload for event-specific metadata.
    pub metadata:         serde_json::Value,
}

#[async_trait]
pub trait TelemetryRepository: Send + Sync {
    /// Append a single telemetry entry. Must never fail silently — caller decides
    /// whether to propagate the error or swallow it (fire-and-forget paths use `warn!`).
    async fn append(&self, entry: TelemetryEntry) -> anyhow::Result<()>;

    /// Query recent telemetry for a shipment, ordered by device_timestamp DESC.
    async fn list_for_shipment(
        &self,
        shipment_id: Uuid,
        limit:       u32,
    ) -> anyhow::Result<Vec<TelemetryEntry>>;
}
