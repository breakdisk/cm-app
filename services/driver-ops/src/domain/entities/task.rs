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
    pub pod_id: Option<uuid::Uuid>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub failed_reason: Option<String>,
    /// Number of delivery attempts that did not result in completion.
    /// Incremented by `attempt()`. Distinct from `fail()` which is permanent.
    pub attempt_count: i32,
    pub last_attempted_at: Option<DateTime<Utc>>,
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

    /// Record a non-completing delivery attempt — driver tried but couldn't
    /// deliver (no one home, address unclear, etc.). Task resets to Pending
    /// for retry while the attempt is logged for engagement notification.
    pub fn attempt(&mut self, reason: String) {
        self.attempt_count += 1;
        self.failed_reason = Some(reason);
        self.last_attempted_at = Some(Utc::now());
        // Reset to Pending so the driver app shows it as retry-eligible.
        self.status = TaskStatus::Pending;
        self.started_at = None;
    }
}
