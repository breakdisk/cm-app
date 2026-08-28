pub mod awb_generator;
pub use awb_generator::{generate_child_awbs, AwbGenerator, AwbGeneratorError};

pub mod quote_token;
pub use quote_token::{sign as sign_quote_token, verify as verify_quote_token, QuoteTokenError, QuoteTokenPayload};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ServiceType {
    Standard,
    Express,
    SameDay,
    Balikbayan,
    International,
}

impl ServiceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Express => "express",
            Self::SameDay => "same_day",
            Self::Balikbayan => "balikbayan",
            Self::International => "international",
        }
    }

    /// Business rule: same-day orders must be placed before 14:00 local time.
    pub fn cutoff_hour(&self) -> Option<u32> {
        match self {
            Self::SameDay => Some(14),
            _ => None,
        }
    }

    /// Inverse of `as_str()`. The single source of truth for parsing a
    /// service-type string from a request body — both the quote endpoint
    /// and shipment creation use this, so a new service type only needs
    /// updating here.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "standard"      => Ok(Self::Standard),
            "express"       => Ok(Self::Express),
            "same_day"      => Ok(Self::SameDay),
            "balikbayan"    => Ok(Self::Balikbayan),
            "international" => Ok(Self::International),
            other => Err(format!("Unknown service type: {other}")),
        }
    }
}

/// Weight in grams — stored as integer to avoid floating point issues.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ShipmentWeight {
    pub grams: u32,
}

impl ShipmentWeight {
    pub fn from_grams(g: u32) -> Self {
        Self { grams: g }
    }
    pub fn from_kg(kg: f64) -> Self {
        Self {
            grams: (kg * 1000.0).round() as u32,
        }
    }
    pub fn kg(&self) -> f64 {
        self.grams as f64 / 1000.0
    }

    /// Business rule: max single-parcel weight is 70kg (standard carrier limit).
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.grams == 0 {
            return Err("Weight must be greater than zero");
        }
        if self.grams > 70_000 {
            return Err("Weight exceeds 70kg maximum for standard parcels");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ShipmentDimensions {
    pub length_cm: u32,
    pub width_cm: u32,
    pub height_cm: u32,
}

impl ShipmentDimensions {
    /// Volumetric weight in grams (DIM factor = 5000 cm³/kg, standard carrier rate).
    pub fn volumetric_weight_grams(&self) -> u32 {
        let vol_cm3 = self.length_cm * self.width_cm * self.height_cm;
        (vol_cm3 as f64 / 5.0).round() as u32 // vol_cm3 / 5000 * 1000
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_type_parse_and_as_str_round_trip_for_all_variants() {
        let variants = [
            ServiceType::Standard,
            ServiceType::Express,
            ServiceType::SameDay,
            ServiceType::Balikbayan,
            ServiceType::International,
        ];
        for variant in variants {
            let s = variant.as_str();
            let parsed = ServiceType::parse(s).unwrap_or_else(|e| {
                panic!("expected {s} to parse back to {variant:?}, got error: {e}")
            });
            assert_eq!(parsed, variant, "round trip mismatch for {s}");
        }
    }

    #[test]
    fn service_type_parse_rejects_unknown_string() {
        let err = ServiceType::parse("teleport").unwrap_err();
        assert_eq!(err, "Unknown service type: teleport");
    }
}
