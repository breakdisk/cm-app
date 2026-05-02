use logisticos_types::{DriverId, RouteId, TenantId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Represents the binding between a driver and a route.
/// A driver can only be assigned to one active route at a time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverAssignment {
    pub id: Uuid,
    pub tenant_id: TenantId,
    pub driver_id: DriverId,
    pub route_id: RouteId,
    /// Populated for single-shipment quick_dispatch flows.
    /// None for multi-stop auto-assign routes (the route owns multiple shipments).
    pub shipment_id: Option<Uuid>,
    pub status: AssignmentStatus,
    pub assigned_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
    pub rejected_at: Option<DateTime<Utc>>,
    pub rejection_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AssignmentStatus {
    Pending,   // Sent to driver, awaiting acceptance
    Accepted,  // Driver accepted; route transitions to InProgress
    Rejected,  // Driver rejected; needs reassignment
    Cancelled, // Dispatcher cancelled before driver responded
}

impl DriverAssignment {
    pub fn new(tenant_id: TenantId, driver_id: DriverId, route_id: RouteId) -> Self {
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            driver_id,
            route_id,
            shipment_id: None,
            status: AssignmentStatus::Pending,
            assigned_at: Utc::now(),
            accepted_at: None,
            rejected_at: None,
            rejection_reason: None,
        }
    }

    /// Driver accepts the assignment — triggers route activation.
    pub fn accept(&mut self) -> Result<(), &'static str> {
        if self.status != AssignmentStatus::Pending {
            return Err("Only pending assignments can be accepted");
        }
        self.status = AssignmentStatus::Accepted;
        self.accepted_at = Some(Utc::now());
        Ok(())
    }

    /// Driver rejects the assignment — dispatcher must find another driver.
    pub fn reject(&mut self, reason: String) -> Result<(), &'static str> {
        if self.status != AssignmentStatus::Pending {
            return Err("Only pending assignments can be rejected");
        }
        self.status = AssignmentStatus::Rejected;
        self.rejected_at = Some(Utc::now());
        self.rejection_reason = Some(reason);
        Ok(())
    }

    pub fn cancel(&mut self) -> Result<(), &'static str> {
        if self.status != AssignmentStatus::Pending {
            return Err("Only pending assignments can be cancelled");
        }
        self.status = AssignmentStatus::Cancelled;
        Ok(())
    }

    pub fn is_active(&self) -> bool {
        self.status == AssignmentStatus::Pending || self.status == AssignmentStatus::Accepted
    }
}
