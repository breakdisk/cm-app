//! Forward address → (lat, lng) resolution via Mapbox Geocoding v6.
//!
//! Graceful-degradation policy: on any failure (network timeout, 4xx/5xx,
//! empty result set), log a warning and return `coordinates: None`. The
//! shipment still gets created; the dispatch service will reject it with a
//! clean "no coordinates" error when someone tries to assign a driver. That
//! keeps geocoder outages from blocking the booking flow entirely.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use logisticos_types::{Address, Coordinates};

use crate::application::{
    commands::AddressInput,
    services::shipment_service::AddressNormalizer,
};

const MAPBOX_FORWARD_URL: &str = "https://api.mapbox.com/search/geocode/v6/forward";
const GEOCODE_TIMEOUT: Duration = Duration::from_secs(5);

pub struct MapboxGeocoder {
    access_token: String,
    client: reqwest::Client,
}

impl MapboxGeocoder {
    pub fn new(access_token: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(GEOCODE_TIMEOUT)
            .build()
            .expect("reqwest client must build");
        Self { access_token, client }
    }

    fn build_query(input: &AddressInput) -> String {
        [
            input.line1.as_str(),
            input.city.as_str(),
            input.province.as_str(),
            input.country_code.as_str(),
        ]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(", ")
    }

    async fn forward(&self, query: &str, country: &str) -> anyhow::Result<Option<Coordinates>> {
        let country_lc = country.to_lowercase();
        let resp: serde_json::Value = self
            .client
            .get(MAPBOX_FORWARD_URL)
            .query(&[
                ("q", query),
                ("access_token", self.access_token.as_str()),
                ("country", country_lc.as_str()),
                ("limit", "1"),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        // v6 response: features[0].geometry.coordinates = [lng, lat]
        let coords = resp
            .pointer("/features/0/geometry/coordinates")
            .and_then(|v| v.as_array())
            .filter(|a| a.len() == 2)
            .and_then(|a| Some((a[0].as_f64()?, a[1].as_f64()?)));

        Ok(coords.map(|(lng, lat)| Coordinates { lat, lng }))
    }
}

impl AddressNormalizer for MapboxGeocoder {
    fn normalize<'a>(
        &'a self,
        input: &'a AddressInput,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Address>> + Send + 'a>> {
        Box::pin(async move {
            let query = Self::build_query(input);
            let coords = if query.is_empty() {
                None
            } else {
                match self.forward(&query, &input.country_code).await {
                    Ok(Some(c)) => Some(c),
                    Ok(None) => {
                        tracing::warn!(
                            query = %query,
                            "Mapbox geocode returned no results"
                        );
                        None
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            query = %query,
                            "Mapbox geocode failed — shipment will be created without coordinates"
                        );
                        None
                    }
                }
            };

            Ok(Address {
                line1:        input.line1.clone(),
                line2:        input.line2.clone(),
                barangay:     input.barangay.clone(),
                city:         input.city.clone(),
                province:     input.province.clone(),
                postal_code:  input.postal_code.clone(),
                country_code: input.country_code.clone(),
                coordinates:  coords,
            })
        })
    }
}
