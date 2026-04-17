//! Merchant-scoped COD remittance batch.
//!
//! Groups one or more `CodCollection` rows for a single merchant up to a
//! cutoff date. Created by ops (or a scheduled job) once the driver has
//! handed cash to the hub/finance team; confirmed once finance verifies the
//! physical cash matches. On confirmation, the merchant wallet is credited
//! with the **net** amount (gross − platform fee).
//!
//! State machine: `Created` → `Paid` (credits wallet) | `Failed` (discrepancy)

use chrono::{DateTime, NaiveDate, Utc};
use logisticos_types::{Currency, MerchantId, TenantId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum CodBatchStatus {
    Created,
    Paid,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodRemittanceBatch {
    pub id:                   Uuid,
    pub tenant_id:            TenantId,
    pub merchant_id:          MerchantId,
    pub cutoff_date:          NaiveDate,
    pub currency:             Currency,
    pub cod_count:            i32,
    pub gross_cents:          i64,
    pub platform_fee_cents:   i64,
    pub net_cents:            i64,
    pub status:               CodBatchStatus,
    pub created_at:           DateTime<Utc>,
    pub paid_at:              Option<DateTime<Utc>>,
    pub failure_reason:       Option<String>,
}

impl CodRemittanceBatch {
    pub fn new(
        tenant_id:    TenantId,
        merchant_id:  MerchantId,
        cutoff_date:  NaiveDate,
        currency:     Currency,
        cod_count:    i32,
        gross_cents:  i64,
        fee_cents:    i64,
    ) -> Self {
        Self {
            id:                 Uuid::new_v4(),
            tenant_id,
            merchant_id,
            cutoff_date,
            currency,
            cod_count,
            gross_cents,
            platform_fee_cents: fee_cents,
            net_cents:          gross_cents - fee_cents,
            status:             CodBatchStatus::Created,
            created_at:         Utc::now(),
            paid_at:            None,
            failure_reason:     None,
        }
    }

    pub fn mark_paid(&mut self) -> Result<(), &'static str> {
        if self.status != CodBatchStatus::Created {
            return Err("Only Created batches can be paid");
        }
        self.status  = CodBatchStatus::Paid;
        self.paid_at = Some(Utc::now());
        Ok(())
    }

    pub fn mark_failed(&mut self, reason: String) -> Result<(), &'static str> {
        if self.status != CodBatchStatus::Created {
            return Err("Only Created batches can be failed");
        }
        self.status         = CodBatchStatus::Failed;
        self.failure_reason = Some(reason);
        Ok(())
    }
}
