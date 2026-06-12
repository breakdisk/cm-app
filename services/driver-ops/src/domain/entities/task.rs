use chrono::{DateTime, Utc};
use logisticos_types::{DriverId, Coordinates};
use serde::{Deserialize, Serialize};

/// A single delivery or pickup task assigned to a driver.
/// Tasks are the unit of work in the driver app.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverTask {
    pub id: uuid::Uuid,
    pub driver_id: DriverId,
    pub route_id: uuid::Uuid,
    pub shipment_id: uuid::Uuid,
    pub task_type: TaskType,
    pub sequence: i32,
    pub status: TaskStatus,
    pub address: logisticos_types::Address,
    pub customer_name: String,
    pub customer_phone: String,
    pub customer_email: Option<String>,
    /// Customer UUID — populated when TaskAssigned carries it (requires
    /// dispatch to forward customer_id). Used to populate TaskCompleted so
    /// engagement can link the notification to the customer profile.
    pub customer_id: Option<uuid::Uuid>,
    pub tracking_number: Option<String>,
    pub cod_amount_cents: Option<i64>,
    pub special_instructions: Option<String>,
    /// Merchant / sender display name — shown on the driver task card.
    pub merchant_name: String,
    /// "food" | "parcel" | "grocery" | "medicine" | "heavy" | "large" — drives
    /// the task-card icon in the driver app.
    pub delivery_category: String,
    /// Declared shipment weight in grams (0 = unknown).
    pub weight_grams: i64,
    /// Full route ends (pickup AND delivery) regardless of this task's leg,
    /// so the app can render the route sketch on every card.
    pub pickup_lat: Option<f64>,
    pub pickup_lng: Option<f64>,
    pub delivery_lat: Option<f64>,
    pub delivery_lng: Option<f64>,
    pub pod_id: Option<uuid::Uuid>,         // Filled when delivery task completed
    pub pop_id: Option<uuid::Uuid>,         // Filled when pickup task completed
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub failed_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TaskType {
    Pickup,
    Delivery,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Skipped,
}

impl DriverTask {
    /// Returns `true` when the task type requires a Proof of Delivery.
    /// Pickup tasks require a Proof of Pickup (pop_id) instead.
    pub fn requires_pod(&self) -> bool {
        self.task_type == TaskType::Delivery
    }

    /// Returns `true` when the task type requires a Proof of Pickup.
    pub fn requires_pop(&self) -> bool {
        self.task_type == TaskType::Pickup
    }

    pub fn start(&mut self) {
        self.status = TaskStatus::InProgress;
        self.started_at = Some(Utc::now());
    }

    pub fn complete(&mut self, pod_id: Option<uuid::Uuid>, pop_id: Option<uuid::Uuid>) {
        self.status = TaskStatus::Completed;
        self.pod_id = pod_id;
        self.pop_id = pop_id;
        self.completed_at = Some(Utc::now());
    }

    pub fn fail(&mut self, reason: String) {
        self.status = TaskStatus::Failed;
        self.failed_reason = Some(reason);
        self.completed_at = Some(Utc::now());
    }
}
