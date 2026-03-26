use logisticos_types::{RouteId, DriverId, VehicleId, TenantId, Coordinates};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Route {
    pub id: RouteId,
    pub tenant_id: TenantId,
    pub driver_id: DriverId,
    pub vehicle_id: VehicleId,
    pub stops: Vec<DeliveryStop>,
    pub status: RouteStatus,
    pub total_distance_km: f64,
    pub estimated_duration_minutes: u32,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryStop {
    pub sequence: u32,
    pub shipment_id: uuid::Uuid,
    pub address: logisticos_types::Address,
    pub time_window_start: Option<DateTime<Utc>>,
    pub time_window_end: Option<DateTime<Utc>>,
    pub estimated_arrival: Option<DateTime<Utc>>,
    pub actual_arrival: Option<DateTime<Utc>>,
    pub stop_type: StopType,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum StopType {
    Pickup,
    Delivery,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum RouteStatus {
    Planned,
    InProgress,
    Completed,
    Cancelled,
}

impl Route {
    /// Business rule: A route cannot be modified once started
    pub fn can_modify(&self) -> bool {
        self.status == RouteStatus::Planned
    }

    /// Business rule: Max stops per route depends on vehicle capacity
    pub fn is_at_capacity(&self, max_stops: usize) -> bool {
        self.stops.len() >= max_stops
    }

    /// Business logic: Add a stop and re-sort by proximity (greedy nearest-neighbor)
    pub fn add_stop_greedy(&mut self, stop: DeliveryStop) -> Result<(), &'static str> {
        if !self.can_modify() {
            return Err("Cannot modify an in-progress or completed route");
        }
        self.stops.push(stop);
        self.sort_stops_by_proximity();
        Ok(())
    }

    fn sort_stops_by_proximity(&mut self) {
        // Greedy nearest-neighbor from first stop
        if self.stops.len() <= 2 { return; }
        let mut sorted = Vec::with_capacity(self.stops.len());
        let mut remaining = self.stops.drain(..).collect::<Vec<_>>();
        sorted.push(remaining.remove(0));

        while !remaining.is_empty() {
            let last = sorted.last().unwrap();
            let last_coords = last.address.coordinates.unwrap_or(Coordinates { lat: 0.0, lng: 0.0 });
            let nearest_idx = remaining
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    let ca = a.address.coordinates.unwrap_or(Coordinates { lat: 0.0, lng: 0.0 });
                    let cb = b.address.coordinates.unwrap_or(Coordinates { lat: 0.0, lng: 0.0 });
                    let da = last_coords.distance_km(&ca);
                    let db = last_coords.distance_km(&cb);
                    da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(i, _)| i)
                .unwrap_or(0);
            sorted.push(remaining.remove(nearest_idx));
        }

        self.stops = sorted;
        for (i, stop) in self.stops.iter_mut().enumerate() {
            stop.sequence = i as u32 + 1;
        }
    }
}
