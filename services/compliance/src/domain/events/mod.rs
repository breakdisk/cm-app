use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const TOPIC_COMPLIANCE: &str = "compliance";
pub const TOPIC_DRIVER:     &str = "driver";

#[derive(Debug, Serialize, Deserialize)]
pub struct ComplianceStatusChangedPayload {
    pub entity_type:  String,
    pub entity_id:    Uuid,
    pub old_status:   String,
    pub new_status:   String,
    pub is_assignable: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DocumentReviewedPayload {
    pub tenant_id:        Uuid,
    pub entity_id:        Uuid,
    pub document_type:    String,
    pub decision:         String,   // "approved" | "rejected"
    pub rejection_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExpiryWarningPayload {
    pub tenant_id:      Uuid,
    pub entity_id:      Uuid,
    pub document_type:  String,
    pub expiry_date:    String,   // ISO 8601
    pub days_remaining: i32,
}

/// Emitted when a driver is reinstated from suspension.
#[derive(Debug, Serialize, Deserialize)]
pub struct DriverReinstatedPayload {
    pub entity_id:  Uuid,
    pub entity_type: String,
    pub reinstated_by: Uuid,
}

/// Inbound — from driver-ops topic
#[derive(Debug, Serialize, Deserialize)]
pub struct DriverRegisteredPayload {
    pub driver_id:  Uuid,
    pub tenant_id:  Uuid,
    pub jurisdiction: String,
}
