use chrono::{DateTime, Utc};
use logisticos_types::{DriverId, TenantId, Coordinates};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Driver {
    pub id: DriverId,
    pub tenant_id: TenantId,
    pub user_id: uuid::Uuid,       // Links to identity.users
    pub first_name: String,
    pub last_name: String,
    pub phone: String,
    pub status: DriverStatus,
    pub current_location: Option<Coordinates>,
    pub last_location_at: Option<DateTime<Utc>>,
    pub vehicle_id: Option<uuid::Uuid>,
    pub active_route_id: Option<uuid::Uuid>,
    pub is_active: bool,
    pub driver_type: DriverType,
    pub per_delivery_rate_cents: i32,
    pub cod_commission_rate_bps: i32,   // basis points (250 = 2.50%)
    pub zone: Option<String>,
    pub vehicle_type: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum DriverStatus {
    Offline,
    Available,
    EnRoute,
    Delivering,
    Returning,
    OnBreak,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriverType {
    FullTime,
    PartTime,
}

impl Driver {
    /// Business rule: driver can only be assigned a route when Available.
    pub fn can_accept_route(&self) -> bool {
        self.is_active && self.status == DriverStatus::Available && self.active_route_id.is_none()
    }

    pub fn update_location(&mut self, coords: Coordinates) {
        self.current_location = Some(coords);
        self.last_location_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }

    pub fn go_online(&mut self)  { self.status = DriverStatus::Available; self.updated_at = Utc::now(); }
    pub fn go_offline(&mut self) { self.status = DriverStatus::Offline;   self.updated_at = Utc::now(); }
}
