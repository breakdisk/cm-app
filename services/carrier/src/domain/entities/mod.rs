use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use logisticos_types::TenantId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CarrierId(Uuid);

impl CarrierId {
    pub fn new() -> Self { Self(Uuid::new_v4()) }
    pub fn from_uuid(id: Uuid) -> Self { Self(id) }
    pub fn inner(&self) -> Uuid { self.0 }
}

impl Default for CarrierId {
    fn default() -> Self { Self::new() }
}

/// Delivery performance grade based on SLA compliance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerformanceGrade {
    Excellent,  // ≥ 95%
    Good,       // ≥ 85%
    Fair,       // ≥ 70%
    Poor,       // < 70%
}

impl PerformanceGrade {
    pub fn from_rate(on_time_rate: f64) -> Self {
        if on_time_rate >= 95.0 { Self::Excellent }
        else if on_time_rate >= 85.0 { Self::Good }
        else if on_time_rate >= 70.0 { Self::Fair }
        else { Self::Poor }
    }
}

/// Per-service-type rate card entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateCard {
    pub service_type:       String,    // "same_day" | "next_day" | "standard"
    pub base_rate_cents:    i64,       // flat fee per shipment
    pub per_kg_cents:       i64,       // incremental per kg
    pub max_weight_kg:      f32,
    pub coverage_zones:     Vec<String>, // zone codes covered
}

/// SLA commitment from the carrier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlaCommitment {
    pub on_time_target_pct:    f64,   // e.g. 95.0
    pub max_delivery_days:     u8,
    pub penalty_per_breach:    i64,   // cents deducted per SLA miss
}

impl Default for SlaCommitment {
    fn default() -> Self {
        Self {
            on_time_target_pct: 90.0,
            max_delivery_days: 3,
            penalty_per_breach: 0,
        }
    }
}

/// KYB / document verification state for the carrier.
/// Managed by admin compliance review; independent of operational status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ComplianceStatus {
    /// No documents submitted yet (default for newly onboarded carriers).
    #[default]
    PendingSubmission,
    /// Documents received and under admin review.
    UnderReview,
    /// Fully verified and compliant.
    Compliant,
    /// Compliant but expiring within 30 days.
    ExpiringSoon,
    /// Compliance documents have expired — partner must re-submit.
    Expired,
    /// Submission was rejected; partner must correct and resubmit.
    Rejected,
    /// Compliance suspended (e.g. regulatory hold).
    Suspended,
}


#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CarrierStatus {
    PendingVerification,
    Active,
    Suspended,
    Deactivated,
}

/// A 3PL carrier partner onboarded to the platform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Carrier {
    pub id:               CarrierId,
    pub tenant_id:        TenantId,

    pub name:             String,
    pub code:             String,      // Short code e.g. "JNT", "LBC", "GRAB"
    pub contact_email:    String,
    pub contact_phone:    Option<String>,
    pub api_endpoint:     Option<String>,  // 3PL API base URL (for webhook integrations)
    /// SHA-256 hash of the carrier's API key. Never serialized to API responses.
    /// Frontend reads `has_api_key` (a bool) to know whether a key is configured.
    #[serde(skip_serializing)]
    pub api_key_hash:     Option<String>,
    /// True when `api_key_hash` is set. Computed on load; not persisted separately.
    pub has_api_key:      bool,

    pub status:             CarrierStatus,
    /// KYB / document verification state. Managed by admin; not affected by
    /// operational activate/suspend calls.
    pub compliance_status:  ComplianceStatus,
    pub sla:              SlaCommitment,
    pub rate_cards:       Vec<RateCard>,  // JSONB in DB

    // Running performance metrics
    pub total_shipments:  i64,
    pub on_time_count:    i64,
    pub failed_count:     i64,
    pub performance_grade: PerformanceGrade,

    pub onboarded_at:     DateTime<Utc>,
    pub updated_at:       DateTime<Utc>,
}

impl Carrier {
    pub fn new(
        tenant_id: TenantId,
        name: String,
        code: String,
        contact_email: String,
        sla: SlaCommitment,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: CarrierId::new(),
            tenant_id,
            name,
            code: code.to_uppercase(),
            contact_email,
            contact_phone: None,
            api_endpoint: None,
            api_key_hash: None,
            has_api_key:  false,
            status: CarrierStatus::PendingVerification,
            compliance_status: ComplianceStatus::PendingSubmission,
            sla,
            rate_cards: Vec::new(),
            total_shipments: 0,
            on_time_count: 0,
            failed_count: 0,
            performance_grade: PerformanceGrade::Good,
            onboarded_at: now,
            updated_at: now,
        }
    }

    pub fn activate(&mut self) -> anyhow::Result<()> {
        if self.status == CarrierStatus::Deactivated {
            anyhow::bail!("Cannot reactivate a deactivated carrier");
        }
        self.status = CarrierStatus::Active;
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn suspend(&mut self, _reason: &str) {
        self.status = CarrierStatus::Suspended;
        self.updated_at = Utc::now();
    }

    /// Record a delivery outcome and recompute performance grade.
    pub fn record_delivery(&mut self, on_time: bool) {
        self.total_shipments += 1;
        if on_time { self.on_time_count += 1; } else { self.failed_count += 1; }
        let rate = if self.total_shipments > 0 {
            self.on_time_count as f64 / self.total_shipments as f64 * 100.0
        } else { 0.0 };
        self.performance_grade = PerformanceGrade::from_rate(rate);
        self.updated_at = Utc::now();
    }

    pub fn on_time_rate(&self) -> f64 {
        if self.total_shipments == 0 { return 0.0; }
        self.on_time_count as f64 / self.total_shipments as f64 * 100.0
    }

    /// Find the cheapest rate card for a given service type and weight.
    pub fn quote(&self, service_type: &str, weight_kg: f32) -> Option<i64> {
        self.rate_cards
            .iter()
            .find(|r| r.service_type == service_type && r.max_weight_kg >= weight_kg)
            .map(|r| r.base_rate_cents + (r.per_kg_cents as f32 * weight_kg) as i64)
    }
}

// ── SLA record ────────────────────────────────────────────────────────────────

/// Status of a carrier's SLA commitment for a specific shipment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlaStatus {
    InTransit,
    Delivered,
    Failed,
}

impl SlaStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InTransit => "in_transit",
            Self::Delivered => "delivered",
            Self::Failed    => "failed",
        }
    }
}

/// Per-shipment SLA commitment record.
/// Created by dispatch when a carrier is allocated to a shipment.
/// Updated by the carrier Kafka consumer when delivery.completed/failed fires.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlaRecord {
    pub id:             Uuid,
    pub tenant_id:      Uuid,
    pub carrier_id:     Uuid,
    pub shipment_id:    Uuid,
    /// Destination zone, e.g. "Metro Manila", "Visayas". Used for zone-level SLA reporting.
    pub zone:           String,
    pub service_level:  String,   // "standard" | "next_day" | "same_day"
    /// When the carrier committed to deliver by.
    pub promised_by:    DateTime<Utc>,
    /// Actual delivery time — None while in-transit.
    pub delivered_at:   Option<DateTime<Utc>>,
    pub status:         SlaStatus,
    /// True = delivered on or before promised_by. None = still in transit.
    pub on_time:        Option<bool>,
    pub failure_reason: Option<String>,
    pub created_at:     DateTime<Utc>,
}

impl SlaRecord {
    pub fn new(
        tenant_id: Uuid,
        carrier_id: Uuid,
        shipment_id: Uuid,
        zone: String,
        service_level: String,
        promised_by: DateTime<Utc>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            carrier_id,
            shipment_id,
            zone,
            service_level,
            promised_by,
            delivered_at: None,
            status: SlaStatus::InTransit,
            on_time: None,
            failure_reason: None,
            created_at: now,
        }
    }

    /// Mark this record as delivered, computing on_time from promised_by.
    pub fn mark_delivered(&mut self, delivered_at: DateTime<Utc>) {
        self.delivered_at = Some(delivered_at);
        self.on_time = Some(delivered_at <= self.promised_by);
        self.status = SlaStatus::Delivered;
    }

    /// Mark this record as failed (undeliverable).
    pub fn mark_failed(&mut self, reason: &str) {
        self.status = SlaStatus::Failed;
        self.on_time = Some(false);
        self.failure_reason = Some(reason.to_owned());
    }
}

/// Aggregated SLA row per zone — returned by `GET /v1/carriers/:id/sla-summary`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneSlaRow {
    pub zone:         String,
    pub total:        i64,
    pub on_time:      i64,
    pub failed:       i64,
    /// on_time / total * 100, or 0.0 if total == 0.
    pub on_time_rate: f64,
}

// ── Marketplace entities ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SizeClass {
    Motorcycle,
    Sedan,
    Van,
    L300,
    #[serde(rename = "6wheeler")]
    SixWheeler,
    #[serde(rename = "10wheeler")]
    TenWheeler,
    Trailer,
}

impl SizeClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Motorcycle  => "motorcycle",
            Self::Sedan       => "sedan",
            Self::Van         => "van",
            Self::L300        => "l300",
            Self::SixWheeler  => "6wheeler",
            Self::TenWheeler  => "10wheeler",
            Self::Trailer     => "trailer",
        }
    }

    // Inherent `from_str` is total (unknown input maps to a variant); `FromStr` implies fallible.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> anyhow::Result<Self> {
        match s {
            "motorcycle"  => Ok(Self::Motorcycle),
            "sedan"       => Ok(Self::Sedan),
            "van"         => Ok(Self::Van),
            "l300"        => Ok(Self::L300),
            "6wheeler"    => Ok(Self::SixWheeler),
            "10wheeler"   => Ok(Self::TenWheeler),
            "trailer"     => Ok(Self::Trailer),
            other         => anyhow::bail!("Unknown size class: {other}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListingStatus {
    Active,
    Paused,
    Booked,
    Expired,
}

impl ListingStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active  => "active",
            Self::Paused  => "paused",
            Self::Booked  => "booked",
            Self::Expired => "expired",
        }
    }

    // Inherent `from_str` is total (unknown input maps to a variant); `FromStr` implies fallible.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> anyhow::Result<Self> {
        match s {
            "active"  => Ok(Self::Active),
            "paused"  => Ok(Self::Paused),
            "booked"  => Ok(Self::Booked),
            "expired" => Ok(Self::Expired),
            other     => anyhow::bail!("Unknown listing status: {other}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BookingStatus {
    Pending,
    Accepted,
    Rejected,
    InTransit,
    Delivered,
    Cancelled,
    Disputed,
}

impl BookingStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending   => "pending",
            Self::Accepted  => "accepted",
            Self::Rejected  => "rejected",
            Self::InTransit => "in_transit",
            Self::Delivered => "delivered",
            Self::Cancelled => "cancelled",
            Self::Disputed  => "disputed",
        }
    }

    // Inherent `from_str` is total (unknown input maps to a variant); `FromStr` implies fallible.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> anyhow::Result<Self> {
        match s {
            "pending"    => Ok(Self::Pending),
            "accepted"   => Ok(Self::Accepted),
            "rejected"   => Ok(Self::Rejected),
            "in_transit" => Ok(Self::InTransit),
            "delivered"  => Ok(Self::Delivered),
            "cancelled"  => Ok(Self::Cancelled),
            "disputed"   => Ok(Self::Disputed),
            other        => anyhow::bail!("Unknown booking status: {other}"),
        }
    }
}

/// A carrier's idle vehicle available for spot bookings on the marketplace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VehicleListing {
    pub id:                           Uuid,
    pub tenant_id:                    Uuid,
    pub carrier_id:                   Uuid,
    pub vehicle_plate:                String,
    pub size_class:                   SizeClass,
    pub max_weight_kg:                f32,
    pub max_volume_m3:                Option<f32>,
    pub base_price_cents:             i64,
    pub per_km_cents:                 i64,
    pub per_kg_cents:                 Option<i64>,
    pub service_area_label:           String,
    pub idle_from:                    DateTime<Utc>,
    pub idle_until:                   DateTime<Utc>,
    pub status:                       ListingStatus,
    pub carrier_response_window_mins: i32,
    pub bookings_today:               i64,
    pub revenue_today_cents:          i64,
    pub created_at:                   DateTime<Utc>,
    pub updated_at:                   DateTime<Utc>,
}

impl VehicleListing {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tenant_id:                    Uuid,
        carrier_id:                   Uuid,
        vehicle_plate:                String,
        size_class:                   SizeClass,
        max_weight_kg:                f32,
        max_volume_m3:                Option<f32>,
        base_price_cents:             i64,
        per_km_cents:                 i64,
        per_kg_cents:                 Option<i64>,
        service_area_label:           String,
        idle_from:                    DateTime<Utc>,
        idle_until:                   DateTime<Utc>,
        carrier_response_window_mins: i32,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            carrier_id,
            vehicle_plate,
            size_class,
            max_weight_kg,
            max_volume_m3,
            base_price_cents,
            per_km_cents,
            per_kg_cents,
            service_area_label,
            idle_from,
            idle_until,
            status: ListingStatus::Active,
            carrier_response_window_mins,
            bookings_today: 0,
            revenue_today_cents: 0,
            created_at: now,
            updated_at: now,
        }
    }
}

/// A spot booking placed against a `VehicleListing` by a merchant.
/// Lifecycle: pending → accepted|rejected; accepted → in_transit → delivered.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceBooking {
    pub id:                 Uuid,
    pub tenant_id:          Uuid,
    pub listing_id:         Uuid,
    pub carrier_id:         Uuid,
    pub shipment_id:        Uuid,
    pub awb:                String,
    /// Masked pre-accept; unmasked after carrier accepts.
    pub consumer_name:      String,
    pub consumer_phone:     Option<String>,
    pub pickup_label:       String,
    pub dropoff_label:      String,
    pub cargo_weight_kg:    f32,
    pub cargo_volume_m3:    Option<f32>,
    pub quoted_price_cents: i64,
    pub status:             BookingStatus,
    pub pickup_at:          DateTime<Utc>,
    pub picked_up_at:       Option<DateTime<Utc>>,
    pub picked_up_by:       Option<String>,
    pub pickup_notes:       Option<String>,
    pub created_at:         DateTime<Utc>,
    pub updated_at:         DateTime<Utc>,
}

impl MarketplaceBooking {
    pub fn accept(&mut self) -> anyhow::Result<()> {
        if self.status != BookingStatus::Pending {
            anyhow::bail!("Booking can only be accepted when pending, current status: {}", self.status.as_str());
        }
        self.status = BookingStatus::Accepted;
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn reject(&mut self) -> anyhow::Result<()> {
        if self.status != BookingStatus::Pending {
            anyhow::bail!("Booking can only be rejected when pending, current status: {}", self.status.as_str());
        }
        self.status = BookingStatus::Rejected;
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn record_pickup(&mut self, picked_up_by: Option<String>, notes: Option<String>) -> anyhow::Result<()> {
        if self.status != BookingStatus::Accepted {
            anyhow::bail!("Pickup can only be recorded for accepted bookings, current status: {}", self.status.as_str());
        }
        self.status = BookingStatus::InTransit;
        self.picked_up_at = Some(Utc::now());
        self.picked_up_by = picked_up_by;
        self.pickup_notes = notes;
        self.updated_at = Utc::now();
        Ok(())
    }
}
