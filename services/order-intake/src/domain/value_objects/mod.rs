use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ServiceType {
    Standard,
    Express,
    SameDay,
    Balikbayan,
}

impl ServiceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Standard   => "standard",
            Self::Express    => "express",
            Self::SameDay    => "same_day",
            Self::Balikbayan => "balikbayan",
        }
    }

    /// Business rule: same-day orders must be placed before 14:00 local time.
    pub fn cutoff_hour(&self) -> Option<u32> {
        match self {
            Self::SameDay => Some(14),
            _             => None,
        }
    }
}

/// Weight in grams — stored as integer to avoid floating point issues.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ShipmentWeight {
    pub grams: u32,
}

impl ShipmentWeight {
    pub fn from_grams(g: u32)  -> Self { Self { grams: g } }
    pub fn from_kg(kg: f64)    -> Self { Self { grams: (kg * 1000.0).round() as u32 } }
    pub fn kg(&self)           -> f64  { self.grams as f64 / 1000.0 }

    /// Business rule: max single-parcel weight is 70kg (standard carrier limit).
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.grams == 0 { return Err("Weight must be greater than zero"); }
        if self.grams > 70_000 { return Err("Weight exceeds 70kg maximum for standard parcels"); }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ShipmentDimensions {
    pub length_cm: u32,
    pub width_cm:  u32,
    pub height_cm: u32,
}

impl ShipmentDimensions {
    /// Volumetric weight in grams (DIM factor = 5000 cm³/kg, standard carrier rate).
    pub fn volumetric_weight_grams(&self) -> u32 {
        let vol_cm3 = self.length_cm * self.width_cm * self.height_cm;
        (vol_cm3 as f64 / 5.0).round() as u32  // vol_cm3 / 5000 * 1000
    }
}

/// Tracking number format: LSPH + 10 digits. Example: LSPH0012345678
pub struct TrackingNumber;

impl TrackingNumber {
    pub fn generate() -> String {
        use rand::Rng;
        let n: u64 = rand::thread_rng().gen_range(1_000_000_000..9_999_999_999);
        format!("LSPH{:010}", n)
    }
}
