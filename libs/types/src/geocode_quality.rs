//! How precisely a destination was geocoded.
//!
//! Stored as TEXT with no SQL CHECK: values cross service boundaries and a new
//! level must be data, not DDL. Anything unrecognised reads as `Unknown`, which
//! fails closed for the store resolver.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Ordered best to worst: Rooftop > Building > Parcel > Interpolated > Street
/// > Locality > Unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GeocodeQuality {
    Rooftop,
    Building,
    Parcel,
    Interpolated,
    Street,
    Locality,
    Unknown,
}

impl GeocodeQuality {
    /// The stored string (upper-case).
    pub fn as_str(&self) -> &'static str {
        match self {
            GeocodeQuality::Rooftop      => "ROOFTOP",
            GeocodeQuality::Building     => "BUILDING",
            GeocodeQuality::Parcel       => "PARCEL",
            GeocodeQuality::Interpolated => "INTERPOLATED",
            GeocodeQuality::Street       => "STREET",
            GeocodeQuality::Locality     => "LOCALITY",
            GeocodeQuality::Unknown      => "UNKNOWN",
        }
    }

    /// Read a stored value. Exact upper-case names only; anything else
    /// (lower-case, empty, a level this build does not know) is `Unknown`.
    pub fn parse(s: &str) -> Self {
        match s {
            "ROOFTOP"      => GeocodeQuality::Rooftop,
            "BUILDING"     => GeocodeQuality::Building,
            "PARCEL"       => GeocodeQuality::Parcel,
            "INTERPOLATED" => GeocodeQuality::Interpolated,
            "STREET"       => GeocodeQuality::Street,
            "LOCALITY"     => GeocodeQuality::Locality,
            _              => GeocodeQuality::Unknown,
        }
    }

    fn rank(&self) -> u8 {
        match self {
            GeocodeQuality::Rooftop      => 6,
            GeocodeQuality::Building     => 5,
            GeocodeQuality::Parcel       => 4,
            GeocodeQuality::Interpolated => 3,
            GeocodeQuality::Street       => 2,
            GeocodeQuality::Locality     => 1,
            GeocodeQuality::Unknown      => 0,
        }
    }

    /// The lower of `self` and `cap`. A cap never raises quality.
    pub fn capped_at(self, cap: GeocodeQuality) -> Self {
        if self.rank() > cap.rank() { cap } else { self }
    }
}

// Serialized as the stored string; an unrecognised value deserializes to Unknown.
impl Serialize for GeocodeQuality {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for GeocodeQuality {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(GeocodeQuality::parse(&s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [GeocodeQuality; 7] = [
        GeocodeQuality::Rooftop,
        GeocodeQuality::Building,
        GeocodeQuality::Parcel,
        GeocodeQuality::Interpolated,
        GeocodeQuality::Street,
        GeocodeQuality::Locality,
        GeocodeQuality::Unknown,
    ];

    #[test]
    fn stored_strings_are_the_spec_names_and_round_trip() {
        let names = ["ROOFTOP", "BUILDING", "PARCEL", "INTERPOLATED", "STREET", "LOCALITY", "UNKNOWN"];
        for (q, n) in ALL.iter().zip(names) {
            assert_eq!(q.as_str(), n);
            assert_eq!(GeocodeQuality::parse(n), *q);
        }
    }

    #[test]
    fn unrecognized_quality_reads_as_unknown() {
        for s in ["", "rooftop", "Rooftop", " ROOFTOP", "EXACT", "NULL", "ROOFTOP2"] {
            assert_eq!(GeocodeQuality::parse(s), GeocodeQuality::Unknown, "{s:?}");
        }
    }

    #[test]
    fn capped_at_takes_the_lower_and_never_raises() {
        use GeocodeQuality::*;
        assert_eq!(Rooftop.capped_at(Interpolated), Interpolated);
        assert_eq!(Interpolated.capped_at(Interpolated), Interpolated);
        assert_eq!(Street.capped_at(Interpolated), Street);
        assert_eq!(Unknown.capped_at(Locality), Unknown);
        assert_eq!(Locality.capped_at(Rooftop), Locality);
    }
}
