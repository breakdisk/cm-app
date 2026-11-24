use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;
use crate::domain::entities::{ProofOfDelivery, OtpCode, ProofOfPickup};

#[async_trait]
pub trait PodRepository: Send + Sync {
    /// Always tenant-scoped: another tenant's id reads as absent.
    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<Option<ProofOfDelivery>>;
    async fn find_by_shipment(&self, tenant_id: Uuid, shipment_id: Uuid) -> anyhow::Result<Option<ProofOfDelivery>>;
    async fn save(&self, pod: &ProofOfDelivery) -> anyhow::Result<()>;
}

#[async_trait]
pub trait OtpRepository: Send + Sync {
    async fn find_active_by_shipment(&self, shipment_id: Uuid, tenant_id: Uuid) -> anyhow::Result<Option<OtpCode>>;
    async fn save(&self, otp: &OtpCode) -> anyhow::Result<()>;
    /// Atomically spends one attempt against the PIN before it is compared.
    /// Returns the attempts used including this one, or `None` when the PIN is locked.
    async fn claim_attempt(&self, id: Uuid, max_attempts: i32) -> anyhow::Result<Option<i32>>;
    /// Hands back the attempt claimed by a correct PIN, so a legitimate check never counts.
    async fn release_attempt(&self, id: Uuid) -> anyhow::Result<()>;
}

// ── Proof of Pickup ────────────────────────────────────────────────────────────

#[async_trait]
pub trait PickupRepository: Send + Sync {
    /// Always tenant-scoped: another tenant's id reads as absent.
    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<Option<ProofOfPickup>>;
    /// Returns the most recent POP (any status) for a shipment, within the tenant.
    async fn find_by_shipment(&self, tenant_id: Uuid, shipment_id: Uuid) -> anyhow::Result<Option<ProofOfPickup>>;
    /// Returns the completed (submitted) POP for a shipment, or None if pickup has
    /// not yet been captured. Used by the carrier/container assignment guard to block
    /// sea-container loading until the chain of custody is open.
    /// Tenant-bound: another tenant's POP never satisfies this tenant's gate.
    async fn find_completed_by_shipment(&self, tenant_id: Uuid, shipment_id: Uuid) -> anyhow::Result<Option<ProofOfPickup>>;
    /// Upsert — idempotent; uses `ON CONFLICT (id) DO UPDATE` for draft → submitted transitions.
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
        tenant_id:   Uuid,
        shipment_id: Uuid,
        limit:       u32,
    ) -> anyhow::Result<Vec<TelemetryEntry>>;
}

// ── Shipment billing context (driven port → order-intake) ──────────────────────

/// Booking-time billing classification for a shipment, owned by order-intake.
///
/// POP reads this at `initiate_pickup` so the billing track and the debited
/// amount come from the booking record rather than from the driver's handset.
#[derive(Debug, Clone)]
pub struct ShipmentBillingContext {
    /// `ServiceType::as_str()` — "standard" | "express" | "same_day" |
    /// "balikbayan" | "international". Payments gates the Track A driver-ledger
    /// debit on exactly "balikbayan".
    pub service_code:         String,
    /// Declared value in cents; `None` when the merchant booked without one.
    pub declared_value_cents: Option<i64>,
    /// The recipient's phone, from the booking. The delivery PIN is sent here and
    /// nowhere else -- a caller-supplied phone is what let a driver mint their
    /// own PIN. `None` only against an order-intake predating the field.
    pub customer_phone:       Option<String>,
}

/// order-intake said, explicitly, that the shipment is not this tenant's: it
/// does not exist, or it belongs to another tenant (the two are deliberately
/// indistinguishable). Carried inside the `anyhow::Error` of a failed lookup so
/// callers can tell it from an outage, which must never block capture.
#[derive(Debug)]
pub struct ShipmentNotFound(pub Uuid);

impl std::fmt::Display for ShipmentNotFound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "shipment {} not found", self.0)
    }
}

impl std::error::Error for ShipmentNotFound {}

/// Driven port — resolves a shipment's billing classification from order-intake,
/// for the caller's tenant. Implemented in `infrastructure/external/order_intake.rs`.
#[async_trait]
pub trait ShipmentBillingContextSource: Send + Sync {
    /// A shipment that is missing, or belongs to a tenant other than `tenant_id`,
    /// fails with [`ShipmentNotFound`]; every other failure is an outage.
    async fn fetch(&self, tenant_id: Uuid, shipment_id: Uuid) -> anyhow::Result<ShipmentBillingContext>;
}
