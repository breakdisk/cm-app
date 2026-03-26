use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceAuditLog {
    pub id:                    Uuid,
    pub tenant_id:             Uuid,
    pub compliance_profile_id: Uuid,
    pub document_id:           Option<Uuid>,
    pub event_type:            String,
    pub actor_id:              Uuid,
    pub actor_type:            String,
    pub notes:                 Option<String>,
    pub created_at:            DateTime<Utc>,
}
