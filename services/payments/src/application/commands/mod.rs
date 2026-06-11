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
    /// Recipient name from POD — shown on the receipt email/WhatsApp body.
    pub customer_name:  String,
    /// Customer phone for WhatsApp receipt dispatch.
    pub customer_phone: String,
}

#[derive(Debug, Deserialize)]
pub struct ReconcileCodCommand {
    pub shipment_id:    Uuid,
    pub pod_id:         Uuid,
    pub driver_id:      Uuid,
    pub amount_cents:   i64,
    /// Customer phone — forwarded from PodCaptured so engagement's
    /// "cod_receipt" WhatsApp notification has a recipient without a
    /// cross-service lookup. Empty string when absent (non-COD or legacy).
    #[serde(default)]
    pub customer_phone: String,
    /// Recipient name from POD — shown in the COD receipt WhatsApp body.
    #[serde(default)]
    pub customer_name:  String,
}

#[derive(Debug, Deserialize)]
pub struct RequestWithdrawalCommand {
    pub amount_cents:    i64,
    #[serde(default)]
    pub bank_account_id: Option<Uuid>,
    /// Carrier contact email — supplied by the partner portal so the engagement
    /// service can send an email notification when the withdrawal is disbursed or
    /// rejected. Optional; email channel is silently skipped when absent.
    #[serde(default)]
    pub carrier_email:   Option<String>,
}

/// Create a COD remittance batch for one (tenant, merchant) grouping all
/// collected-but-unbatched COD rows up to `cutoff_date` end-of-day UTC.
/// Returns the batch with computed totals in `Created` status.
#[derive(Debug, Deserialize)]
pub struct CreateCodBatchCommand {
    pub tenant_id:   Uuid,
    pub merchant_id: Uuid,
    pub cutoff_date: NaiveDate,
}

/// Confirm a COD remittance batch — finance has verified physical cash or
/// driver payout. Flips batch → `Paid`, flips member COD rows → `remitted`,
/// credits the merchant wallet with `net_cents`, emits `cod.remitted`.
#[derive(Debug, Deserialize)]
pub struct ConfirmCodBatchCommand {
    pub tenant_id: Uuid,
    pub batch_id:  Uuid,
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

#[derive(Debug, Deserialize)]
pub struct AdminRunBillingCommand {
    pub merchant_id:    Uuid,
    pub merchant_email: Option<String>,
    pub tenant_code:    String,
    pub period_start:   NaiveDate,
    pub period_end:     NaiveDate,
}

// ── Response shapes ───────────────────────────────────────────────────────────

/// Invoice list item returned to the partner portal and admin console.
/// All monetary values are in PHP (centavos ÷ 100) so the frontend consumes
/// them directly without conversion.
#[derive(Debug, Serialize)]
pub struct InvoiceSummary {
    pub id:             Uuid,
    pub invoice_number: String,
    pub invoice_type:   String,
    pub status:         String,
    pub awb_count:      usize,
    pub subtotal_php:   f64,
    pub vat_php:        f64,
    pub total_php:      f64,
    pub period_from:    String,   // "2026-04-01"
    pub period_to:      String,   // "2026-04-30"
    pub due_date:       String,   // RFC 3339
    pub paid_at:        Option<String>,
    pub created_at:     String,   // RFC 3339 (= issued_at)
}

/// Wallet balance summary returned to the partner portal.
/// All monetary values are in PHP; centavos stay internal to the domain.
#[derive(Debug, Serialize)]
pub struct WalletSummary {
    pub wallet_id:     Uuid,
    pub tenant_id:     Uuid,
    pub balance_php:   f64,
    pub reserved_php:  f64,
    pub available_php: f64,
    pub currency:      String,
    pub updated_at:    String,
}

/// Wallet transaction item. Maps internal TransactionType to "credit"/"debit"
/// and converts centavos to PHP at the HTTP boundary.
#[derive(Debug, Serialize)]
pub struct WalletTransactionDto {
    pub id:           Uuid,
    #[serde(rename = "type")]
    pub kind:         &'static str,  // "credit" | "debit"
    pub amount_php:   f64,
    pub description:  String,
    pub reference_id: Uuid,
    pub created_at:   String,
}

/// Withdrawal request item returned to the partner portal history list.
#[derive(Debug, Serialize)]
pub struct WithdrawalRequestDto {
    pub id:          Uuid,
    pub amount_php:  f64,
    pub currency:    String,
    pub status:      crate::domain::entities::WithdrawalStatus,
    pub review_note: Option<String>,
    pub created_at:  String,
    pub updated_at:  String,
}
