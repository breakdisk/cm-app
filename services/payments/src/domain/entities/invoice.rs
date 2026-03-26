use logisticos_types::{InvoiceId, MerchantId, Money, Currency};
use chrono::{DateTime, Utc, Duration};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invoice {
    pub id: InvoiceId,
    pub merchant_id: MerchantId,
    pub line_items: Vec<InvoiceLineItem>,
    pub status: InvoiceStatus,
    pub issued_at: DateTime<Utc>,
    pub due_at: DateTime<Utc>,
    pub paid_at: Option<DateTime<Utc>>,
    pub currency: Currency,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceLineItem {
    pub description: String,
    pub quantity: u32,
    pub unit_price: Money,
    pub discount: Option<Money>,
}

impl InvoiceLineItem {
    pub fn total(&self) -> Money {
        let gross = Money::new(self.unit_price.amount * self.quantity as i64, self.unit_price.currency);
        if let Some(discount) = &self.discount {
            Money::new(gross.amount - discount.amount, gross.currency)
        } else {
            gross
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum InvoiceStatus {
    Draft,
    Issued,
    Paid,
    Overdue,
    Disputed,
    Cancelled,
}

impl Invoice {
    pub fn subtotal(&self) -> Money {
        let total = self.line_items.iter().fold(0i64, |acc, item| acc + item.total().amount);
        Money::new(total, self.currency)
    }

    /// Business rule: 12% VAT (Philippines)
    pub fn vat_amount(&self) -> Money {
        Money::new((self.subtotal().amount as f64 * 0.12).round() as i64, self.currency)
    }

    pub fn total_due(&self) -> Money {
        Money::new(self.subtotal().amount + self.vat_amount().amount, self.currency)
    }

    /// Business rule: Invoice is overdue if unpaid past due_at
    pub fn check_overdue(&mut self) {
        if self.status == InvoiceStatus::Issued && Utc::now() > self.due_at {
            self.status = InvoiceStatus::Overdue;
        }
    }

    /// Business rule: Net-15 payment terms by default
    pub fn with_net_15_terms(mut self) -> Self {
        self.due_at = self.issued_at + Duration::days(15);
        self
    }

    /// Business rule: Cannot cancel a paid invoice
    pub fn can_cancel(&self) -> bool {
        !matches!(self.status, InvoiceStatus::Paid)
    }
}
