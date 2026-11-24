use std::pin::Pin;
use std::future::Future;

use logisticos_types::Address;

use crate::application::{
    commands::AddressInput,
    services::shipment_service::AddressNormalizer,
};

pub mod mapbox_geocoder;
pub use mapbox_geocoder::MapboxGeocoder;
pub mod road_router;
pub use road_router::{DirectRouter, DistanceBasis, Drive, MapboxDirections, RoadRouter};

/// Passthrough normalizer — maps the input fields directly without external geocoding.
/// Used as a fallback when no geocoder token is configured; shipments created through
/// this path will have `coordinates: None` and dispatch will reject them until the
/// coords are populated out-of-band.
pub struct PassthroughNormalizer;

impl AddressNormalizer for PassthroughNormalizer {
    fn normalize<'a>(
        &'a self,
        input: &'a AddressInput,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Address>> + Send + 'a>> {
        Box::pin(async move {
            Ok(Address {
                line1:        input.line1.clone(),
                line2:        input.line2.clone(),
                barangay:     input.barangay.clone(),
                city:         input.city.clone(),
                province:     input.province.clone(),
                postal_code:  input.postal_code.clone(),
                country_code: input.country_code.clone(),
                coordinates:  None, // enriched async by geocoder in prod
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use logisticos_types::GeocodeQuality;

    #[tokio::test]
    async fn a_normalizer_without_quality_records_unknown() {
        let input = AddressInput {
            line1: "1 Rizal St".into(),
            line2: None,
            barangay: None,
            city: "Manila".into(),
            province: "NCR".into(),
            postal_code: "1000".into(),
            country_code: "PH".into(),
            pin_lat: None, pin_lng: None, building: None, unit: None, area: None,
        };
        let out = PassthroughNormalizer.normalize_detailed(&input).await.expect("normalizes");
        assert_eq!(out.geocoding.quality, GeocodeQuality::Unknown);
        assert_eq!(out.geocoding.source, None);
        assert_eq!(out.address.line1, "1 Rizal St");
    }
}
