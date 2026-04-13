//! Invoice and related billing entities for the payments service.
//!
//! # Document hierarchy
//!
//! ```text
//! Invoice (one per billing cycle or event)
//! └── InvoiceLineItem (one per AWB/piece or per fee type)
//!     └── ChargeType (enum — determines GL account mapping)
//!
//! BillingPeriod — defines the start/end window for an invoice cycle
//! InvoiceAdjustment — credit or debit appended to a finalized invoice
//! ```
//!
//! Billing is ALWAYS at the AWB/piece level; pallets and containers
//! are invisible to merchants on invoices.

use chrono::{DateTime, Duration, NaiveDate, Utc};
use logisticos_types::{
    awb::Awb,
    invoice::{ChargeType, InvoiceNumber, InvoiceType},
    Currency, CustomerId, InvoiceId, LineItemId, MerchantId, Money, TenantId,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── BillingPeriod ─────────────────────────────────────────────────────────────

/// Calendar window for a billing cycle (typically one calendar month).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BillingPeriod {
    pub start: NaiveDate,
    pub end:   NaiveDate,   // inclusive
}

impl BillingPeriod {
    /// Construct a monthly billing period for the given year/month.
    pub fn monthly(year: i32, month: u32) -> Self {
        use chrono::NaiveDate;
        let start = NaiveDate::from_ymd_opt(year, month, 1)
            .expect("invalid month");
        // Last day of the month
        let end = if month == 12 {
            NaiveDate::from_ymd_opt(year + 1, 1, 1).unwrap()
        } else {
            NaiveDate::from_ymd_opt(year, month + 1, 1).unwrap()
        }
        .pred_opt()
        .unwrap();
        Self { start, end }
    }

    /// Single-day billing period — used for per-shipment payment receipts.
    pub fn single_day(date: NaiveDate) -> Self {
        Self { start: date, end: date }
    }

    /// Whether a date falls within this billing period.
    pub fn contains(&self, date: NaiveDate) -> bool {
        date >= self.start && date <= self.end
    }
}

// ── InvoiceStatus ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceStatus {
    /// Still accumulating charges — not yet sent to merchant.
    Draft,
    /// Sent to merchant — payment expected by `due_at`.
    Issued,
    /// Fully paid.
    Paid,
    /// Payment overdue (`due_at` has passed, still unpaid).
    Overdue,
    /// Merchant raised a dispute — on hold pending resolution.
    Disputed,
    /// Voided (e.g. replaced by a corrected invoice).
    Cancelled,
}

// ── InvoiceLineItem ───────────────────────────────────────────────────────────

/// A single billable charge on an invoice.
///
/// When `charge_type.requires_awb()` is true, `awb` must be populated.
/// For document-level charges (FuelSurcharge, ManualAdjustment), `awb` is None.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceLineItem {
    pub id:          LineItemId,
    pub charge_type: ChargeType,
    /// AWB this charge relates to — None for document-level charges.
    pub awb:         Option<Awb>,
    pub description: String,
    pub quantity:    u32,
    pub unit_price:  Money,
    /// Optional discount applied to this line.
    pub discount:    Option<Money>,
    /// Reason text — required when `charge_type == ManualAdjustment`.
    pub reason:      Option<String>,
}

impl InvoiceLineItem {
    pub fn for_awb(
        charge_type: ChargeType,
        awb:         Awb,
        description: String,
        quantity:    u32,
        unit_price:  Money,
    ) -> Self {
        Self {
            id: LineItemId::new(),
            charge_type,
            awb: Some(awb),
            description,
            quantity,
            unit_price,
            discount: None,
            reason: None,
        }
    }

    pub fn document_level(
        charge_type: ChargeType,
        description: String,
        unit_price:  Money,
    ) -> Self {
        Self {
            id: LineItemId::new(),
            charge_type,
            awb: None,
            description,
            quantity: 1,
            unit_price,
            discount: None,
            reason: None,
        }
    }

    /// Gross amount before discount (quantity × unit_price).
    pub fn gross(&self) -> Money {
        Money::new(self.unit_price.amount * self.quantity as i64, self.unit_price.currency)
    }

    /// Net amount after discount.
    pub fn net(&self) -> Money {
        let gross = self.gross();
        if let Some(disc) = &self.discount {
            Money::new((gross.amount - disc.amount).max(0), gross.currency)
        } else {
            gross
        }
    }
}

// ── InvoiceAdjustment ─────────────────────────────────────────────────────────

/// A post-finalisation adjustment (credit or debit) appended to an issued invoice.
///
/// Adjustments do NOT reopen the invoice — they are additive records that feed
/// into the next settlement cycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceAdjustment {
    pub id:           Uuid,
    pub invoice_id:   InvoiceId,
    pub charge_type:  ChargeType,
    /// Positive = additional charge; negative = credit to merchant.
    pub amount:       Money,
    pub reason:       String,
    /// AWB that triggered the adjustment (e.g. weight discrepancy).
    pub awb:          Option<Awb>,
    pub created_by:   Uuid,
    pub created_at:   DateTime<Utc>,
}

impl InvoiceAdjustment {
    pub fn is_credit(&self) -> bool {
        self.amount.amount < 0
    }
}

// ── Invoice ───────────────────────────────────────────────────────────────────

/// A billing document issued to a merchant (B2B) or to a customer (B2C PaymentReceipt).
///
/// The `invoice_number` field uses the structured `InvoiceNumber` value object
/// from `logisticos_types::invoice` — format: `IN-PH1-2026-04-00001` / `RC-PH1-2026-04-00001`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invoice {
    pub id:             InvoiceId,
    /// Structured document number (replaces raw String in earlier version).
    pub invoice_number: InvoiceNumber,
    pub invoice_type:   InvoiceType,
    pub tenant_id:      TenantId,
    pub merchant_id:    MerchantId,
    /// Set on PaymentReceipt invoices — the B2C customer who paid.
    /// NULL on all merchant/B2B invoices.
    pub customer_id:    Option<CustomerId>,
    /// Billing window covered by this invoice.
    pub billing_period: BillingPeriod,
    pub line_items:     Vec<InvoiceLineItem>,
    /// Post-issue adjustments (weight discrepancies, manual credits, etc.).
    pub adjustments:    Vec<InvoiceAdjustment>,
    pub status:         InvoiceStatus,
    pub currency:       Currency,
    pub issued_at:      DateTime<Utc>,
    pub due_at:         DateTime<Utc>,
    pub paid_at:        Option<DateTime<Utc>>,
    pub created_at:     DateTime<Utc>,
    pub updated_at:     DateTime<Utc>,
}

impl Invoice {
    pub fn new(
        invoice_number: InvoiceNumber,
        invoice_type:   InvoiceType,
        tenant_id:      TenantId,
        merchant_id:    MerchantId,
        customer_id:    Option<CustomerId>,
        billing_period: BillingPeriod,
        currency:       Currency,
    ) -> Self {
        let now = Utc::now();
        Self {
            id:             InvoiceId::new(),
            invoice_number,
            invoice_type,
            tenant_id,
            merchant_id,
            customer_id,
            billing_period,
            line_items:     Vec::new(),
            adjustments:    Vec::new(),
            status:         InvoiceStatus::Draft,
            currency,
            issued_at:      now,
            due_at:         now + Duration::days(15), // Net-15 default
            paid_at:        None,
            created_at:     now,
            updated_at:     now,
        }
    }

    /// Add a line item. Only allowed while invoice is in Draft.
    pub fn add_line_item(&mut self, item: InvoiceLineItem) -> Result<(), InvoiceError> {
        if self.status != InvoiceStatus::Draft {
            return Err(InvoiceError::NotDraft);
        }
        self.line_items.push(item);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Append a post-issue adjustment (weight discrepancy, manual credit, etc.).
    /// Allowed on Issued or Overdue invoices.
    pub fn add_adjustment(&mut self, adj: InvoiceAdjustment) -> Result<(), InvoiceError> {
        if !matches!(self.status, InvoiceStatus::Issued | InvoiceStatus::Overdue) {
            return Err(InvoiceError::CannotAdjust(format!("{:?}", self.status)));
        }
        self.adjustments.push(adj);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Finalise the invoice — transition Draft → Issued.
    /// Business rule: must have at least one line item.
    pub fn issue(&mut self) -> Result<(), InvoiceError> {
        if self.status != InvoiceStatus::Draft {
            return Err(InvoiceError::NotDraft);
        }
        if self.line_items.is_empty() {
            return Err(InvoiceError::Empty);
        }
        self.status     = InvoiceStatus::Issued;
        self.issued_at  = Utc::now();
        self.updated_at = self.issued_at;
        Ok(())
    }

    /// One-shot Draft → Issued → Paid for receipts where money was already
    /// captured before the document was generated (e.g. customer-app preauth
    /// captured at POD). Restricted to PaymentReceipt — monthly invoices must
    /// still go through the explicit `issue()` then `mark_paid()` flow.
    pub fn issue_and_capture(&mut self) -> Result<(), InvoiceError> {
        if self.invoice_type != InvoiceType::PaymentReceipt {
            return Err(InvoiceError::CannotAdjust(
                "issue_and_capture is only valid for PaymentReceipt".into(),
            ));
        }
        self.issue()?;
        self.mark_paid()?;
        Ok(())
    }

    /// Mark as paid.
    pub fn mark_paid(&mut self) -> Result<(), InvoiceError> {
        if !matches!(self.status, InvoiceStatus::Issued | InvoiceStatus::Overdue) {
            return Err(InvoiceError::CannotPay(format!("{:?}", self.status)));
        }
        let now = Utc::now();
        self.status     = InvoiceStatus::Paid;
        self.paid_at    = Some(now);
        self.updated_at = now;
        Ok(())
    }

    /// Transition Issued → Overdue if past due_at and not yet paid.
    pub fn check_overdue(&mut self) {
        if self.status == InvoiceStatus::Issued && Utc::now() > self.due_at {
            self.status     = InvoiceStatus::Overdue;
            self.updated_at = Utc::now();
        }
    }

    /// Business rule: cannot cancel a paid invoice.
    pub fn cancel(&mut self) -> Result<(), InvoiceError> {
        if self.status == InvoiceStatus::Paid {
            return Err(InvoiceError::CannotCancelPaid);
        }
        self.status     = InvoiceStatus::Cancelled;
        self.updated_at = Utc::now();
        Ok(())
    }

    // ── Calculation helpers ───────────────────────────────────────────────────

    /// Sum of all line item net amounts.
    pub fn subtotal(&self) -> Money {
        let total = self.line_items.iter().fold(0i64, |acc, item| acc + item.net().amount);
        Money::new(total, self.currency)
    }

    /// Sum of all adjustments (positive = additional charge; negative = credit).
    pub fn adjustments_total(&self) -> Money {
        let total = self.adjustments.iter().fold(0i64, |acc, adj| acc + adj.amount.amount);
        Money::new(total, self.currency)
    }

    /// 12% VAT (Philippines standard rate) applied to subtotal + adjustments.
    pub fn vat_amount(&self) -> Money {
        let taxable = self.subtotal().amount + self.adjustments_total().amount;
        Money::new((taxable as f64 * 0.12).round() as i64, self.currency)
    }

    /// Grand total: subtotal + adjustments + VAT.
    pub fn total_due(&self) -> Money {
        let base = self.subtotal().amount + self.adjustments_total().amount;
        Money::new(base + self.vat_amount().amount, self.currency)
    }

    /// Number of distinct AWBs on this invoice.
    pub fn awb_count(&self) -> usize {
        let mut seen = std::collections::HashSet::new();
        for item in &self.line_items {
            if let Some(awb) = &item.awb {
                seen.insert(awb.as_str().to_string());
            }
        }
        seen.len()
    }
}

// ── InvoiceError ──────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum InvoiceError {
    #[error("Invoice must be in Draft status to add line items")]
    NotDraft,

    #[error("Invoice is empty — must have at least one line item before issuing")]
    Empty,

    #[error("Cannot adjust invoice in status {0}")]
    CannotAdjust(String),

    #[error("Cannot mark invoice as paid from status {0}")]
    CannotPay(String),

    #[error("Cannot cancel a paid invoice")]
    CannotCancelPaid,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use logisticos_types::{awb::{Awb, ServiceCode, TenantCode}, invoice::InvoiceType, TenantId, MerchantId, Currency};

    fn make_invoice() -> Invoice {
        let tenant_code = "PH1";
        let period_date = NaiveDate::from_ymd_opt(2026, 4, 1).unwrap();
        let inv_number  = InvoiceNumber::generate(
            InvoiceType::ShipmentCharges, tenant_code, period_date, 1
        ).unwrap();
        Invoice::new(
            inv_number,
            InvoiceType::ShipmentCharges,
            TenantId::new(),
            MerchantId::new(),
            None,
            BillingPeriod::monthly(2026, 4),
            Currency::PHP,
        )
    }

    fn make_awb() -> Awb {
        let t = TenantCode::new("PH1").unwrap();
        Awb::generate(&t, ServiceCode::Standard, 1)
    }

    fn php(cents: i64) -> Money {
        Money::new(cents, Currency::PHP)
    }

    // ── BillingPeriod ─────────────────────────────────────────────────────────

    #[test]
    fn billing_period_monthly_april() {
        let p = BillingPeriod::monthly(2026, 4);
        assert_eq!(p.start, NaiveDate::from_ymd_opt(2026, 4, 1).unwrap());
        assert_eq!(p.end,   NaiveDate::from_ymd_opt(2026, 4, 30).unwrap());
    }

    #[test]
    fn billing_period_monthly_december() {
        let p = BillingPeriod::monthly(2026, 12);
        assert_eq!(p.end, NaiveDate::from_ymd_opt(2026, 12, 31).unwrap());
    }

    #[test]
    fn billing_period_contains() {
        let p = BillingPeriod::monthly(2026, 4);
        assert!(p.contains(NaiveDate::from_ymd_opt(2026, 4, 15).unwrap()));
        assert!(!p.contains(NaiveDate::from_ymd_opt(2026, 5, 1).unwrap()));
    }

    // ── InvoiceLineItem ───────────────────────────────────────────────────────

    #[test]
    fn line_item_net_with_discount() {
        let item = InvoiceLineItem {
            id:          LineItemId::new(),
            charge_type: ChargeType::BaseFreight,
            awb:         Some(make_awb()),
            description: "Base freight".into(),
            quantity:    1,
            unit_price:  php(8500),
            discount:    Some(php(500)),
            reason:      None,
        };
        assert_eq!(item.gross(), php(8500));
        assert_eq!(item.net(),   php(8000));
    }

    #[test]
    fn line_item_gross_quantity() {
        let item = InvoiceLineItem::for_awb(
            ChargeType::BaseFreight,
            make_awb(),
            "Freight".into(),
            3,
            php(8500),
        );
        assert_eq!(item.gross(), php(25500));
        assert_eq!(item.net(),   php(25500));
    }

    // ── Invoice lifecycle ─────────────────────────────────────────────────────

    #[test]
    fn invoice_issue_and_total() {
        let mut inv = make_invoice();
        inv.add_line_item(InvoiceLineItem::for_awb(
            ChargeType::BaseFreight,
            make_awb(),
            "Base freight".into(),
            1,
            php(8500),
        )).unwrap();
        inv.issue().unwrap();
        assert_eq!(inv.status, InvoiceStatus::Issued);
        // subtotal=8500, vat=1020 (12%), total=9520
        assert_eq!(inv.subtotal(),  php(8500));
        assert_eq!(inv.vat_amount(), php(1020));
        assert_eq!(inv.total_due(), php(9520));
    }

    #[test]
    fn cannot_issue_empty_invoice() {
        let mut inv = make_invoice();
        assert_eq!(inv.issue().unwrap_err(), InvoiceError::Empty);
    }

    #[test]
    fn cannot_add_line_item_after_issue() {
        let mut inv = make_invoice();
        inv.add_line_item(InvoiceLineItem::for_awb(
            ChargeType::BaseFreight,
            make_awb(),
            "Freight".into(),
            1,
            php(8500),
        )).unwrap();
        inv.issue().unwrap();
        let err = inv.add_line_item(InvoiceLineItem::for_awb(
            ChargeType::FuelSurcharge,
            make_awb(),
            "Fuel".into(),
            1,
            php(500),
        )).unwrap_err();
        assert_eq!(err, InvoiceError::NotDraft);
    }

    #[test]
    fn adjustment_adds_to_total() {
        let mut inv = make_invoice();
        inv.add_line_item(InvoiceLineItem::for_awb(
            ChargeType::BaseFreight,
            make_awb(),
            "Freight".into(),
            1,
            php(8500),
        )).unwrap();
        inv.issue().unwrap();

        let adj = InvoiceAdjustment {
            id:          uuid::Uuid::new_v4(),
            invoice_id:  inv.id.clone(),
            charge_type: ChargeType::WeightSurcharge,
            amount:      php(1000), // additional charge
            reason:      "Weight discrepancy: 1.2kg vs declared 0.5kg".into(),
            awb:         Some(make_awb()),
            created_by:  uuid::Uuid::new_v4(),
            created_at:  Utc::now(),
        };
        inv.add_adjustment(adj).unwrap();

        // subtotal=8500 adj=1000, vat=12% of 9500=1140, total=10640
        assert_eq!(inv.adjustments_total(), php(1000));
        assert_eq!(inv.vat_amount(),        php(1140));
        assert_eq!(inv.total_due(),         php(10640));
    }

    #[test]
    fn cannot_cancel_paid_invoice() {
        let mut inv = make_invoice();
        inv.add_line_item(InvoiceLineItem::for_awb(
            ChargeType::BaseFreight, make_awb(), "Freight".into(), 1, php(8500),
        )).unwrap();
        inv.issue().unwrap();
        inv.mark_paid().unwrap();
        assert_eq!(inv.cancel().unwrap_err(), InvoiceError::CannotCancelPaid);
    }

    #[test]
    fn awb_count_deduplicates() {
        let mut inv = make_invoice();
        let awb = make_awb();
        // Two line items for the same AWB (base freight + fuel surcharge)
        inv.add_line_item(InvoiceLineItem::for_awb(
            ChargeType::BaseFreight, awb.clone(), "Freight".into(), 1, php(8500),
        )).unwrap();
        inv.add_line_item(InvoiceLineItem::for_awb(
            ChargeType::FuelSurcharge, awb.clone(), "Fuel".into(), 1, php(500),
        )).unwrap();
        // Plus a document-level fuel surcharge with no AWB
        inv.add_line_item(InvoiceLineItem::document_level(
            ChargeType::FuelSurcharge, "Fleet fuel surcharge".into(), php(200),
        )).unwrap();
        assert_eq!(inv.awb_count(), 1);
        assert_eq!(inv.line_items.len(), 3);
    }
}
