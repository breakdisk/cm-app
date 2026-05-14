use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceGenerated {
    pub invoice_id:     Uuid,
    pub invoice_number: String,
    /// "merchant" for ShipmentCharges tax invoices; "customer" for PaymentReceipt.
    pub recipient_type: String,
    /// Merchant UUID — populated for tax invoices; nil UUID for receipts.
    pub merchant_id:    Uuid,
    pub merchant_email: Option<String>,
    /// Customer UUID — populated for payment receipts; nil UUID for tax invoices.
    pub customer_id:    Uuid,
    pub customer_email: Option<String>,
    pub tenant_id:      Uuid,
    pub total_cents:    i64,
    pub currency:       String,
    pub due_at:         chrono::DateTime<chrono::Utc>,
    /// AWB tracking number — used by engagement to populate the receipt template.
    /// Empty string for non-shipment invoices (e.g. weight surcharge credits).
    #[serde(default)]
    pub tracking_number: String,
    /// Customer / recipient name — shown in the "payment_receipt" notification body.
    /// Empty for merchant invoices (engagement shows "Merchant" as fallback).
    #[serde(default)]
    pub customer_name:  String,
    /// Customer phone — enables engagement to send "payment_receipt" WhatsApp.
    /// Empty for merchant invoices (WhatsApp channel skipped when blank).
    #[serde(default)]
    pub customer_phone: String,
    /// ISO date string (YYYY-MM-DD) of the delivery / capture date.
    /// Shown on the payment receipt as the "Paid on" date.
    /// Empty for periodic merchant invoices.
    #[serde(default)]
    pub paid_at:        String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodReconciled {
    pub cod_id:                Uuid,
    pub shipment_id:           Uuid,
    pub tenant_id:             Uuid,
    pub amount_cents:          i64,
    pub merchant_credit_cents: i64,
    pub platform_fee_cents:    i64,
    /// AWB for the engagement "cod_receipt" template variable. Populated from
    /// ShipmentBillingDto.awb which is already fetched during reconciliation.
    pub tracking_number:       String,
    /// Customer phone — for engagement's "cod_receipt" WhatsApp notification.
    /// Forwarded from ReconcileCodCommand (originated in SubmitPodCommand).
    #[serde(default)]
    pub customer_phone:        String,
    /// Recipient name from POD — shown in the cod_receipt WhatsApp body.
    #[serde(default)]
    pub customer_name:         String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodRemitted {
    pub batch_id:             Uuid,
    pub tenant_id:            Uuid,
    pub merchant_id:          Uuid,
    pub cutoff_date:          chrono::NaiveDate,
    pub cod_count:            i32,
    pub gross_cents:          i64,
    pub platform_fee_cents:   i64,
    pub net_credit_cents:     i64,
    pub remitted_at:          chrono::DateTime<chrono::Utc>,
    /// Merchant billing email — populated from MerchantBillingAccount.billing_email if found.
    /// Empty string if no billing account is configured; engagement will skip email.
    #[serde(default)]
    pub merchant_email:       String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletCredited {
    pub wallet_id: Uuid,
    pub tenant_id: Uuid,
    pub amount_cents: i64,
    pub balance_after_cents: i64,
    pub reference_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WithdrawalDisbursed {
    pub withdrawal_id:   Uuid,
    pub tenant_id:       Uuid,
    pub wallet_id:       Uuid,
    pub amount_centavos: i64,
    pub disbursed_by:    Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WithdrawalRejected {
    pub withdrawal_id: Uuid,
    pub tenant_id:     Uuid,
    pub wallet_id:     Uuid,
    pub amount_centavos: i64,
    pub rejected_by:   Uuid,
    pub review_note:   Option<String>,
}
