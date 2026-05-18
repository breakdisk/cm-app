use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;
use logisticos_types::TenantId;

use crate::domain::{
    entities::DriverLedger,
    repositories::{CashLiabilitySummary, DriverLedgerRepository},
};

pub struct PgDriverLedgerRepository { pool: PgPool }
impl PgDriverLedgerRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl DriverLedgerRepository for PgDriverLedgerRepository {
    async fn find_for_shift(
        &self,
        _tenant_id: &TenantId,
        _driver_id: Uuid,
        _shift_id:  Option<Uuid>,
    ) -> anyhow::Result<Option<DriverLedger>> {
        anyhow::bail!("PgDriverLedgerRepository::find_for_shift not yet implemented")
    }

    async fn find_active_for_driver(
        &self,
        _tenant_id: &TenantId,
        _driver_id: Uuid,
    ) -> anyhow::Result<Option<DriverLedger>> {
        anyhow::bail!("PgDriverLedgerRepository::find_active_for_driver not yet implemented")
    }

    async fn save(&self, _ledger: &DriverLedger) -> anyhow::Result<()> {
        anyhow::bail!("PgDriverLedgerRepository::save not yet implemented")
    }

    async fn find_or_create_for_shift(
        &self,
        _tenant_id: &TenantId,
        _driver_id: Uuid,
        _shift_id:  Option<Uuid>,
    ) -> anyhow::Result<DriverLedger> {
        anyhow::bail!("PgDriverLedgerRepository::find_or_create_for_shift not yet implemented")
    }

    async fn cash_liability_summary(
        &self,
        _tenant_id: &TenantId,
        _driver_id: Uuid,
        _shift_id:  Option<Uuid>,
    ) -> anyhow::Result<CashLiabilitySummary> {
        anyhow::bail!("PgDriverLedgerRepository::cash_liability_summary not yet implemented")
    }
}
