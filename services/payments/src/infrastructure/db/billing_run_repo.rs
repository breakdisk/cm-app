use async_trait::async_trait;
use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;
use logisticos_types::{InvoiceId, MerchantId, TenantId};

use crate::domain::repositories::{BillingRunRecord, BillingRunRepository};

pub struct PgBillingRunRepository {
    pool: PgPool,
}

impl PgBillingRunRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl BillingRunRepository for PgBillingRunRepository {
    async fn find_for_period(
        &self,
        tenant_id:     &TenantId,
        merchant_id:   &MerchantId,
        period_start:  NaiveDate,
        period_end:    NaiveDate,
    ) -> anyhow::Result<Option<BillingRunRecord>> {
        let row = sqlx::query_as::<_, BillingRunRow>(
            r#"SELECT id, tenant_id, merchant_id, period_start, period_end,
                      invoice_id, shipment_count, total_cents, created_at
               FROM payments.billing_runs
               WHERE tenant_id    = $1
                 AND merchant_id  = $2
                 AND period_start = $3
                 AND period_end   = $4"#,
        )
        .bind(tenant_id.inner())
        .bind(merchant_id.inner())
        .bind(period_start)
        .bind(period_end)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(Into::into))
    }

    async fn save(&self, run: &BillingRunRecord) -> anyhow::Result<()> {
        sqlx::query(
            r#"INSERT INTO payments.billing_runs (
                   id, tenant_id, merchant_id, period_start, period_end,
                   invoice_id, shipment_count, total_cents, created_at
               ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
               ON CONFLICT (tenant_id, merchant_id, period_start, period_end) DO UPDATE
                   SET invoice_id     = EXCLUDED.invoice_id,
                       shipment_count = EXCLUDED.shipment_count,
                       total_cents    = EXCLUDED.total_cents"#,
        )
        .bind(run.id)
        .bind(run.tenant_id.inner())
        .bind(run.merchant_id.inner())
        .bind(run.period_start)
        .bind(run.period_end)
        .bind(run.invoice_id.as_ref().map(|i| i.inner()))
        .bind(run.shipment_count)
        .bind(run.total_cents)
        .bind(run.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct BillingRunRow {
    id:             Uuid,
    tenant_id:      Uuid,
    merchant_id:    Uuid,
    period_start:   NaiveDate,
    period_end:     NaiveDate,
    invoice_id:     Option<Uuid>,
    shipment_count: i32,
    total_cents:    i64,
    created_at:     chrono::DateTime<chrono::Utc>,
}

impl From<BillingRunRow> for BillingRunRecord {
    fn from(r: BillingRunRow) -> Self {
        Self {
            id:             r.id,
            tenant_id:      TenantId::from_uuid(r.tenant_id),
            merchant_id:    MerchantId::from_uuid(r.merchant_id),
            period_start:   r.period_start,
            period_end:     r.period_end,
            invoice_id:     r.invoice_id.map(InvoiceId::from_uuid),
            shipment_count: r.shipment_count,
            total_cents:    r.total_cents,
            created_at:     r.created_at,
        }
    }
}
