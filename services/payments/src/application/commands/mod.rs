use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct GenerateInvoiceCommand {
    pub merchant_id: Uuid,
    pub shipment_ids: Vec<Uuid>,   // One line item per shipment delivered this billing period
    pub billing_period_start: chrono::DateTime<chrono::Utc>,
    pub billing_period_end: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct ReconcileCodCommand {
    pub shipment_id: Uuid,
    pub pod_id: Uuid,
    pub driver_id: Uuid,
    pub amount_cents: i64,
}

#[derive(Debug, Deserialize)]
pub struct RequestWithdrawalCommand {
    pub amount_cents: i64,
    pub bank_account_id: Uuid,  // Must be a verified bank account linked to this tenant
}

#[derive(Debug, Serialize)]
pub struct InvoiceSummary {
    pub invoice_id: Uuid,
    pub status: String,
    pub subtotal_cents: i64,
    pub vat_cents: i64,
    pub total_cents: i64,
    pub due_at: String,
    pub issued_at: String,
}

#[derive(Debug, Serialize)]
pub struct WalletSummary {
    pub wallet_id: Uuid,
    pub balance_cents: i64,
    pub currency: String,
    pub updated_at: String,
}
