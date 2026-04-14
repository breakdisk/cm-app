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
    pub api_key_hash:     Option<String>,  // SHA-256 of API key (never stored plaintext)

    pub status:           CarrierStatus,
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
            status: CarrierStatus::PendingVerification,
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
