use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceGenerated {
    pub invoice_id: Uuid,
    pub merchant_id: Uuid,
    pub tenant_id: Uuid,
    pub total_cents: i64,
    pub due_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodReconciled {
    pub cod_id: Uuid,
    pub shipment_id: Uuid,
    pub tenant_id: Uuid,
    pub amount_cents: i64,
    pub merchant_credit_cents: i64,
    pub platform_fee_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletCredited {
    pub wallet_id: Uuid,
    pub tenant_id: Uuid,
    pub amount_cents: i64,
    pub balance_after_cents: i64,
    pub reference_id: Uuid,
}
