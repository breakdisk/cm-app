pub mod pallet;
pub mod container;

/// Hub Operations — sorting hub / cross-dock facility management.
///
/// A hub is a physical facility where packages are received from drivers,
/// sorted by delivery zone, and dispatched on outbound routes.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use logisticos_types::TenantId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HubId(Uuid);
impl HubId {
    pub fn new() -> Self { Self(Uuid::new_v4()) }
    pub fn from_uuid(id: Uuid) -> Self { Self(id) }
    pub fn inner(&self) -> Uuid { self.0 }
}
impl Default for HubId { fn default() -> Self { Self::new() } }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InductionId(Uuid);
impl InductionId {
    pub fn new() -> Self { Self(Uuid::new_v4()) }
    pub fn from_uuid(id: Uuid) -> Self { Self(id) }
    pub fn inner(&self) -> Uuid { self.0 }
}

/// Physical sorting hub.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hub {
    pub id:                HubId,
    pub tenant_id:         TenantId,
    pub name:              String,
    pub address:           String,
    pub lat:               f64,
    pub lng:               f64,
    pub capacity:          u32,   // Max parcels per day
    pub current_load:      u32,   // Parcels currently in hub
    pub serving_zones:     Vec<String>,  // Zone codes this hub handles
    pub is_active:         bool,
    pub created_at:        DateTime<Utc>,
    pub updated_at:        DateTime<Utc>,
}

impl Hub {
    pub fn new(tenant_id: TenantId, name: String, address: String, lat: f64, lng: f64, capacity: u32) -> Self {
        let now = Utc::now();
        Self {
            id: HubId::new(),
            tenant_id,
            name,
            address,
            lat,
            lng,
            capacity,
            current_load: 0,
            serving_zones: Vec::new(),
            is_active: true,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn capacity_pct(&self) -> f32 {
        if self.capacity == 0 { return 0.0; }
        self.current_load as f32 / self.capacity as f32 * 100.0
    }

    pub fn is_over_capacity(&self) -> bool {
        self.current_load >= self.capacity
    }

    pub fn induct_parcel(&mut self) -> anyhow::Result<()> {
        if self.is_over_capacity() {
            anyhow::bail!("Hub '{}' is at capacity ({}/{})", self.name, self.current_load, self.capacity);
        }
        self.current_load += 1;
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn dispatch_parcel(&mut self) {
        if self.current_load > 0 {
            self.current_load -= 1;
            self.updated_at = Utc::now();
        }
    }
}

/// Status of a parcel at the hub.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InductionStatus {
    Inducted,      // Received at hub, not yet sorted
    Sorted,        // Assigned to outbound route/zone
    Dispatched,    // Left the hub on an outbound route
    Returned,      // Customer return — awaiting merchant pickup
}

/// A parcel induction record — tracks a shipment's time in the hub.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParcelInduction {
    pub id:           InductionId,
    pub hub_id:       HubId,
    pub tenant_id:    TenantId,
    pub shipment_id:  Uuid,
    pub tracking_number: String,
    pub status:       InductionStatus,
    pub zone:         Option<String>,      // Assigned sort zone
    pub bay:          Option<String>,      // Physical bay/slot in hub
    pub inducted_by:  Option<Uuid>,        // driver_id or staff_id
    pub inducted_at:  DateTime<Utc>,
    pub sorted_at:    Option<DateTime<Utc>>,
    pub dispatched_at: Option<DateTime<Utc>>,
}

impl ParcelInduction {
    pub fn new(hub_id: HubId, tenant_id: TenantId, shipment_id: Uuid, tracking_number: String, inducted_by: Option<Uuid>) -> Self {
        Self {
            id: InductionId::new(),
            hub_id,
            tenant_id,
            shipment_id,
            tracking_number,
            status: InductionStatus::Inducted,
            zone: None,
            bay: None,
            inducted_by,
            inducted_at: Utc::now(),
            sorted_at: None,
            dispatched_at: None,
        }
    }

    pub fn sort_to(&mut self, zone: String, bay: String) {
        self.zone = Some(zone);
        self.bay = Some(bay);
        self.status = InductionStatus::Sorted;
        self.sorted_at = Some(Utc::now());
    }

    pub fn dispatch(&mut self) {
        self.status = InductionStatus::Dispatched;
        self.dispatched_at = Some(Utc::now());
    }

    /// Time in hub so far (in minutes).
    pub fn dwell_minutes(&self) -> i64 {
        let end = self.dispatched_at.unwrap_or_else(Utc::now);
        (end - self.inducted_at).num_minutes()
    }
}
