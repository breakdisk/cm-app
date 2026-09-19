//! Driving distance and time between two located points.
//!
//! Prices were built on the straight line between the two geocoded ends,
//! which understates any trip that has to go around a bay, over a bridge or
//! through a one-way grid. A quote is now priced on the road: Mapbox
//! Directions, on the geocoder's token.
//!
//! Two cases are not the road:
//! - **No driving route exists** (another island, another country): the
//!   direct distance is used and the quote says so (`DistanceBasis::Direct`).
//!   There is no road figure to be wrong about.
//! - **Routing is down**: an error. The quote is refused rather than priced on
//!   a figure known to be short.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use logisticos_types::Coordinates;
use serde::Serialize;

const MAPBOX_DIRECTIONS_URL: &str = "https://api.mapbox.com/directions/v5/mapbox/driving";
const ROUTE_TIMEOUT: Duration = Duration::from_secs(5);

/// What a distance was measured along.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DistanceBasis {
    /// The driving route.
    Road,
    /// The straight line: no driving route exists, or no router is configured.
    Direct,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drive {
    pub distance_km: f64,
    /// Driving time, where there is a road.
    pub minutes: Option<u32>,
    pub basis: DistanceBasis,
}

impl Drive {
    fn direct(from: &Coordinates, to: &Coordinates) -> Self {
        Self { distance_km: from.distance_km(to), minutes: None, basis: DistanceBasis::Direct }
    }
}

pub trait RoadRouter: Send + Sync {
    fn drive<'a>(
        &'a self,
        from: Coordinates,
        to: Coordinates,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Drive>> + Send + 'a>>;
}

/// No routing configured (a development box without a Mapbox token): the
/// straight line, as before. Production sets `GEOCODER__MAPBOX_ACCESS_TOKEN`,
/// which the geocoder needs anyway.
pub struct DirectRouter;

impl RoadRouter for DirectRouter {
    fn drive<'a>(
        &'a self,
        from: Coordinates,
        to: Coordinates,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Drive>> + Send + 'a>> {
        Box::pin(async move { Ok(Drive::direct(&from, &to)) })
    }
}

pub struct MapboxDirections {
    access_token: String,
    client: reqwest::Client,
}

impl MapboxDirections {
    pub fn new(access_token: String) -> anyhow::Result<Self> {
        let client = reqwest::Client::builder().timeout(ROUTE_TIMEOUT).build()?;
        Ok(Self { access_token, client })
    }

    async fn ask(&self, from: &Coordinates, to: &Coordinates) -> anyhow::Result<Routed> {
        let url = format!("{MAPBOX_DIRECTIONS_URL}/{},{};{},{}", from.lng, from.lat, to.lng, to.lat);
        let resp = self
            .client
            .get(&url)
            .query(&[("access_token", self.access_token.as_str()), ("overview", "false"), ("alternatives", "false")])
            .send()
            .await?;
        let status = resp.status().as_u16();
        // A "no route" answer can come back as a 4xx with a code in the body,
        // so the body is read whatever the status.
        let body: serde_json::Value = resp.json().await.unwrap_or(serde_json::Value::Null);
        read_directions(status, &body).map_err(anyhow::Error::msg)
    }
}

impl RoadRouter for MapboxDirections {
    fn drive<'a>(
        &'a self,
        from: Coordinates,
        to: Coordinates,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Drive>> + Send + 'a>> {
        Box::pin(async move {
            // One retry: a quote is interactive, so no longer than that.
            let routed = match self.ask(&from, &to).await {
                Ok(r) => r,
                Err(first) => {
                    tracing::warn!(err = %first, "Mapbox directions failed — retrying once");
                    self.ask(&from, &to).await?
                }
            };
            Ok(match routed {
                Routed::Road { meters, seconds } => Drive {
                    distance_km: meters / 1000.0,
                    minutes: Some(minutes_from(seconds)),
                    basis: DistanceBasis::Road,
                },
                Routed::NoRoad => Drive::direct(&from, &to),
            })
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Routed {
    Road { meters: f64, seconds: f64 },
    /// Mapbox found no driving route between the points.
    NoRoad,
}

fn minutes_from(seconds: f64) -> u32 {
    let m = (seconds / 60.0).ceil();
    if m.is_finite() && m >= 0.0 { m.min(f64::from(u32::MAX)) as u32 } else { 0 }
}

/// A Directions answer: a route, no road at all, or an error.
pub(crate) fn read_directions(status: u16, body: &serde_json::Value) -> Result<Routed, String> {
    let code = body.get("code").and_then(|c| c.as_str()).unwrap_or("");
    match code {
        "Ok" => {
            let route = body.pointer("/routes/0").ok_or("Directions answered Ok with no route")?;
            let meters = route.get("distance").and_then(serde_json::Value::as_f64).ok_or("route has no distance")?;
            let seconds = route.get("duration").and_then(serde_json::Value::as_f64).ok_or("route has no duration")?;
            if !meters.is_finite() || meters < 0.0 {
                return Err(format!("route distance {meters} is not a distance"));
            }
            Ok(Routed::Road { meters, seconds })
        }
        "NoRoute" | "NoSegment" => Ok(Routed::NoRoad),
        "" => Err(format!("Directions HTTP {status} with no code")),
        other => Err(format!("Directions HTTP {status}: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_route_is_read_in_km_and_minutes() {
        let body = json!({ "code": "Ok", "routes": [{ "distance": 18_432.7, "duration": 2_410.0 }] });
        assert_eq!(read_directions(200, &body), Ok(Routed::Road { meters: 18_432.7, seconds: 2_410.0 }));
        assert_eq!(minutes_from(2_410.0), 41);
    }

    #[test]
    fn no_road_is_not_an_error() {
        // Manila to Cebu: no driving route. Mapbox may send it as a 4xx.
        assert_eq!(read_directions(200, &json!({ "code": "NoRoute" })), Ok(Routed::NoRoad));
        assert_eq!(read_directions(422, &json!({ "code": "NoSegment" })), Ok(Routed::NoRoad));
    }

    #[test]
    fn anything_else_is_an_error_not_a_price() {
        assert!(read_directions(401, &json!({ "code": "InvalidToken", "message": "Not Authorized" })).is_err());
        assert!(read_directions(503, &serde_json::Value::Null).is_err());
        assert!(read_directions(200, &json!({ "code": "Ok", "routes": [] })).is_err());
        assert!(read_directions(200, &json!({ "code": "Ok", "routes": [{ "distance": -1.0, "duration": 5.0 }] })).is_err());
    }

    #[tokio::test]
    async fn without_routing_the_direct_distance_is_labelled() {
        let a = Coordinates { lat: 14.5547, lng: 121.0244 };
        let b = Coordinates { lat: 14.5176, lng: 121.0509 };
        let d = DirectRouter.drive(a, b).await.expect("direct");
        assert_eq!(d.basis, DistanceBasis::Direct);
        assert!(d.minutes.is_none());
        assert!((d.distance_km - a.distance_km(&b)).abs() < 1e-9);
    }
}
