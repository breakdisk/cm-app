//! Forward address → (lat, lng) resolution via Mapbox Geocoding v6.
//!
//! Graceful-degradation policy: on any failure (network timeout, 4xx/5xx,
//! empty result set), log a warning and return `coordinates: None`. The
//! shipment still gets created; the dispatch service will reject it with a
//! clean "no coordinates" error when someone tries to assign a driver. That
//! keeps geocoder outages from blocking the booking flow entirely. The
//! destination's geocoding quality is then `UNKNOWN`, never a new error.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use chrono::{DateTime, Utc};
use logisticos_types::{Address, Coordinates, GeocodeQuality};

use crate::{
    application::{
        commands::AddressInput,
        services::shipment_service::AddressNormalizer,
    },
    domain::value_objects::{geocoding::SOURCE_PROVIDER, GeocodedAddress, GeocodingRecord},
};

const MAPBOX_FORWARD_URL: &str = "https://api.mapbox.com/search/geocode/v6/forward";
const GEOCODE_TIMEOUT: Duration = Duration::from_secs(5);

/// `dest_geocoding_provider` for results from this geocoder.
const PROVIDER: &str = "mapbox";

/// Map a Mapbox v6 result onto the quality table in the neighborhood network
/// design (section "Address quality"), then apply the `match_code.confidence`
/// cap: `medium` caps at INTERPOLATED, `low` at LOCALITY. A cap never raises.
///
/// - `address` + rooftop / point / parcel / interpolated: ROOFTOP / BUILDING /
///   PARCEL / INTERPOLATED. An `address` with any other accuracy, or none, is
///   UNKNOWN: the table does not say how precise it is, so it is not trusted.
/// - `street`, or accuracy `street` / `intersection`: STREET.
/// - accuracy `approximate`, or neighborhood / locality / place and the broader
///   areas (postcode, district, region, country): LOCALITY.
/// - anything else: UNKNOWN.
pub fn quality_from_mapbox(
    feature_type: &str,
    accuracy: Option<&str>,
    confidence: Option<&str>,
) -> GeocodeQuality {
    let feature_type = feature_type.trim().to_ascii_lowercase();
    let accuracy = accuracy.map(|a| a.trim().to_ascii_lowercase());
    let accuracy = accuracy.as_deref();

    let base = match feature_type.as_str() {
        "address" => match accuracy {
            Some("rooftop")      => GeocodeQuality::Rooftop,
            Some("point")        => GeocodeQuality::Building,
            Some("parcel")       => GeocodeQuality::Parcel,
            Some("interpolated") => GeocodeQuality::Interpolated,
            Some("street") | Some("intersection") => GeocodeQuality::Street,
            Some("approximate")  => GeocodeQuality::Locality,
            _                    => GeocodeQuality::Unknown,
        },
        "street" | "intersection" => GeocodeQuality::Street,
        "neighborhood" | "locality" | "place" | "postcode" | "district" | "region" | "country" => {
            GeocodeQuality::Locality
        }
        // A type this build does not know: the accuracy is all there is.
        _ => match accuracy {
            Some("street") | Some("intersection") => GeocodeQuality::Street,
            Some("approximate")                   => GeocodeQuality::Locality,
            _                                     => GeocodeQuality::Unknown,
        },
    };

    match confidence.map(|c| c.trim().to_ascii_lowercase()).as_deref() {
        None           => base,
        Some("exact") | Some("high") => base,
        Some("medium") => base.capped_at(GeocodeQuality::Interpolated),
        // `low`, and any value this build does not recognise: fail closed.
        Some(_)        => base.capped_at(GeocodeQuality::Locality),
    }
}

pub struct MapboxGeocoder {
    access_token: String,
    client: reqwest::Client,
    /// Send `permanent=true` and keep the provider's place id and confidence.
    /// Off by default: it is a separately-priced Mapbox mode.
    permanent: bool,
}

impl MapboxGeocoder {
    pub fn new(access_token: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(GEOCODE_TIMEOUT)
            .build()
            .expect("reqwest client must build");
        Self { access_token, client, permanent: false }
    }

    /// Turn on Mapbox permanent geocoding (`MAPBOX__PERMANENT_GEOCODING`).
    pub fn with_permanent(mut self, permanent: bool) -> Self {
        self.permanent = permanent;
        self
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

    async fn forward(
        &self,
        query: &str,
        country: &str,
    ) -> anyhow::Result<Option<(Coordinates, GeocodingRecord)>> {
        let country_lc = country.to_lowercase();
        let mut params = vec![
            ("q", query),
            ("access_token", self.access_token.as_str()),
            ("country", country_lc.as_str()),
            ("limit", "1"),
        ];
        if self.permanent {
            params.push(("permanent", "true"));
        }
        let resp: serde_json::Value = self
            .client
            .get(MAPBOX_FORWARD_URL)
            .query(&params)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        Ok(parse_forward_response(&resp, self.permanent, Utc::now()))
    }
}

/// Pull coordinates and the geocoding record out of a v6 forward response.
///
/// Only with `permanent` set are the provider's place id and confidence kept:
/// storing them is what the `permanent` licence pays for.
fn parse_forward_response(
    resp: &serde_json::Value,
    permanent: bool,
    now: DateTime<Utc>,
) -> Option<(Coordinates, GeocodingRecord)> {
    // v6 response: features[0].geometry.coordinates = [lng, lat]
    let (lng, lat) = resp
        .pointer("/features/0/geometry/coordinates")
        .and_then(|v| v.as_array())
        .filter(|a| a.len() == 2)
        .and_then(|a| Some((a[0].as_f64()?, a[1].as_f64()?)))?;

    let text = |path: &str| resp.pointer(path).and_then(|v| v.as_str());
    let feature_type = text("/features/0/properties/feature_type").unwrap_or("");
    let accuracy     = text("/features/0/properties/coordinates/accuracy");
    let confidence   = text("/features/0/properties/match_code/confidence");

    let record = GeocodingRecord {
        quality:     quality_from_mapbox(feature_type, accuracy, confidence),
        source:      Some(SOURCE_PROVIDER.to_string()),
        provider:    Some(PROVIDER.to_string()),
        place_id:    text("/features/0/properties/mapbox_id").filter(|_| permanent).map(str::to_string),
        confidence:  confidence.filter(|_| permanent).map(str::to_string),
        geocoded_at: Some(now),
    };
    Some((Coordinates { lat, lng }, record))
}

impl AddressNormalizer for MapboxGeocoder {
    fn normalize<'a>(
        &'a self,
        input: &'a AddressInput,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<Address>> + Send + 'a>> {
        Box::pin(async move { Ok(self.normalize_detailed(input).await?.address) })
    }

    fn normalize_detailed<'a>(
        &'a self,
        input: &'a AddressInput,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<GeocodedAddress>> + Send + 'a>> {
        Box::pin(async move {
            let query = Self::build_query(input);
            // A failed or empty geocode is UNKNOWN and never an error: the
            // booking goes ahead exactly as it always did.
            let (coords, geocoding) = if query.is_empty() {
                (None, GeocodingRecord::unknown())
            } else {
                match self.forward(&query, &input.country_code).await {
                    Ok(Some((c, record))) => (Some(c), record),
                    Ok(None) => {
                        tracing::warn!(
                            query = %query,
                            "Mapbox geocode returned no results"
                        );
                        (None, GeocodingRecord::unknown())
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            query = %query,
                            "Mapbox geocode failed — shipment will be created without coordinates"
                        );
                        (None, GeocodingRecord::unknown())
                    }
                }
            };

            Ok(GeocodedAddress {
                address: Address {
                    line1:        input.line1.clone(),
                    line2:        input.line2.clone(),
                    barangay:     input.barangay.clone(),
                    city:         input.city.clone(),
                    province:     input.province.clone(),
                    postal_code:  input.postal_code.clone(),
                    country_code: input.country_code.clone(),
                    coordinates:  coords,
                },
                geocoding,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use GeocodeQuality::*;

    // ── The spec's Address quality table, one test per row ──────────────────

    #[test]
    fn address_rooftop_is_rooftop() {
        assert_eq!(quality_from_mapbox("address", Some("rooftop"), Some("exact")), Rooftop);
    }

    #[test]
    fn address_point_is_building() {
        assert_eq!(quality_from_mapbox("address", Some("point"), Some("exact")), Building);
    }

    #[test]
    fn address_parcel_is_parcel() {
        assert_eq!(quality_from_mapbox("address", Some("parcel"), Some("exact")), Parcel);
    }

    #[test]
    fn address_interpolated_is_interpolated() {
        assert_eq!(quality_from_mapbox("address", Some("interpolated"), Some("exact")), Interpolated);
    }

    #[test]
    fn street_and_intersection_are_street() {
        assert_eq!(quality_from_mapbox("street", None, Some("exact")), Street);
        assert_eq!(quality_from_mapbox("street", Some("street"), Some("high")), Street);
        assert_eq!(quality_from_mapbox("address", Some("intersection"), Some("exact")), Street);
        assert_eq!(quality_from_mapbox("intersection", None, None), Street);
    }

    #[test]
    fn approximate_neighborhood_locality_and_place_are_locality() {
        assert_eq!(quality_from_mapbox("address", Some("approximate"), Some("exact")), Locality);
        assert_eq!(quality_from_mapbox("neighborhood", None, Some("exact")), Locality);
        assert_eq!(quality_from_mapbox("locality", None, Some("exact")), Locality);
        assert_eq!(quality_from_mapbox("place", None, Some("exact")), Locality);
    }

    #[test]
    fn broader_areas_are_locality() {
        for t in ["postcode", "district", "region", "country"] {
            assert_eq!(quality_from_mapbox(t, None, Some("exact")), Locality, "{t}");
        }
    }

    #[test]
    fn none_or_invalid_is_unknown() {
        assert_eq!(quality_from_mapbox("", None, None), Unknown);
        assert_eq!(quality_from_mapbox("poi", None, Some("exact")), Unknown);
        assert_eq!(quality_from_mapbox("nonsense", Some("nonsense"), Some("exact")), Unknown);
    }

    #[test]
    fn address_without_a_known_accuracy_is_unknown() {
        // Not in the table: an address that does not say how precise it is.
        assert_eq!(quality_from_mapbox("address", None, Some("exact")), Unknown);
        assert_eq!(quality_from_mapbox("address", Some("nonsense"), Some("exact")), Unknown);
    }

    #[test]
    fn matching_is_case_and_whitespace_insensitive() {
        assert_eq!(quality_from_mapbox(" Address ", Some("ROOFTOP"), Some("EXACT")), Rooftop);
    }

    // ── match_code.confidence caps ──────────────────────────────────────────

    #[test]
    fn medium_confidence_caps_at_interpolated() {
        assert_eq!(quality_from_mapbox("address", Some("rooftop"), Some("medium")), Interpolated);
        assert_eq!(quality_from_mapbox("address", Some("point"), Some("medium")), Interpolated);
        assert_eq!(quality_from_mapbox("address", Some("parcel"), Some("medium")), Interpolated);
        assert_eq!(quality_from_mapbox("address", Some("interpolated"), Some("medium")), Interpolated);
    }

    #[test]
    fn low_confidence_caps_at_locality() {
        assert_eq!(quality_from_mapbox("address", Some("rooftop"), Some("low")), Locality);
        assert_eq!(quality_from_mapbox("street", None, Some("low")), Locality);
        assert_eq!(quality_from_mapbox("address", Some("interpolated"), Some("low")), Locality);
    }

    #[test]
    fn cap_never_raises_quality() {
        // Already below the cap: unchanged, not lifted up to it.
        assert_eq!(quality_from_mapbox("street", None, Some("medium")), Street);
        assert_eq!(quality_from_mapbox("place", None, Some("medium")), Locality);
        assert_eq!(quality_from_mapbox("place", None, Some("low")), Locality);
        assert_eq!(quality_from_mapbox("", None, Some("low")), Unknown);
        assert_eq!(quality_from_mapbox("", None, Some("medium")), Unknown);
    }

    #[test]
    fn exact_high_or_missing_confidence_does_not_cap() {
        for c in [Some("exact"), Some("high"), None] {
            assert_eq!(quality_from_mapbox("address", Some("rooftop"), c), Rooftop, "{c:?}");
        }
    }

    #[test]
    fn unrecognized_confidence_caps_at_locality() {
        // A confidence value this build does not know is not evidence of
        // precision: fail closed, like the quality table does.
        for c in ["weird", "", "very_high", "exactish"] {
            assert_eq!(quality_from_mapbox("address", Some("rooftop"), Some(c)), Locality, "{c:?}");
        }
        // Still never raises: below the cap stays put.
        assert_eq!(quality_from_mapbox("", None, Some("weird")), Unknown);
        // Absent confidence is unchanged.
        assert_eq!(quality_from_mapbox("address", Some("rooftop"), None), Rooftop);
    }

    // ── What is stored, with and without permanent geocoding ────────────────

    fn response() -> serde_json::Value {
        serde_json::json!({
            "features": [{
                "geometry": { "coordinates": [121.0, 14.5] },
                "properties": {
                    "mapbox_id": "dXJuOm1ieGFkcjoxMjM",
                    "feature_type": "address",
                    "coordinates": { "accuracy": "rooftop" },
                    "match_code": { "confidence": "exact" }
                }
            }]
        })
    }

    #[test]
    fn flag_off_stores_quality_and_source_but_no_place_id_or_confidence() {
        let now = Utc::now();
        let (coords, rec) = parse_forward_response(&response(), false, now).expect("a feature");
        assert_eq!((coords.lat, coords.lng), (14.5, 121.0));
        assert_eq!(rec.quality, Rooftop);
        assert_eq!(rec.source.as_deref(), Some("provider"));
        assert_eq!(rec.provider.as_deref(), Some("mapbox"));
        assert_eq!(rec.geocoded_at, Some(now));
        assert_eq!(rec.place_id, None);
        assert_eq!(rec.confidence, None);
    }

    #[test]
    fn flag_on_also_stores_place_id_and_confidence() {
        let (_, rec) = parse_forward_response(&response(), true, Utc::now()).expect("a feature");
        assert_eq!(rec.quality, Rooftop);
        assert_eq!(rec.place_id.as_deref(), Some("dXJuOm1ieGFkcjoxMjM"));
        assert_eq!(rec.confidence.as_deref(), Some("exact"));
    }

    #[test]
    fn a_feature_with_no_properties_has_coordinates_and_unknown_quality() {
        let resp = serde_json::json!({ "features": [{ "geometry": { "coordinates": [1.0, 2.0] } }] });
        let (_, rec) = parse_forward_response(&resp, true, Utc::now()).expect("a feature");
        assert_eq!(rec.quality, Unknown);
        assert_eq!(rec.place_id, None);
    }

    #[test]
    fn no_features_gives_nothing() {
        assert!(parse_forward_response(&serde_json::json!({ "features": [] }), false, Utc::now()).is_none());
    }
}
