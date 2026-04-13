use async_trait::async_trait;
use logisticos_types::{CustomerId, InvoiceId, MerchantId, TenantId};
use uuid::Uuid;
use crate::domain::entities::{Invoice, CodCollection, Wallet, WalletTransaction};

#[async_trait]
pub trait InvoiceRepository: Send + Sync {
    async fn find_by_id(&self, id: &InvoiceId) -> anyhow::Result<Option<Invoice>>;
    async fn list_by_merchant(&self, merchant_id: &MerchantId) -> anyhow::Result<Vec<Invoice>>;
    /// Lists PaymentReceipt invoices for a B2C customer (customer app inbox).
    async fn list_by_customer(&self, customer_id: &CustomerId) -> anyhow::Result<Vec<Invoice>>;
    async fn save(&self, invoice: &Invoice) -> anyhow::Result<()>;
}

#[async_trait]
pub trait CodRepository: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<CodCollection>>;
    async fn find_by_shipment(&self, shipment_id: Uuid) -> anyhow::Result<Option<CodCollection>>;
    async fn list_pending_by_tenant(&self, tenant_id: &TenantId) -> anyhow::Result<Vec<CodCollection>>;
    async fn save(&self, cod: &CodCollection) -> anyhow::Result<()>;
}

#[async_trait]
pub trait WalletRepository: Send + Sync {
    async fn find_by_tenant(&self, tenant_id: &TenantId) -> anyhow::Result<Option<Wallet>>;
    async fn save_wallet(&self, wallet: &Wallet) -> anyhow::Result<()>;
    async fn record_transaction(&self, tx: &WalletTransaction) -> anyhow::Result<()>;
    async fn list_transactions(&self, wallet_id: Uuid, limit: u32) -> anyhow::Result<Vec<WalletTransaction>>;
}

/// Pre-computed billing breakdown for a single shipment.
/// Owned by order-intake; payments only consumes it.
#[derive(Debug, Clone)]
pub struct ShipmentBillingDto {
    pub shipment_id:          Uuid,
    pub awb:                  String,
    pub merchant_id:          Uuid,
    pub currency:             String,
    pub base_freight_cents:   i64,
    pub fuel_surcharge_cents: i64,
    pub insurance_cents:      i64,
    pub total_cents:          i64,
}

/// Driven port — fetches per-shipment billing breakdown from order-intake.
/// Implemented in `infrastructure/http/order_intake_client.rs`.
#[async_trait]
pub trait ShipmentBillingSource: Send + Sync {
    async fn fetch(&self, shipment_id: Uuid) -> anyhow::Result<ShipmentBillingDto>;
}
