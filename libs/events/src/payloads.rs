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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipmentCreated {
    pub shipment_id: Uuid,
    pub merchant_id: Uuid,
    pub customer_id: Uuid,
    pub origin_address: String,
    pub destination_address: String,
    pub service_type: String,
    pub cod_amount: Option<i64>,
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
