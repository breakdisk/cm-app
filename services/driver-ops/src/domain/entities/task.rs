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
    pub tracking_number: Option<String>,
    pub cod_amount_cents: Option<i64>,
    pub special_instructions: Option<String>,
    pub pod_id: Option<uuid::Uuid>,         // Filled when task completed
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
    /// Business rule: task cannot be completed without a POD record
    /// (for delivery tasks).
    pub fn can_complete_without_pod(&self) -> bool {
        self.task_type == TaskType::Pickup
    }

    pub fn start(&mut self) {
        self.status = TaskStatus::InProgress;
        self.started_at = Some(Utc::now());
    }

    pub fn complete(&mut self, pod_id: Option<uuid::Uuid>) -> Result<(), &'static str> {
        if self.task_type == TaskType::Delivery && pod_id.is_none() {
            return Err("Delivery task requires proof of delivery");
        }
        self.status = TaskStatus::Completed;
        self.pod_id = pod_id;
        self.completed_at = Some(Utc::now());
        Ok(())
    }

    pub fn fail(&mut self, reason: String) {
        self.status = TaskStatus::Failed;
        self.failed_reason = Some(reason);
        self.completed_at = Some(Utc::now());
    }
}
