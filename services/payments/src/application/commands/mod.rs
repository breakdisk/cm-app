use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One billable charge entry for a single AWB — passed in when generating an invoice.
#[derive(Debug, Deserialize)]
pub struct AwbChargeInput {
    /// Master AWB string (e.g. "CM-PH1-S0001234X").
    pub awb:              String,
    /// Charge type string (e.g. "base_freight", "weight_surcharge").
    pub charge_type:      String,
    pub description:      String,
    pub quantity:         u32,
    pub unit_price_cents: i64,
    pub discount_cents:   Option<i64>,
}

/// Generate a shipment-charges invoice for a merchant covering a billing period.
///
/// Called by the billing cron job (weekly for Starter, monthly for Business+).
/// `charges` is the pre-computed list of AWB-level fees; the service builds the
/// `InvoiceLineItem` records from these inputs.
#[derive(Debug, Deserialize)]
pub struct GenerateInvoiceCommand {
    pub merchant_id:          Uuid,
    pub merchant_email:       Option<String>,
    pub tenant_code:          String,    // 3-char, e.g. "PH1"
    pub billing_period_year:  i32,
    pub billing_period_month: u32,
    /// Pre-computed per-AWB charges for the billing period.
    pub charges:              Vec<AwbChargeInput>,
}

/// Apply a weight-discrepancy adjustment to an already-issued invoice.
///
/// Triggered by `WeightDiscrepancyFound` Kafka events from hub-ops.
#[derive(Debug, Deserialize)]
pub struct ApplyWeightAdjustmentCommand {
    pub invoice_id:       Uuid,
    pub awb:              String,
    pub declared_grams:   u32,
    pub actual_grams:     u32,
    pub surcharge_cents:  i64,
    pub applied_by:       Uuid,
}

/// Issue a per-shipment payment receipt for a B2C self-booking once the
/// shipment is delivered. Money was already preauthorised at booking time,
/// so the receipt is issued and immediately marked paid.
///
/// Triggered by `PodConsumer` when it receives `pod.captured` for a shipment
/// whose `booked_by_customer == true`.
#[derive(Debug, Deserialize)]
pub struct IssuePaymentReceiptCommand {
    pub shipment_id:    Uuid,
    pub tenant_code:    String,        // 3-char, e.g. "PH1"
    pub customer_id:    Uuid,          // recipient — receipts go to customers, not merchants
    pub customer_email: Option<String>,
    pub delivered_on:   NaiveDate,     // used for the billing period (single-day window)
}

#[derive(Debug, Deserialize)]
pub struct ReconcileCodCommand {
    pub shipment_id:  Uuid,
    pub pod_id:       Uuid,
    pub driver_id:    Uuid,
    pub amount_cents: i64,
}

#[derive(Debug, Deserialize)]
pub struct RequestWithdrawalCommand {
    pub amount_cents:    i64,
    pub bank_account_id: Uuid,
}

/// Run the monthly billing aggregation for a single (tenant, merchant) and
/// issue a shipment-charges invoice covering all shipments delivered in the
/// period. Idempotent on `(tenant_id, merchant_id, year, month)`.
#[derive(Debug, Deserialize)]
pub struct RunBillingCommand {
    pub tenant_id:      Uuid,
    pub tenant_code:    String,          // 3-char AWB tenant code, e.g. "PH1"
    pub merchant_id:    Uuid,
    pub merchant_email: Option<String>,
    pub year:           i32,
    pub month:          u32,
}

// ── Response shapes ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct InvoiceSummary {
    pub invoice_id:      Uuid,
    pub invoice_number:  String,
    pub invoice_type:    String,
    pub status:          String,
    pub awb_count:       usize,
    pub subtotal_cents:  i64,
    pub vat_cents:       i64,
    pub total_cents:     i64,
    pub billing_period:  String,  // "2026-04"
    pub due_at:          String,
    pub issued_at:       String,
}

#[derive(Debug, Serialize)]
pub struct WalletSummary {
    pub wallet_id:     Uuid,
    pub balance_cents: i64,
    pub currency:      String,
    pub updated_at:    String,
}
