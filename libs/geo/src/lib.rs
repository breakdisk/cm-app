//! Geospatial utilities for LogisticOS.
//!
//! Provides:
//! - Coordinate types (lat/lng, bounding box)
//! - Haversine distance calculation
//! - Nearest-neighbor stop ordering
//! - Philippine logistics hub defaults

use serde::{Deserialize, Serialize};

const EARTH_RADIUS_KM: f64 = 6371.0;

// ── Core types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Coordinates {
    pub lat: f64,
    pub lng: f64,
}

impl Coordinates {
    pub fn new(lat: f64, lng: f64) -> Self { Self { lat, lng } }

    pub fn is_valid(&self) -> bool {
        self.lat >= -90.0 && self.lat <= 90.0
            && self.lng >= -180.0 && self.lng <= 180.0
    }

    pub fn distance_km(&self, other: &Coordinates) -> f64 {
        haversine_km(self, other)
    }

    /// Haversine × 1.3 road factor.
    pub fn driving_distance_km(&self, other: &Coordinates) -> f64 {
        self.distance_km(other) * 1.3
    }

    /// WKT string for PostGIS: `POINT(lng lat)`
    pub fn wkt(&self) -> String {
        format!("POINT({} {})", self.lng, self.lat)
    }
}

impl std::fmt::Display for Coordinates {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{},{}", self.lat, self.lng)
    }
}

// ── Bounding box ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BoundingBox {
    pub min_lat: f64,
    pub min_lng: f64,
    pub max_lat: f64,
    pub max_lng: f64,
}

impl BoundingBox {
    pub fn new(min_lat: f64, min_lng: f64, max_lat: f64, max_lng: f64) -> Self {
        Self { min_lat, min_lng, max_lat, max_lng }
    }

    pub fn from_center(center: &Coordinates, radius_km: f64) -> Self {
        let lat_delta = radius_km / EARTH_RADIUS_KM * (180.0 / std::f64::consts::PI);
        let lng_delta = lat_delta / center.lat.to_radians().cos();
        Self {
            min_lat: center.lat - lat_delta,
            min_lng: center.lng - lng_delta,
            max_lat: center.lat + lat_delta,
            max_lng: center.lng + lng_delta,
        }
    }

    pub fn contains(&self, point: &Coordinates) -> bool {
        point.lat >= self.min_lat && point.lat <= self.max_lat
            && point.lng >= self.min_lng && point.lng <= self.max_lng
    }

    pub fn center(&self) -> Coordinates {
        Coordinates::new(
            (self.min_lat + self.max_lat) / 2.0,
            (self.min_lng + self.max_lng) / 2.0,
        )
    }
}

// ── Haversine formula ─────────────────────────────────────────────────────────

pub fn haversine_km(a: &Coordinates, b: &Coordinates) -> f64 {
    let d_lat = (b.lat - a.lat).to_radians();
    let d_lng = (b.lng - a.lng).to_radians();
    let a_lat = a.lat.to_radians();
    let b_lat = b.lat.to_radians();
    let h = (d_lat / 2.0).sin().powi(2)
        + a_lat.cos() * b_lat.cos() * (d_lng / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_KM * h.sqrt().asin()
}

// ── Delivery zone ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryZone {
    pub id:     String,
    pub name:   String,
    pub bounds: BoundingBox,
}

impl DeliveryZone {
    pub fn contains(&self, point: &Coordinates) -> bool {
        self.bounds.contains(point)
    }
}

pub fn assign_zone<'a>(point: &Coordinates, zones: &'a [DeliveryZone]) -> Option<&'a DeliveryZone> {
    zones.iter().find(|z| z.contains(point))
}

// ── Nearest-neighbor stop ordering ───────────────────────────────────────────

/// Sort delivery stops in nearest-neighbor order from origin.
/// Returns indices into `stops` in visit order. O(n²) — suitable for n ≤ 200.
pub fn nearest_neighbor_order(origin: &Coordinates, stops: &[Coordinates]) -> Vec<usize> {
    let n = stops.len();
    let mut visited = vec![false; n];
    let mut order   = Vec::with_capacity(n);
    let mut current = *origin;

    for _ in 0..n {
        let next = stops.iter().enumerate()
            .filter(|(i, _)| !visited[*i])
            .map(|(i, c)| (i, haversine_km(&current, c)))
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        if let Some((idx, _)) = next {
            visited[idx] = true;
            order.push(idx);
            current = stops[idx];
        }
    }
    order
}

// ── Philippine logistics hub defaults ────────────────────────────────────────

pub mod ph {
    use super::Coordinates;
    pub const MANILA:    Coordinates = Coordinates { lat: 14.5995, lng: 120.9842 };
    pub const CEBU:      Coordinates = Coordinates { lat: 10.3157, lng: 123.8854 };
    pub const DAVAO:     Coordinates = Coordinates { lat: 7.0731,  lng: 125.6128 };
    pub const ILOILO:    Coordinates = Coordinates { lat: 10.7202, lng: 122.5621 };
    pub const CAGAYAN:   Coordinates = Coordinates { lat: 8.4822,  lng: 124.6472 };
    pub const ZAMBOANGA: Coordinates = Coordinates { lat: 6.9214,  lng: 122.0790 };
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn haversine_manila_cebu() {
        let d = haversine_km(&ph::MANILA, &ph::CEBU);
        assert!((d - 565.0).abs() < 20.0, "Manila-Cebu = {:.1} km", d);
    }

    #[test]
    fn bounding_box_center_contains() {
        let bb = BoundingBox::from_center(&ph::MANILA, 10.0);
        assert!(bb.contains(&ph::MANILA));
    }

    #[test]
    fn nearest_neighbor_returns_all() {
        let stops = vec![
            Coordinates::new(14.60, 120.99),
            Coordinates::new(14.55, 121.00),
            Coordinates::new(14.65, 120.98),
        ];
        let order = nearest_neighbor_order(&ph::MANILA, &stops);
        assert_eq!(order.len(), 3);
    }
}
