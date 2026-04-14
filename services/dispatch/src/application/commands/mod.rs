use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
pub struct CreateRouteCommand {
    pub driver_id: Uuid,
    pub vehicle_id: Uuid,
    pub shipment_ids: Vec<Uuid>,    // Order-intake creates shipments; dispatch creates the route
}

#[derive(Debug, Deserialize)]
pub struct AutoAssignDriverCommand {
    pub route_id: Uuid,             // Route to assign
    pub preferred_driver_id: Option<Uuid>,  // Optional explicit preference from dispatcher
}

#[derive(Debug, Deserialize)]
pub struct AcceptAssignmentCommand {
    pub assignment_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct RejectAssignmentCommand {
    pub assignment_id: Uuid,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct RouteView {
    pub route_id: Uuid,
    pub driver_id: Uuid,
    pub vehicle_id: Uuid,
    pub status: String,
    pub stop_count: usize,
    pub total_distance_km: f64,
    pub estimated_duration_minutes: u32,
    pub created_at: String,
}

#[derive(Debug)]
pub struct QuickDispatchCommand {
    pub shipment_id:         uuid::Uuid,
    pub preferred_driver_id: Option<uuid::Uuid>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct AddStopCommand {
    pub route_id: Uuid,
    pub shipment_id: Uuid,
    #[validate(length(min = 1))]
    pub address_line1: String,
    pub city: String,
    pub lat: f64,
    pub lng: f64,
}
