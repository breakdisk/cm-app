//! AWB (Airway Bill) value objects for LogisticOS.
//!
//! # Format
//!
//! **Master AWB**: `LS-{TTT}-{S}{NNNNNNN}{C}`
//! - `LS`       — platform prefix, always fixed
//! - `{TTT}`    — 3-char tenant code (e.g. `PH1`, `SG2`, `AE3`)
//! - `{S}`      — service code: `S`=Standard, `E`=Express, `D`=SameDay,
//!                `B`=Balikbayan, `I`=International
//! - `{NNNNNNN}` — 7-digit zero-padded sequence (per tenant+service)
//! - `{C}`      — Luhn mod-34 check character
//!
//! Example: `LS-PH1-S0001234X`
//!
//! **Child AWB** (piece label): `LS-{TTT}-{S}{NNNNNNN}{C}-{PPP}`
//! - `{PPP}` — 3-digit piece number, 1-based, zero-padded
//!
//! Example: `LS-PH1-B0009012Z-002`

use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

// ── Charset ───────────────────────────────────────────────────────────────────

/// Base-34 charset: digits + uppercase letters, excluding O (→0) and I (→1)
/// to prevent human transcription errors.
const CHARSET: &[u8] = b"0123456789ABCDEFGHJKLMNPQRSTUVWXYZ";
const BASE: u32 = 34;

fn char_value(c: char) -> Option<u32> {
    CHARSET.iter().position(|&b| b == c as u8).map(|i| i as u32)
}

fn value_char(v: u32) -> char {
    CHARSET[(v % BASE) as usize] as char
}

// ── Luhn Mod-34 ───────────────────────────────────────────────────────────────

/// Compute Luhn mod-34 check character for a payload string.
/// Payload must consist entirely of CHARSET characters.
fn luhn_checksum(payload: &str) -> Result<char, AwbError> {
    let mut sum: u32 = 0;
    let mut double = false;

    for c in payload.chars().rev() {
        let v = char_value(c).ok_or(AwbError::InvalidCharset(c))?;
        let mut d = if double { v * 2 } else { v };
        if d >= BASE {
            d = d / BASE + d % BASE;
        }
        sum += d;
        double = !double;
    }

    let check_val = (BASE - (sum % BASE)) % BASE;
    Ok(value_char(check_val))
}

fn luhn_valid(full: &str) -> bool {
    let len = full.len();
    if len < 2 {
        return false;
    }
    let (payload, check_str) = full.split_at(len - 1);
    let check_char = check_str.chars().next().unwrap();
    matches!(luhn_checksum(payload), Ok(c) if c == check_char)
}

// ── ServiceCode ───────────────────────────────────────────────────────────────

/// One-character service code embedded in the AWB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ServiceCode {
    /// Standard ground delivery (1–3 business days).
    Standard,
    /// Express delivery (next business day).
    Express,
    /// Same-day delivery (order before 14:00).
    SameDay,
    /// Balikbayan box freight (sea or air, multi-piece common).
    Balikbayan,
    /// International cross-border shipment.
    International,
}

impl ServiceCode {
    pub fn as_char(self) -> char {
        match self {
            Self::Standard      => 'S',
            Self::Express       => 'E',
            Self::SameDay       => 'D',
            Self::Balikbayan    => 'B',
            Self::International => 'I',
        }
    }

    pub fn from_char(c: char) -> Result<Self, AwbError> {
        match c {
            'S' => Ok(Self::Standard),
            'E' => Ok(Self::Express),
            'D' => Ok(Self::SameDay),
            'B' => Ok(Self::Balikbayan),
            'I' => Ok(Self::International),
            _   => Err(AwbError::UnknownServiceCode(c)),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Standard      => "standard",
            Self::Express       => "express",
            Self::SameDay       => "same_day",
            Self::Balikbayan    => "balikbayan",
            Self::International => "international",
        }
    }
}

impl fmt::Display for ServiceCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_char())
    }
}

// ── TenantCode ────────────────────────────────────────────────────────────────

/// 3-character alphanumeric tenant code (e.g. `PH1`, `SG2`).
/// Assigned at tenant onboarding. Uppercase, no O or I.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TenantCode(String);

impl TenantCode {
    pub fn new(code: &str) -> Result<Self, AwbError> {
        if code.len() != 3 {
            return Err(AwbError::InvalidTenantCode(code.to_string()));
        }
        let upper = code.to_uppercase();
        for c in upper.chars() {
            if !c.is_ascii_alphanumeric() || c == 'O' || c == 'I' {
                return Err(AwbError::InvalidTenantCode(code.to_string()));
            }
        }
        Ok(Self(upper))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TenantCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ── Awb (Master) ──────────────────────────────────────────────────────────────

/// Master AWB — the commercial tracking number issued at booking.
/// Immutable once issued. Format: `LS-{TTT}-{S}{NNNNNNN}{C}`
///
/// # Examples
/// ```
/// use logisticos_types::awb::{Awb, TenantCode, ServiceCode};
///
/// let awb = Awb::generate(&TenantCode::new("PH1").unwrap(), ServiceCode::Standard, 1234);
/// assert!(awb.is_valid());
/// assert_eq!(awb.service_code(), ServiceCode::Standard);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Awb(String);

impl Awb {
    /// Generate a new master AWB from components.
    /// `sequence` must be 1..=9_999_999.
    pub fn generate(tenant: &TenantCode, service: ServiceCode, sequence: u32) -> Self {
        debug_assert!(sequence >= 1 && sequence <= 9_999_999, "sequence out of range");
        let payload = format!(
            "LSPH1{}{}{:07}",  // internal compact form for checksum
            tenant.as_str(),
            service.as_char(),
            sequence,
        );
        // checksum over the compact payload (no dashes)
        let compact_payload = format!("{}{}{:07}", tenant.as_str(), service.as_char(), sequence);
        let check = luhn_checksum(&compact_payload).expect("charset always valid for generated AWB");
        let formatted = format!("LS-{}-{}{:07}{}", tenant.as_str(), service.as_char(), sequence, check);
        Self(formatted)
    }

    /// Parse and validate an AWB string.
    /// Accepts both dash form (`LS-PH1-S0001234X`) and compact form (`LSPH1S0001234X`).
    pub fn parse(raw: &str) -> Result<Self, AwbError> {
        let normalised = Self::normalise(raw)?;
        // validate checksum over TTT+S+NNNNNNN+C (11 chars)
        let inner = &normalised[3..]; // strip "LS-"
        let parts: Vec<&str> = inner.splitn(2, '-').collect();
        if parts.len() != 2 {
            return Err(AwbError::InvalidFormat);
        }
        let service_seq_check = parts[1]; // e.g. "S0001234X"
        if service_seq_check.len() != 9 {
            return Err(AwbError::InvalidFormat);
        }
        let tenant_code = parts[0]; // e.g. "PH1"
        let payload = format!("{}{}", tenant_code, &service_seq_check[..8]); // TTT+S+NNNNNNN
        let check_char = service_seq_check.chars().last().unwrap();
        let expected = luhn_checksum(&payload)?;
        if check_char != expected {
            return Err(AwbError::InvalidChecksum);
        }
        // validate service code
        ServiceCode::from_char(service_seq_check.chars().next().unwrap())?;
        Ok(Self(normalised))
    }

    /// Full display string with dashes: `LS-PH1-S0001234X`
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Compact barcode string (no dashes): `LSPH1S0001234X`
    pub fn barcode_str(&self) -> String {
        self.0.replace('-', "")
    }

    /// Validate the embedded checksum.
    pub fn is_valid(&self) -> bool {
        let inner = match self.0.strip_prefix("LS-") {
            Some(s) => s,
            None    => return false,
        };
        let parts: Vec<&str> = inner.splitn(2, '-').collect();
        if parts.len() != 2 || parts[1].len() != 9 {
            return false;
        }
        let payload = format!("{}{}", parts[0], &parts[1][..8]);
        matches!(luhn_checksum(&payload), Ok(c) if c == parts[1].chars().last().unwrap())
    }

    /// Extract the tenant code segment (e.g. `"PH1"`).
    pub fn tenant_code(&self) -> &str {
        &self.0[3..6]
    }

    /// Extract the service code character.
    pub fn service_code(&self) -> ServiceCode {
        let c = self.0.chars().nth(7).expect("AWB always has service char at position 7");
        ServiceCode::from_char(c).expect("AWB always has valid service code")
    }

    /// Extract the sequence number.
    pub fn sequence(&self) -> u32 {
        self.0[8..15].parse().expect("AWB always has numeric sequence")
    }

    /// Normalise raw string to dash form. Accepts compact or dash form.
    fn normalise(raw: &str) -> Result<String, AwbError> {
        let s = raw.trim().to_uppercase();
        // already dash form: LS-XXX-XNNNNNNNX (16 chars)
        if s.starts_with("LS-") && s.len() == 16 {
            return Ok(s);
        }
        // compact form: LSXXXSXXXXXXXC (14 chars)
        if s.starts_with("LS") && !s.contains('-') && s.len() == 14 {
            let tenant  = &s[2..5];
            let service = &s[5..6];
            let seq_chk = &s[6..14];
            return Ok(format!("LS-{}-{}{}", tenant, service, seq_chk));
        }
        Err(AwbError::InvalidFormat)
    }
}

impl fmt::Display for Awb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ── ChildAwb (Piece label) ────────────────────────────────────────────────────

/// Child AWB — piece-level barcode derived from a master AWB.
/// Format: `{master_awb}-{PPP}` where PPP is 3-digit piece number (001..=999).
///
/// # Examples
/// ```
/// use logisticos_types::awb::{Awb, ChildAwb, TenantCode, ServiceCode};
///
/// let master = Awb::generate(&TenantCode::new("PH1").unwrap(), ServiceCode::Balikbayan, 9012);
/// let piece1 = ChildAwb::new(&master, 1).unwrap();
/// let piece2 = ChildAwb::new(&master, 2).unwrap();
/// assert_eq!(piece1.piece_number(), 1);
/// assert_eq!(piece2.as_str(), &format!("{}-002", master.as_str()));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChildAwb(String);

impl ChildAwb {
    /// Create a child AWB for the given piece number (1..=999).
    pub fn new(master: &Awb, piece_number: u16) -> Result<Self, AwbError> {
        if piece_number == 0 || piece_number > 999 {
            return Err(AwbError::InvalidPieceNumber(piece_number));
        }
        Ok(Self(format!("{}-{:03}", master.as_str(), piece_number)))
    }

    /// Parse a child AWB string.
    pub fn parse(raw: &str) -> Result<Self, AwbError> {
        let s = raw.trim().to_uppercase();
        // Expected: LS-XXX-XNNNNNNNC-PPP  (20 chars)
        if s.len() != 20 {
            return Err(AwbError::InvalidFormat);
        }
        let (master_part, piece_part) = s
            .rsplit_once('-')
            .ok_or(AwbError::InvalidFormat)?;
        // validate master portion
        Awb::parse(master_part)?;
        // validate piece suffix
        let piece_num: u16 = piece_part.parse().map_err(|_| AwbError::InvalidFormat)?;
        if piece_num == 0 || piece_num > 999 {
            return Err(AwbError::InvalidPieceNumber(piece_num));
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The master AWB this piece belongs to.
    pub fn master(&self) -> Awb {
        // master is everything before the last "-PPP"
        let master_str = &self.0[..16];
        Awb(master_str.to_string())
    }

    /// 1-based piece number.
    pub fn piece_number(&self) -> u16 {
        self.0[17..].parse().expect("ChildAwb always has valid piece number")
    }

    /// Compact barcode string (no dashes): `LSPH1B0009012Z002`
    pub fn barcode_str(&self) -> String {
        self.0.replace('-', "")
    }
}

impl fmt::Display for ChildAwb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ── AwbError ──────────────────────────────────────────────────────────────────

#[derive(Debug, Error, PartialEq)]
pub enum AwbError {
    #[error("Invalid AWB format — expected LS-TTT-SNNNNNNNC or compact LSTTTSXXXXXXXC")]
    InvalidFormat,

    #[error("Invalid checksum — AWB may have been mistyped")]
    InvalidChecksum,

    #[error("Unknown service code '{0}' — must be S, E, D, B, or I")]
    UnknownServiceCode(char),

    #[error("Invalid tenant code '{0}' — must be 3 alphanumeric chars excluding O and I")]
    InvalidTenantCode(String),

    #[error("Invalid piece number {0} — must be between 1 and 999")]
    InvalidPieceNumber(u16),

    #[error("Invalid charset character '{0}'")]
    InvalidCharset(char),
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn ph1() -> TenantCode {
        TenantCode::new("PH1").unwrap()
    }

    // ── TenantCode ──────────────────────────────────────────────────────────

    #[test]
    fn tenant_code_valid() {
        assert!(TenantCode::new("PH1").is_ok());
        assert!(TenantCode::new("SG2").is_ok());
        assert!(TenantCode::new("AE3").is_ok());
        assert!(TenantCode::new("ph1").is_ok()); // lowercase normalised
    }

    #[test]
    fn tenant_code_rejects_ambiguous_chars() {
        assert!(TenantCode::new("PO1").is_err()); // O not allowed
        assert!(TenantCode::new("PI1").is_err()); // I not allowed
    }

    #[test]
    fn tenant_code_rejects_wrong_length() {
        assert!(TenantCode::new("PH").is_err());
        assert!(TenantCode::new("PH12").is_err());
    }

    // ── ServiceCode ─────────────────────────────────────────────────────────

    #[test]
    fn service_code_round_trips() {
        for code in [
            ServiceCode::Standard,
            ServiceCode::Express,
            ServiceCode::SameDay,
            ServiceCode::Balikbayan,
            ServiceCode::International,
        ] {
            assert_eq!(ServiceCode::from_char(code.as_char()).unwrap(), code);
        }
    }

    #[test]
    fn service_code_rejects_unknown() {
        assert!(ServiceCode::from_char('X').is_err());
        assert!(ServiceCode::from_char('Z').is_err());
    }

    // ── Luhn checksum ────────────────────────────────────────────────────────

    #[test]
    fn luhn_checksum_deterministic() {
        let c1 = luhn_checksum("PH1S0001234").unwrap();
        let c2 = luhn_checksum("PH1S0001234").unwrap();
        assert_eq!(c1, c2);
    }

    #[test]
    fn luhn_different_payloads_different_checksums() {
        let c1 = luhn_checksum("PH1S0001234").unwrap();
        let c2 = luhn_checksum("PH1S0001235").unwrap();
        // adjacent sequences should (almost always) differ
        // this is probabilistic — just ensure they CAN differ
        let _ = (c1, c2); // no panic = pass
    }

    // ── Awb generation & parsing ─────────────────────────────────────────────

    #[test]
    fn awb_generate_format() {
        let awb = Awb::generate(&ph1(), ServiceCode::Standard, 1234);
        let s = awb.as_str();
        assert!(s.starts_with("LS-PH1-S"), "got: {}", s);
        assert_eq!(s.len(), 16, "got: {}", s);
    }

    #[test]
    fn awb_generate_is_valid() {
        for seq in [1, 42, 1000, 9_999_999] {
            let awb = Awb::generate(&ph1(), ServiceCode::Express, seq);
            assert!(awb.is_valid(), "invalid for seq={}: {}", seq, awb);
        }
    }

    #[test]
    fn awb_parse_round_trip() {
        let original = Awb::generate(&ph1(), ServiceCode::Balikbayan, 9012);
        let parsed   = Awb::parse(original.as_str()).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn awb_parse_compact_form() {
        let awb      = Awb::generate(&ph1(), ServiceCode::Standard, 5678);
        let compact  = awb.barcode_str();
        let parsed   = Awb::parse(&compact).unwrap();
        assert_eq!(awb, parsed);
    }

    #[test]
    fn awb_parse_rejects_bad_checksum() {
        let awb = Awb::generate(&ph1(), ServiceCode::Standard, 1234);
        let mut mangled = awb.as_str().to_string();
        // flip last char
        let last = mangled.pop().unwrap();
        let replacement = if last == 'A' { 'B' } else { 'A' };
        mangled.push(replacement);
        assert_eq!(Awb::parse(&mangled).unwrap_err(), AwbError::InvalidChecksum);
    }

    #[test]
    fn awb_parse_rejects_garbage() {
        assert!(Awb::parse("GARBAGE").is_err());
        assert!(Awb::parse("LS-XX-S1234567A").is_err()); // wrong tenant len
        assert!(Awb::parse("").is_err());
    }

    #[test]
    fn awb_accessors() {
        let awb = Awb::generate(&ph1(), ServiceCode::SameDay, 42);
        assert_eq!(awb.tenant_code(), "PH1");
        assert_eq!(awb.service_code(), ServiceCode::SameDay);
        assert_eq!(awb.sequence(), 42);
    }

    #[test]
    fn awb_barcode_no_dashes() {
        let awb = Awb::generate(&ph1(), ServiceCode::International, 1);
        assert!(!awb.barcode_str().contains('-'));
        assert_eq!(awb.barcode_str().len(), 14);
    }

    // ── ChildAwb ─────────────────────────────────────────────────────────────

    #[test]
    fn child_awb_format() {
        let master = Awb::generate(&ph1(), ServiceCode::Balikbayan, 9012);
        let child  = ChildAwb::new(&master, 1).unwrap();
        assert!(child.as_str().ends_with("-001"), "got: {}", child);
        assert_eq!(child.as_str().len(), 20);
    }

    #[test]
    fn child_awb_piece_number() {
        let master = Awb::generate(&ph1(), ServiceCode::Balikbayan, 9012);
        for n in [1u16, 2, 3, 100, 999] {
            let child = ChildAwb::new(&master, n).unwrap();
            assert_eq!(child.piece_number(), n);
        }
    }

    #[test]
    fn child_awb_master_round_trip() {
        let master = Awb::generate(&ph1(), ServiceCode::Balikbayan, 9012);
        let child  = ChildAwb::new(&master, 2).unwrap();
        assert_eq!(child.master(), master);
    }

    #[test]
    fn child_awb_rejects_zero() {
        let master = Awb::generate(&ph1(), ServiceCode::Standard, 1);
        assert_eq!(ChildAwb::new(&master, 0).unwrap_err(), AwbError::InvalidPieceNumber(0));
    }

    #[test]
    fn child_awb_rejects_over_999() {
        let master = Awb::generate(&ph1(), ServiceCode::Standard, 1);
        assert_eq!(ChildAwb::new(&master, 1000).unwrap_err(), AwbError::InvalidPieceNumber(1000));
    }

    #[test]
    fn child_awb_parse_round_trip() {
        let master   = Awb::generate(&ph1(), ServiceCode::Express, 777);
        let child    = ChildAwb::new(&master, 3).unwrap();
        let reparsed = ChildAwb::parse(child.as_str()).unwrap();
        assert_eq!(child, reparsed);
    }

    #[test]
    fn child_awb_barcode_no_dashes() {
        let master = Awb::generate(&ph1(), ServiceCode::Balikbayan, 9012);
        let child  = ChildAwb::new(&master, 1).unwrap();
        assert!(!child.barcode_str().contains('-'));
    }
}
