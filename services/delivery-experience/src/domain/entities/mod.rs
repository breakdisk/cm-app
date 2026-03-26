/// Shipment tracking read model.
///
/// This is a projection / materialised view of shipment state.
/// The authoritative source is order-intake; this service owns the
/// customer-facing presentation layer and event timeline.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use logisticos_types::TenantId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackingStatus {
    Pending,
    Confirmed,
    AssignedToDriver,
    OutForPickup,
    PickedUp,
    InTransit,
    OutForDelivery,
    DeliveryAttempted,
    Delivered,
    DeliveryFailed,
    Cancelled,
    ReturnInitiated,
    Returned,
}

impl TrackingStatus {
    pub fn display_label(&self) -> &'static str {
        match self {
            Self::Pending           => "Order Placed",
            Self::Confirmed         => "Order Confirmed",
            Self::AssignedToDriver  => "Driver Assigned",
            Self::OutForPickup      => "Driver On The Way",
            Self::PickedUp          => "Package Picked Up",
            Self::InTransit         => "In Transit",
            Self::OutForDelivery    => "Out for Delivery",
            Self::DeliveryAttempted => "Delivery Attempted",
            Self::Delivered         => "Delivered",
            Self::DeliveryFailed    => "Delivery Failed",
            Self::Cancelled         => "Cancelled",
            Self::ReturnInitiated   => "Return Initiated",
            Self::Returned          => "Returned",
        }
    }

    /// True if this is a terminal status — no further updates expected.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Delivered | Self::Cancelled | Self::Returned)
    }
}

/// Immutable point-in-time entry on the tracking timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusEvent {
    pub status:      TrackingStatus,
    pub description: String,
    pub occurred_at: DateTime<Utc>,
    /// Optional driver or hub location at the time of the event.
    pub location:    Option<String>,
}

/// Live driver position — updated via LocationUpdated Kafka events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverPosition {
    pub lat:          f64,
    pub lng:          f64,
    pub updated_at:   DateTime<Utc>,
}

/// The core read model: everything a customer or merchant needs to display
/// a rich tracking page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackingRecord {
    pub shipment_id:          Uuid,
    pub tenant_id:            TenantId,
    pub tracking_number:      String,

    pub current_status:       TrackingStatus,
    pub status_history:       Vec<StatusEvent>,   // chronological, JSONB in DB

    // Addresses (display only)
    pub origin_address:       String,
    pub destination_address:  String,

    // Driver info (populated once assigned)
    pub driver_id:            Option<Uuid>,
    pub driver_name:          Option<String>,
    pub driver_phone:         Option<String>,
    pub driver_position:      Option<DriverPosition>,

    // Timing
    pub estimated_delivery:   Option<DateTime<Utc>>,
    pub delivered_at:         Option<DateTime<Utc>>,

    // POD
    pub pod_id:               Option<Uuid>,
    pub recipient_name:       Option<String>,

    // Attempt tracking
    pub attempt_number:       u8,
    pub next_attempt_at:      Option<DateTime<Utc>>,

    pub created_at:           DateTime<Utc>,
    pub updated_at:           DateTime<Utc>,
}

impl TrackingRecord {
    pub fn new(
        shipment_id: Uuid,
        tenant_id: TenantId,
        tracking_number: String,
        origin_address: String,
        destination_address: String,
    ) -> Self {
        let now = Utc::now();
        let initial_event = StatusEvent {
            status:      TrackingStatus::Pending,
            description: "Order placed".into(),
            occurred_at: now,
            location:    None,
        };
        Self {
            shipment_id,
            tenant_id,
            tracking_number,
            current_status:      TrackingStatus::Pending,
            status_history:      vec![initial_event],
            origin_address,
            destination_address,
            driver_id:           None,
            driver_name:         None,
            driver_phone:        None,
            driver_position:     None,
            estimated_delivery:  None,
            delivered_at:        None,
            pod_id:              None,
            recipient_name:      None,
            attempt_number:      0,
            next_attempt_at:     None,
            created_at:          now,
            updated_at:          now,
        }
    }

    /// Transition to a new status, appending to the history.
    pub fn transition(&mut self, status: TrackingStatus, description: String, location: Option<String>) {
        // Don't append duplicate adjacent status.
        if self.current_status == status {
            return;
        }
        // Don't allow backward transitions from terminal state.
        if self.current_status.is_terminal() {
            return;
        }
        self.status_history.push(StatusEvent {
            status:      status.clone(),
            description,
            occurred_at: Utc::now(),
            location,
        });
        self.current_status = status;
        self.updated_at = Utc::now();
    }

    pub fn assign_driver(
        &mut self,
        driver_id: Uuid,
        driver_name: String,
        driver_phone: String,
        estimated_delivery: Option<DateTime<Utc>>,
    ) {
        self.driver_id = Some(driver_id);
        self.driver_name = Some(driver_name);
        self.driver_phone = Some(driver_phone);
        self.estimated_delivery = estimated_delivery;
        self.transition(
            TrackingStatus::AssignedToDriver,
            "Driver assigned to your shipment".into(),
            None,
        );
    }

    pub fn mark_delivered(&mut self, pod_id: Uuid, recipient_name: String, delivered_at: DateTime<Utc>) {
        self.pod_id = Some(pod_id);
        self.recipient_name = Some(recipient_name);
        self.delivered_at = Some(delivered_at);
        self.transition(
            TrackingStatus::Delivered,
            format!("Package delivered. Received by: {}", recipient_name),
            None,
        );
    }

    pub fn mark_failed(
        &mut self,
        reason: String,
        attempt_number: u32,
        next_attempt_at: Option<DateTime<Utc>>,
    ) {
        self.attempt_number = attempt_number as u8;
        self.next_attempt_at = next_attempt_at;
        self.transition(
            TrackingStatus::DeliveryFailed,
            format!("Delivery attempt #{} failed: {}", attempt_number, reason),
            None,
        );
    }

    pub fn update_driver_position(&mut self, lat: f64, lng: f64) {
        self.driver_position = Some(DriverPosition { lat, lng, updated_at: Utc::now() });
        self.updated_at = Utc::now();
    }
}
