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
    pub assignment_id:         Uuid,
    pub shipment_id:           Uuid,
    pub customer_id:           Uuid,  // Populated from order-intake shipment lookup
    pub route_id:              Uuid,
    pub driver_id:             Uuid,
    pub tenant_id:             Uuid,
    // Customer contact — denormalized so engagement can send "pickup_scheduled"
    // WhatsApp/push without querying another service.
    pub customer_name:         String,
    pub customer_phone:        String,
    pub customer_email:        String,
    pub tracking_number:       String,
    pub estimated_pickup_time: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteStarted {
    pub route_id: Uuid,
    pub driver_id: Uuid,
    pub tenant_id: Uuid,
}
