use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VehicleStats {
    pub total_vehicles: i64,
    pub active: i64,
    pub under_maintenance: i64,
    pub decommissioned: i64,
}

impl VehicleStats {
    pub fn new(total: i64, active: i64, maintenance: i64, decommissioned: i64) -> Self {
        Self {
            total_vehicles: total,
            active,
            under_maintenance: maintenance,
            decommissioned,
        }
    }
}
