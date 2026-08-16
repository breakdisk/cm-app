use serde::{Deserialize, Serialize};

// ── Zone ──────────────────────────────────────────────────────────────────────

/// A logistics zone code used for SLA scoping and rate-card coverage.
/// Stored as a trimmed, non-empty string; comparison is case-insensitive.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Zone(String);

impl Zone {
    pub fn new(raw: impl Into<String>) -> anyhow::Result<Self> {
        let s = raw.into().trim().to_string();
        if s.is_empty() {
            anyhow::bail!("Zone cannot be empty");
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str { &self.0 }
}

impl std::fmt::Display for Zone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

// ── ServiceLevel ──────────────────────────────────────────────────────────────

/// The three service tiers recognized by the platform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceLevel {
    SameDay,
    NextDay,
    Standard,
}

impl ServiceLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SameDay  => "same_day",
            Self::NextDay  => "next_day",
            Self::Standard => "standard",
        }
    }

    // Inherent `from_str` is total (unknown input maps to a variant); `FromStr` implies fallible.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> anyhow::Result<Self> {
        match s {
            "same_day"  => Ok(Self::SameDay),
            "next_day"  => Ok(Self::NextDay),
            "standard"  => Ok(Self::Standard),
            other       => anyhow::bail!("Unknown service level: {other}"),
        }
    }

    /// Maximum promised delivery days for this tier (used to compute promised_by).
    pub fn max_days(&self) -> u8 {
        match self {
            Self::SameDay  => 1,
            Self::NextDay  => 2,
            Self::Standard => 5,
        }
    }
}

impl std::fmt::Display for ServiceLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ── CarrierCode ───────────────────────────────────────────────────────────────

/// Short uppercase carrier code used as a stable cross-service reference
/// (e.g. "JNT", "LBC", "GRAB"). 2–10 uppercase ASCII letters/digits only.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CarrierCode(String);

impl CarrierCode {
    pub fn new(raw: impl Into<String>) -> anyhow::Result<Self> {
        let s = raw.into().trim().to_uppercase();
        if s.len() < 2 || s.len() > 10 {
            anyhow::bail!("Carrier code must be 2–10 characters, got {}", s.len());
        }
        if !s.chars().all(|c| c.is_ascii_alphanumeric()) {
            anyhow::bail!("Carrier code must contain only ASCII letters and digits");
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str { &self.0 }
}

impl std::fmt::Display for CarrierCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

// ── PhoneNumber ───────────────────────────────────────────────────────────────

/// Philippine mobile / landline number stored in E.164 form.
/// Accepts "+63…" E.164 or local "09…" / "9…" formats and normalises them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhoneNumber(String);

impl PhoneNumber {
    pub fn new(raw: impl Into<String>) -> anyhow::Result<Self> {
        let s = raw.into();
        let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();

        let e164 = if s.starts_with('+') {
            // Already E.164 — keep as-is after stripping non-digits and re-adding +
            format!("+{digits}")
        } else if digits.starts_with("63") && digits.len() >= 12 {
            format!("+{digits}")
        } else if digits.starts_with('0') && digits.len() == 11 {
            format!("+63{}", &digits[1..])
        } else if digits.len() == 10 && digits.starts_with('9') {
            format!("+63{digits}")
        } else {
            anyhow::bail!("Could not parse phone number: {s}");
        };

        if e164.len() < 10 || e164.len() > 16 {
            anyhow::bail!("Phone number out of valid E.164 range: {e164}");
        }

        Ok(Self(e164))
    }

    pub fn as_str(&self) -> &str { &self.0 }
}

impl std::fmt::Display for PhoneNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

// ── WeightKg ─────────────────────────────────────────────────────────────────

/// Shipment weight in kilograms — must be positive and below 30 000 kg.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct WeightKg(f32);

impl WeightKg {
    pub const MAX: f32 = 30_000.0;

    pub fn new(kg: f32) -> anyhow::Result<Self> {
        if kg <= 0.0 {
            anyhow::bail!("Weight must be positive, got {kg}");
        }
        if kg > Self::MAX {
            anyhow::bail!("Weight {kg} kg exceeds platform maximum of {} kg", Self::MAX);
        }
        Ok(Self(kg))
    }

    pub fn value(&self) -> f32 { self.0 }
}

impl std::fmt::Display for WeightKg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.2} kg", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carrier_code_normalises_to_uppercase() {
        let c = CarrierCode::new("jnt").unwrap();
        assert_eq!(c.as_str(), "JNT");
    }

    #[test]
    fn carrier_code_rejects_too_short() {
        assert!(CarrierCode::new("X").is_err());
    }

    #[test]
    fn carrier_code_rejects_too_long() {
        assert!(CarrierCode::new("TOOLONGCODE1").is_err());
    }

    #[test]
    fn phone_normalises_local_format() {
        let p = PhoneNumber::new("09171234567").unwrap();
        assert_eq!(p.as_str(), "+639171234567");
    }

    #[test]
    fn phone_accepts_e164() {
        let p = PhoneNumber::new("+639171234567").unwrap();
        assert_eq!(p.as_str(), "+639171234567");
    }

    #[test]
    fn service_level_roundtrip() {
        let sl = ServiceLevel::from_str("same_day").unwrap();
        assert_eq!(sl.as_str(), "same_day");
        assert_eq!(sl.max_days(), 1);
    }

    #[test]
    fn weight_rejects_zero() {
        assert!(WeightKg::new(0.0).is_err());
    }

    #[test]
    fn weight_rejects_negative() {
        assert!(WeightKg::new(-5.0).is_err());
    }
}
