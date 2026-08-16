use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CourierLocation {
    pub id:               Uuid,
    pub tenant_id:        Uuid,
    pub courier_id:       Uuid,
    pub lat:              f64,
    pub lng:              f64,
    pub accuracy_m:       Option<f32>,
    pub speed_kph:        Option<f32>,
    pub heading_deg:      Option<f32>,
    /// Hardware clock at the physical moment of capture. Primary time basis for
    /// SLA maths; `None` only for server-generated points.
    pub device_timestamp: Option<DateTime<Utc>>,
    pub recorded_at:      DateTime<Utc>,
}

impl CourierLocation {
    pub fn new(
        tenant_id: Uuid,
        courier_id: Uuid,
        lat: f64,
        lng: f64,
        device_timestamp: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            courier_id,
            lat,
            lng,
            accuracy_m: None,
            speed_kph: None,
            heading_deg: None,
            device_timestamp,
            recorded_at: Utc::now(),
        }
    }

    /// The timestamp SLA and velocity calculations should use: the device clock
    /// where we have it, backend receipt time only as a fallback.
    pub fn sla_timestamp(&self) -> DateTime<Utc> {
        self.device_timestamp.unwrap_or(self.recorded_at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sla_timestamp_prefers_the_device_clock() {
        let device = Utc::now() - chrono::Duration::seconds(45);
        let l = CourierLocation::new(Uuid::new_v4(), Uuid::new_v4(), 14.6, 120.98, Some(device));
        assert_eq!(l.sla_timestamp(), device, "device_timestamp must win when present");
        assert_ne!(l.sla_timestamp(), l.recorded_at);
    }

    #[test]
    fn sla_timestamp_falls_back_to_receipt_time_for_server_points() {
        let l = CourierLocation::new(Uuid::new_v4(), Uuid::new_v4(), 14.6, 120.98, None);
        assert_eq!(l.sla_timestamp(), l.recorded_at);
    }
}
