//! Invoice number value objects for LogisticOS billing.
//!
//! # Formats
//!
//! | Type | Format | Example |
//! |------|--------|---------|
//! | Tax invoice (merchant monthly) | `IN-{TTT}-{YYYY}-{MM}-{NNNNN}` | `IN-PH1-2026-04-00001` |
//! | Payment receipt (customer per-shipment) | `RC-{TTT}-{YYYY}-{MM}-{NNNNN}` | `RC-PH1-2026-04-00001` |
//! | COD remittance | `REM-{TTT}-{YYYY}-{MM}-{NNNNN}` | `REM-PH1-2026-04-00001` |
//! | Credit note | `CN-{TTT}-{YYYY}-{MM}-{NNNNN}` | `CN-PH1-2026-04-00001` |
//! | Wallet top-up receipt | `WR-{TTT}-{YYYY}-{MM}-{NNNNN}` | `WR-PH1-2026-04-00001` |
//! | Carrier payable | `CP-{TTT}-{YYYY}-{MM}-{NNNNN}` | `CP-PH1-2026-04-00001` |
//!
//! All sequences are per-tenant, per-month, per-type — resetting each calendar month.
//! Maximum 99,999 documents per type per tenant per month (5-digit sequence).

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

// ── InvoiceType ───────────────────────────────────────────────────────────────

/// The class of billing document, determining prefix and accounting treatment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InvoiceType {
    /// Merchant owes platform — monthly batched shipment delivery charges.
    /// Presentation label: "Tax Invoice".
    ShipmentCharges,
    /// Customer paid platform — per-shipment receipt issued at POD for B2C self-bookings.
    /// Money was already collected at booking (card preauth captured at POD).
    /// Presentation label: "Payment Receipt".
    PaymentReceipt,
    /// Platform owes merchant — COD cash collected by driver.
    CodRemittance,
    /// Cancellation refund or billing dispute resolution.
    CreditNote,
    /// Merchant wallet recharge receipt.
    WalletTopUp,
    /// Platform owes carrier — outsourced delivery charges.
    CarrierPayable,
}

impl InvoiceType {
    pub fn prefix(self) -> &'static str {
        match self {
            Self::ShipmentCharges => "IN",
            Self::PaymentReceipt  => "RC",
            Self::CodRemittance   => "REM",
            Self::CreditNote      => "CN",
            Self::WalletTopUp     => "WR",
            Self::CarrierPayable  => "CP",
        }
    }

    pub fn from_prefix(p: &str) -> Result<Self, InvoiceNumberError> {
        match p {
            "IN"  => Ok(Self::ShipmentCharges),
            "RC"  => Ok(Self::PaymentReceipt),
            "REM" => Ok(Self::CodRemittance),
            "CN"  => Ok(Self::CreditNote),
            "WR"  => Ok(Self::WalletTopUp),
            "CP"  => Ok(Self::CarrierPayable),
            _     => Err(InvoiceNumberError::UnknownPrefix(p.to_string())),
        }
    }

    /// Whether this document type represents money owed TO the platform (receivable).
    pub fn is_receivable(self) -> bool {
        matches!(self, Self::ShipmentCharges | Self::WalletTopUp | Self::PaymentReceipt)
    }

    /// Whether this document type represents money owed BY the platform (payable).
    pub fn is_payable(self) -> bool {
        matches!(self, Self::CodRemittance | Self::CarrierPayable | Self::CreditNote)
    }
}

// ── InvoiceNumber ─────────────────────────────────────────────────────────────

/// A structured, human-readable invoice document number.
///
/// # Examples
/// ```
/// use logisticos_types::invoice::{InvoiceNumber, InvoiceType};
/// use chrono::NaiveDate;
///
/// let num = InvoiceNumber::generate(
///     InvoiceType::ShipmentCharges,
///     "PH1",
///     NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
///     1,
/// ).unwrap();
/// assert_eq!(num.as_str(), "IN-PH1-2026-04-00001");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InvoiceNumber(String);

impl InvoiceNumber {
    /// Generate an invoice number from components.
    /// `sequence` must be 1..=99_999.
    pub fn generate(
        invoice_type: InvoiceType,
        tenant_code: &str,
        period: NaiveDate,
        sequence: u32,
    ) -> Result<Self, InvoiceNumberError> {
        Self::validate_tenant(tenant_code)?;
        if sequence == 0 || sequence > 99_999 {
            return Err(InvoiceNumberError::SequenceOutOfRange(sequence));
        }
        let s = format!(
            "{}-{}-{:04}-{:02}-{:05}",
            invoice_type.prefix(),
            tenant_code.to_uppercase(),
            period.year(),
            period.month(),
            sequence,
        );
        Ok(Self(s))
    }

    /// Parse an invoice number string.
    pub fn parse(raw: &str) -> Result<Self, InvoiceNumberError> {
        let parts: Vec<&str> = raw.trim().split('-').collect();
        // All types have at least 5 dash-separated segments:
        // [prefix, tenant, YYYY, MM, NNNNN]
        // REM has 3-char prefix so still 5 parts after split
        if parts.len() != 5 {
            return Err(InvoiceNumberError::InvalidFormat);
        }
        InvoiceType::from_prefix(parts[0])?;
        Self::validate_tenant(parts[1])?;
        let year: i32 = parts[2].parse().map_err(|_| InvoiceNumberError::InvalidFormat)?;
        let month: u32 = parts[3].parse().map_err(|_| InvoiceNumberError::InvalidFormat)?;
        let seq: u32 = parts[4].parse().map_err(|_| InvoiceNumberError::InvalidFormat)?;
        if !(2020..=2099).contains(&year) { return Err(InvoiceNumberError::InvalidFormat); }
        if !(1..=12).contains(&month)     { return Err(InvoiceNumberError::InvalidFormat); }
        if seq == 0 || seq > 99_999       { return Err(InvoiceNumberError::SequenceOutOfRange(seq)); }
        Ok(Self(raw.trim().to_uppercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn invoice_type(&self) -> InvoiceType {
        let prefix = self.0.split('-').next().unwrap();
        InvoiceType::from_prefix(prefix).unwrap()
    }

    pub fn tenant_code(&self) -> &str {
        self.0.split('-').nth(1).unwrap()
    }

    /// The billing period as (year, month).
    pub fn period(&self) -> (i32, u32) {
        let mut parts = self.0.split('-');
        parts.next(); // prefix
        parts.next(); // tenant
        let year:  i32 = parts.next().unwrap().parse().unwrap();
        let month: u32 = parts.next().unwrap().parse().unwrap();
        (year, month)
    }

    pub fn sequence(&self) -> u32 {
        self.0.split('-').last().unwrap().parse().unwrap()
    }

    /// Redis key used to generate the sequence counter for this document type/tenant/period.
    ///
    /// Pattern: `inv:seq:{prefix}:{tenant}:{YYYY}-{MM}`
    pub fn redis_counter_key(invoice_type: InvoiceType, tenant_code: &str, period: NaiveDate) -> String {
        format!(
            "inv:seq:{}:{}:{:04}-{:02}",
            invoice_type.prefix(),
            tenant_code.to_uppercase(),
            period.year(),
            period.month(),
        )
    }

    fn validate_tenant(code: &str) -> Result<(), InvoiceNumberError> {
        if code.len() != 3 || !code.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(InvoiceNumberError::InvalidTenantCode(code.to_string()));
        }
        Ok(())
    }
}

impl fmt::Display for InvoiceNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ── Type aliases for clarity at call sites ────────────────────────────────────

/// A COD remittance document number. Semantically identical to InvoiceNumber
/// but restricted to the `CodRemittance` type.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RemittanceNumber(pub InvoiceNumber);

impl RemittanceNumber {
    pub fn generate(tenant_code: &str, period: NaiveDate, sequence: u32) -> Result<Self, InvoiceNumberError> {
        InvoiceNumber::generate(InvoiceType::CodRemittance, tenant_code, period, sequence)
            .map(Self)
    }
    pub fn as_str(&self) -> &str { self.0.as_str() }
}

impl fmt::Display for RemittanceNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A credit note document number.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CreditNoteNumber(pub InvoiceNumber);

impl CreditNoteNumber {
    pub fn generate(tenant_code: &str, period: NaiveDate, sequence: u32) -> Result<Self, InvoiceNumberError> {
        InvoiceNumber::generate(InvoiceType::CreditNote, tenant_code, period, sequence)
            .map(Self)
    }
    pub fn as_str(&self) -> &str { self.0.as_str() }
}

impl fmt::Display for CreditNoteNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ── InvoiceNumberError ────────────────────────────────────────────────────────

#[derive(Debug, Error, PartialEq)]
pub enum InvoiceNumberError {
    #[error("Invalid invoice number format")]
    InvalidFormat,

    #[error("Unknown document prefix '{0}' — must be IN, RC, REM, CN, WR, or CP")]
    UnknownPrefix(String),

    #[error("Invalid tenant code '{0}' — must be 3 alphanumeric characters")]
    InvalidTenantCode(String),

    #[error("Sequence {0} out of range — must be 1..=99999")]
    SequenceOutOfRange(u32),
}

// ── ChargeType ────────────────────────────────────────────────────────────────

/// The type of charge on an invoice line item.
/// Determines fee calculation logic and GL account mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChargeType {
    /// Standard delivery fee based on weight and service type.
    BaseFreight,
    /// Surcharge when actual weight exceeds declared weight.
    WeightSurcharge,
    /// Surcharge when volumetric weight exceeds actual weight.
    DimensionalSurcharge,
    /// Surcharge for delivery to remote/rural zones.
    RemoteAreaSurcharge,
    /// Percentage of base freight, covering fuel costs.
    FuelSurcharge,
    /// Percentage of COD amount for cash-handling service.
    CodHandlingFee,
    /// Fee charged on delivery attempt (2nd+ attempt).
    FailedDeliveryFee,
    /// Fee for returning shipment to origin merchant.
    ReturnFee,
    /// Fee for declared-value insurance coverage.
    InsuranceFee,
    /// Import duties and taxes (international / Balikbayan).
    CustomsDuty,
    /// Daily fee for shipment held at hub beyond free storage period.
    StorageFee,
    /// Fee for customer-requested delivery reschedule.
    RescheduleFee,
    /// Ad-hoc adjustment — requires reason text.
    ManualAdjustment,
}

impl ChargeType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BaseFreight          => "base_freight",
            Self::WeightSurcharge      => "weight_surcharge",
            Self::DimensionalSurcharge => "dimensional_surcharge",
            Self::RemoteAreaSurcharge  => "remote_area_surcharge",
            Self::FuelSurcharge        => "fuel_surcharge",
            Self::CodHandlingFee       => "cod_handling_fee",
            Self::FailedDeliveryFee    => "failed_delivery_fee",
            Self::ReturnFee            => "return_fee",
            Self::InsuranceFee         => "insurance_fee",
            Self::CustomsDuty          => "customs_duty",
            Self::StorageFee           => "storage_fee",
            Self::RescheduleFee        => "reschedule_fee",
            Self::ManualAdjustment     => "manual_adjustment",
        }
    }

    /// Whether this charge type requires an AWB reference on the line item.
    pub fn requires_awb(self) -> bool {
        !matches!(self, Self::FuelSurcharge | Self::ManualAdjustment)
    }

    /// Whether this charge type can appear on a COD remittance document.
    pub fn is_remittance_charge(self) -> bool {
        matches!(self, Self::CodHandlingFee)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn apr_2026() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 4, 1).unwrap()
    }

    // ── InvoiceType ─────────────────────────────────────────────────────────

    #[test]
    fn invoice_type_prefix_round_trip() {
        for t in [
            InvoiceType::ShipmentCharges,
            InvoiceType::CodRemittance,
            InvoiceType::CreditNote,
            InvoiceType::WalletTopUp,
            InvoiceType::CarrierPayable,
        ] {
            assert_eq!(InvoiceType::from_prefix(t.prefix()).unwrap(), t);
        }
    }

    #[test]
    fn invoice_type_receivable_payable_exclusive() {
        assert!(InvoiceType::ShipmentCharges.is_receivable());
        assert!(!InvoiceType::ShipmentCharges.is_payable());
        assert!(InvoiceType::CodRemittance.is_payable());
        assert!(!InvoiceType::CodRemittance.is_receivable());
    }

    // ── InvoiceNumber generation ─────────────────────────────────────────────

    #[test]
    fn invoice_number_format_shipment_charges() {
        let n = InvoiceNumber::generate(InvoiceType::ShipmentCharges, "PH1", apr_2026(), 1).unwrap();
        assert_eq!(n.as_str(), "IN-PH1-2026-04-00001");
    }

    #[test]
    fn invoice_number_format_remittance() {
        let n = InvoiceNumber::generate(InvoiceType::CodRemittance, "SG2", apr_2026(), 42).unwrap();
        assert_eq!(n.as_str(), "REM-SG2-2026-04-00042");
    }

    #[test]
    fn invoice_number_format_credit_note() {
        let n = InvoiceNumber::generate(InvoiceType::CreditNote, "AE3", apr_2026(), 99999).unwrap();
        assert_eq!(n.as_str(), "CN-AE3-2026-04-99999");
    }

    #[test]
    fn invoice_number_rejects_seq_zero() {
        assert!(matches!(
            InvoiceNumber::generate(InvoiceType::ShipmentCharges, "PH1", apr_2026(), 0),
            Err(InvoiceNumberError::SequenceOutOfRange(0))
        ));
    }

    #[test]
    fn invoice_number_rejects_seq_overflow() {
        assert!(matches!(
            InvoiceNumber::generate(InvoiceType::ShipmentCharges, "PH1", apr_2026(), 100_000),
            Err(InvoiceNumberError::SequenceOutOfRange(100_000))
        ));
    }

    #[test]
    fn invoice_number_rejects_bad_tenant() {
        assert!(InvoiceNumber::generate(InvoiceType::ShipmentCharges, "PH", apr_2026(), 1).is_err());
        assert!(InvoiceNumber::generate(InvoiceType::ShipmentCharges, "PH12", apr_2026(), 1).is_err());
    }

    // ── InvoiceNumber parsing ────────────────────────────────────────────────

    #[test]
    fn invoice_number_parse_round_trip() {
        let original = InvoiceNumber::generate(InvoiceType::ShipmentCharges, "PH1", apr_2026(), 123).unwrap();
        let parsed   = InvoiceNumber::parse(original.as_str()).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn invoice_number_accessors() {
        let n = InvoiceNumber::generate(InvoiceType::ShipmentCharges, "PH1", apr_2026(), 7).unwrap();
        assert_eq!(n.invoice_type(), InvoiceType::ShipmentCharges);
        assert_eq!(n.tenant_code(), "PH1");
        assert_eq!(n.period(), (2026, 4));
        assert_eq!(n.sequence(), 7);
    }

    // ── Redis counter key ────────────────────────────────────────────────────

    #[test]
    fn redis_counter_key_format() {
        let key = InvoiceNumber::redis_counter_key(InvoiceType::ShipmentCharges, "PH1", apr_2026());
        assert_eq!(key, "inv:seq:IN:PH1:2026-04");
    }

    #[test]
    fn redis_counter_key_remittance() {
        let key = InvoiceNumber::redis_counter_key(InvoiceType::CodRemittance, "SG2", apr_2026());
        assert_eq!(key, "inv:seq:REM:SG2:2026-04");
    }

    // ── RemittanceNumber / CreditNoteNumber ──────────────────────────────────

    #[test]
    fn remittance_number_prefix() {
        let r = RemittanceNumber::generate("PH1", apr_2026(), 5).unwrap();
        assert!(r.as_str().starts_with("REM-"));
    }

    #[test]
    fn credit_note_prefix() {
        let cn = CreditNoteNumber::generate("PH1", apr_2026(), 3).unwrap();
        assert!(cn.as_str().starts_with("CN-"));
    }

    // ── ChargeType ───────────────────────────────────────────────────────────

    #[test]
    fn charge_type_requires_awb() {
        assert!(ChargeType::BaseFreight.requires_awb());
        assert!(ChargeType::WeightSurcharge.requires_awb());
        assert!(!ChargeType::FuelSurcharge.requires_awb());
        assert!(!ChargeType::ManualAdjustment.requires_awb());
    }

    #[test]
    fn charge_type_remittance() {
        assert!(ChargeType::CodHandlingFee.is_remittance_charge());
        assert!(!ChargeType::BaseFreight.is_remittance_charge());
    }
}
