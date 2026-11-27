//! What is known about how a destination was geocoded.
//!
//! The provider's id, accuracy and confidence are kept, not its full response,
//! which would duplicate personal data. Storing the id and confidence depends
//! on the Mapbox `permanent` question, so they are `None` unless that flag is on.

use chrono::{DateTime, Utc};
use logisticos_types::{Address, GeocodeQuality};
use serde::{Deserialize, Serialize};

/// `dest_geocoding_source` values.
pub const SOURCE_PROVIDER: &str = "provider";
/// The customer confirmed the location on a map. The coordinates are theirs.
pub const SOURCE_CUSTOMER_PIN: &str = "customer_pin";
/// `dest_geocoding_provider` for a customer pin.
pub const PROVIDER_CUSTOMER: &str = "customer";

/// The six `dest_geocod*` columns, as one value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeocodingRecord {
    pub quality: GeocodeQuality,
    /// `provider`, `customer_pin` or `ops_corrected`.
    pub source: Option<String>,
    pub provider: Option<String>,
    /// The provider's id for the place (Mapbox `mapbox_id`).
    pub place_id: Option<String>,
    /// The provider's own confidence word, when it gives one.
    pub confidence: Option<String>,
    pub geocoded_at: Option<DateTime<Utc>>,
}

impl GeocodingRecord {
    /// Nothing is known: no geocoder ran, or it failed. Every existing
    /// shipment reads as this.
    pub fn unknown() -> Self {
        Self {
            quality: GeocodeQuality::Unknown,
            source: None,
            provider: None,
            place_id: None,
            confidence: None,
            geocoded_at: None,
        }
    }
}

impl Default for GeocodingRecord {
    fn default() -> Self {
        Self::unknown()
    }
}

/// A normalised address and how it was geocoded.
#[derive(Debug, Clone)]
pub struct GeocodedAddress {
    pub address: Address,
    pub geocoding: GeocodingRecord,
}

/// The destination's building, unit and area, as the customer gave them. Kept
/// as given and never fed to the geocoder.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AddressParts {
    pub building: Option<String>,
    pub unit: Option<String>,
    pub area: Option<String>,
}
