use chrono::{DateTime, Utc};
use logisticos_types::{Money, TenantId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Cash-on-Delivery reconciliation record.
/// When a driver collects cash, a CodCollection is created.
/// The payments service reconciles these against invoice line items and
/// credits the merchant's wallet (minus the platform's COD handling fee).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodCollection {
    pub id: Uuid,
    pub tenant_id: TenantId,
    pub shipment_id: Uuid,
    pub driver_id: Uuid,
    pub pod_id: Uuid,
    pub amount: Money,
    pub status: CodStatus,
    pub collected_at: DateTime<Utc>,
    pub remitted_at: Option<DateTime<Utc>>,
    pub batch_id: Option<Uuid>,     // Grouped for remittance (daily/weekly batches)
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum CodStatus {
    Collected,   // Driver has the cash; not yet remitted to platform
    InBatch,     // Included in a remittance batch; driver is paying it in
    Remitted,    // Cash received by platform; merchant wallet credited
    Disputed,    // Amount discrepancy; under investigation
}

impl CodCollection {
    pub fn new(
        tenant_id: TenantId,
        shipment_id: Uuid,
        driver_id: Uuid,
        pod_id: Uuid,
        amount: Money,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            shipment_id,
            driver_id,
            pod_id,
            amount,
            status: CodStatus::Collected,
            collected_at: Utc::now(),
            remitted_at: None,
            batch_id: None,
        }
    }

    pub fn assign_to_batch(&mut self, batch_id: Uuid) {
        self.batch_id = Some(batch_id);
        self.status = CodStatus::InBatch;
    }

    pub fn mark_remitted(&mut self) {
        self.status = CodStatus::Remitted;
        self.remitted_at = Some(Utc::now());
    }

    /// Business rule: 1.5% COD handling fee charged to the merchant.
    pub fn platform_fee(&self) -> Money {
        let fee = (self.amount.amount as f64 * 0.015).round() as i64;
        Money::new(fee, self.amount.currency)
    }

    /// Amount credited to merchant wallet after platform fee deduction.
    pub fn merchant_credit(&self) -> Money {
        Money::new(
            self.amount.amount - self.platform_fee().amount,
            self.amount.currency,
        )
    }
}
