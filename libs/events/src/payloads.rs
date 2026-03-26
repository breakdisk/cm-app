use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantCreated {
    pub tenant_id: Uuid,
    pub name: String,
    pub slug: String,
    pub owner_email: String,
    pub owner_user_id: Uuid,
    pub subscription_tier: String,
}

// Enriched ShipmentCreated — add customer details for dispatch_queue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipmentCreated {
    pub shipment_id:          Uuid,
    pub merchant_id:          Uuid,
    pub customer_id:          Uuid,
    pub customer_name:        String,
    pub customer_phone:       String,
    pub origin_address:       String,
    pub destination_address:  String,
    pub destination_city:     String,
    pub destination_lat:      Option<f64>,
    pub destination_lng:      Option<f64>,
    pub service_type:         String,
    pub cod_amount_cents:     Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverAssigned {
    pub shipment_id: Uuid,
    pub driver_id: Uuid,
    pub route_id: Uuid,
    pub estimated_pickup_time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryCompleted {
    pub shipment_id: Uuid,
    pub driver_id: Uuid,
    pub pod_id: Uuid,
    pub delivered_at: String,
    pub recipient_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryFailed {
    pub shipment_id: Uuid,
    pub driver_id: Uuid,
    pub reason: String,
    pub attempted_at: String,
    pub attempt_number: u32,
    pub next_attempt_scheduled: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationUpdated {
    pub driver_id: Uuid,
    pub lat: f64,
    pub lng: f64,
    pub timestamp: String,
    pub speed_kmh: Option<f32>,
    pub heading: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PodCaptured {
    pub pod_id: Uuid,
    pub shipment_id: Uuid,
    pub driver_id: Uuid,
    pub pod_type: String,       // "signature" | "photo" | "otp"
    pub captured_at: String,
    pub lat: Option<f64>,
    pub lng: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodCollected {
    pub shipment_id: Uuid,
    pub driver_id: Uuid,
    pub amount_cents: i64,
    pub currency: String,
    pub collected_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceGenerated {
    pub invoice_id: Uuid,
    pub merchant_id: Uuid,
    pub total_cents: i64,
    pub due_date: String,
    pub period_from: String,
    pub period_to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserCreated {
    pub user_id:   Uuid,
    pub tenant_id: Uuid,
    pub email:     String,
    pub roles:     Vec<String>,
}

/// Emitted by dispatch when a shipment is assigned to a driver.
/// Contains all data driver-ops needs to create a DriverTask row
/// without querying other services.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAssigned {
    pub task_id:              Uuid,   // Pre-generated UUID for the task
    pub assignment_id:        Uuid,
    pub shipment_id:          Uuid,
    pub route_id:             Uuid,
    pub driver_id:            Uuid,
    pub tenant_id:            Uuid,
    pub sequence:             u32,
    // Destination (denormalized from dispatch_queue for offline driver app)
    pub address_line1:        String,
    pub address_city:         String,
    pub address_province:     String,
    pub address_postal_code:  String,
    pub address_lat:          Option<f64>,
    pub address_lng:          Option<f64>,
    // Customer (denormalized for driver app display)
    pub customer_name:        String,
    pub customer_phone:       String,
    pub cod_amount_cents:     Option<i64>,
    pub special_instructions: Option<String>,
}
