use async_trait::async_trait;
use logisticos_types::{CustomerId, InvoiceId, MerchantId, TenantId};
use uuid::Uuid;
use crate::domain::entities::{Invoice, CodCollection, CodRemittanceBatch, Wallet, WalletTransaction};

#[async_trait]
pub trait InvoiceRepository: Send + Sync {
    async fn find_by_id(&self, id: &InvoiceId) -> anyhow::Result<Option<Invoice>>;
    async fn list_by_merchant(&self, merchant_id: &MerchantId) -> anyhow::Result<Vec<Invoice>>;
    /// Lists PaymentReceipt invoices for a B2C customer (customer app inbox).
    async fn list_by_customer(&self, customer_id: &CustomerId) -> anyhow::Result<Vec<Invoice>>;
    /// Lists all invoices for a tenant — admin/ops oversight across every merchant.
    /// Excludes PaymentReceipt (customer-facing) invoices so the admin console
    /// only shows B2B billing state.
    async fn list_by_tenant(&self, tenant_id: &TenantId) -> anyhow::Result<Vec<Invoice>>;
    async fn save(&self, invoice: &Invoice) -> anyhow::Result<()>;
    /// Returns the most recently issued (status = 'issued') ShipmentCharges invoice
    /// for a merchant. Used by the weight-discrepancy consumer to find which invoice
    /// to append the surcharge adjustment to.
    async fn find_latest_issued_for_merchant(
        &self,
        tenant_id:   &TenantId,
        merchant_id: &MerchantId,
    ) -> anyhow::Result<Option<Invoice>>;
}

#[async_trait]
pub trait CodRepository: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<CodCollection>>;
    async fn find_by_shipment(&self, shipment_id: Uuid) -> anyhow::Result<Option<CodCollection>>;
    async fn list_pending_by_tenant(&self, tenant_id: &TenantId) -> anyhow::Result<Vec<CodCollection>>;
    async fn save(&self, cod: &CodCollection) -> anyhow::Result<()>;

    /// All COD rows the given driver collected on a specific UTC calendar date.
    /// Powers the driver-app end-of-shift cash summary — so status is not filtered
    /// (a row that has already been handed in is still part of today's activity).
    async fn list_for_driver_on_day(
        &self,
        tenant_id: &TenantId,
        driver_id: Uuid,
        day:       chrono::NaiveDate,
    ) -> anyhow::Result<Vec<CodCollection>>;

    /// Collected-but-unbatched rows for a merchant up to `cutoff` (inclusive).
    /// Status must be `collected`, `batch_id IS NULL`, `collected_at <= cutoff`.
    async fn list_unbatched_for_merchant(
        &self,
        tenant_id:   &TenantId,
        merchant_id: &MerchantId,
        cutoff:      chrono::DateTime<chrono::Utc>,
    ) -> anyhow::Result<Vec<CodCollection>>;

    /// Returns distinct (tenant_id, merchant_id) pairs that have at least one
    /// unbatched COD collection with collected_at <= cutoff.
    /// Used by the nightly COD batching cron to discover which merchants to sweep.
    async fn distinct_merchants_with_unbatched_cod(
        &self,
        cutoff: chrono::DateTime<chrono::Utc>,
    ) -> anyhow::Result<Vec<(uuid::Uuid, uuid::Uuid)>>;

    /// Bulk-assign rows to a batch and flip `collected` → `in_batch`.
    /// Only affects rows currently `collected` with NULL batch_id.
    /// Returns the number of rows actually updated.
    async fn assign_to_batch(
        &self,
        tenant_id: &TenantId,
        cod_ids:   &[Uuid],
        batch_id:  Uuid,
    ) -> anyhow::Result<u64>;

    /// Bulk-transition all rows in a batch to `remitted`.
    /// Only affects rows currently `in_batch`.
    async fn mark_batch_remitted(
        &self,
        tenant_id: &TenantId,
        batch_id:  Uuid,
    ) -> anyhow::Result<u64>;
}

#[async_trait]
pub trait CodRemittanceBatchRepository: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<CodRemittanceBatch>>;
    async fn save(&self, batch: &CodRemittanceBatch) -> anyhow::Result<()>;
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

/// Billing-ready breakdown for a single delivered shipment in a billing period.
#[derive(Debug, Clone)]
pub struct BillingShipmentDto {
    pub shipment_id:          Uuid,
    pub awb:                  String,
    pub merchant_id:          Uuid,
    pub currency:             String,
    pub base_freight_cents:   i64,
    pub fuel_surcharge_cents: i64,
    pub insurance_cents:      i64,
    pub total_cents:          i64,
    pub delivered_at:         chrono::DateTime<chrono::Utc>,
}

/// Driven port — enumerates delivered shipments for a merchant in a billing window.
/// Implemented by `OrderIntakeClient` against `/v1/internal/billing/shipments`.
#[async_trait]
pub trait MerchantBillingSource: Send + Sync {
    async fn list_delivered(
        &self,
        tenant_id:   Uuid,
        merchant_id: Uuid,
        from:        chrono::DateTime<chrono::Utc>,
        to:          chrono::DateTime<chrono::Utc>,
    ) -> anyhow::Result<Vec<BillingShipmentDto>>;
}

#[async_trait]
pub trait BillingRunRepository: Send + Sync {
    /// Returns the existing run for (tenant, merchant, period_start, period_end) if any.
    async fn find_for_period(
        &self,
        tenant_id:     &TenantId,
        merchant_id:   &MerchantId,
        period_start:  chrono::NaiveDate,
        period_end:    chrono::NaiveDate,
    ) -> anyhow::Result<Option<BillingRunRecord>>;

    async fn save(&self, run: &BillingRunRecord) -> anyhow::Result<()>;
}

/// Audit + idempotency record for an invoice-generating billing run.
#[derive(Debug, Clone)]
pub struct BillingRunRecord {
    pub id:             Uuid,
    pub tenant_id:      TenantId,
    pub merchant_id:    MerchantId,
    pub period_start:   chrono::NaiveDate,
    pub period_end:     chrono::NaiveDate,
    pub invoice_id:     Option<InvoiceId>,
    pub shipment_count: i32,
    pub total_cents:    i64,
    pub created_at:     chrono::DateTime<chrono::Utc>,
}
