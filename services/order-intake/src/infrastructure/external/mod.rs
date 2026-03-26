use std::pin::Pin;
use std::future::Future;

use logisticos_types::Address;

use crate::application::{
    commands::AddressInput,
    services::shipment_service::AddressNormalizer,
};

/// Passthrough normalizer — maps the input fields directly without external geocoding.
/// In production this would call a geocoding API (e.g., Google Maps, HERE).
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
