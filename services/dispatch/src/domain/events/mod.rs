use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteCreated {
    pub route_id: Uuid,
    pub tenant_id: Uuid,
    pub driver_id: Uuid,
    pub stop_count: u32,
    pub total_distance_km: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverAssigned {
    pub assignment_id: Uuid,
    pub shipment_id:   Uuid,
    pub customer_id:   Uuid,  // Populated from order-intake shipment lookup
    pub route_id: Uuid,
    pub driver_id: Uuid,
    pub tenant_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteStarted {
    pub route_id: Uuid,
    pub driver_id: Uuid,
    pub tenant_id: Uuid,
}
