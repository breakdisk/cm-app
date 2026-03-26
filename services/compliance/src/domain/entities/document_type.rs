use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentType {
    pub id:                Uuid,
    pub code:              String,
    pub jurisdiction:      String,
    pub applicable_to:     Vec<String>,
    pub name:              String,
    pub description:       Option<String>,
    pub is_required:       bool,
    pub has_expiry:        bool,
    pub warn_days_before:  i32,
    pub grace_period_days: i32,
    pub vehicle_classes:   Option<Vec<String>>,
}
