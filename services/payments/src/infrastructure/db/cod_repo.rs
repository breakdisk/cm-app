use async_trait::async_trait;
use sqlx::PgPool;
use logisticos_types::{MerchantId, Money, Currency, TenantId};
use uuid::Uuid;
use crate::domain::{entities::{CodCollection, CodStatus}, repositories::CodRepository};

pub struct PgCodRepository { pool: PgPool }
impl PgCodRepository { pub fn new(pool: PgPool) -> Self { Self { pool } } }

#[derive(sqlx::FromRow)]
struct CodRow {
    id:           Uuid,
    tenant_id:    Uuid,
    merchant_id:  Uuid,
    shipment_id:  Uuid,
    driver_id:    Uuid,
    pod_id:       Uuid,
    amount_cents: i64,
    currency:     String,
    status:       String,
    collected_at: chrono::DateTime<chrono::Utc>,
    remitted_at:  Option<chrono::DateTime<chrono::Utc>>,
    batch_id:     Option<Uuid>,
}

fn parse_status(s: &str) -> CodStatus {
    match s {
        "in_batch"  => CodStatus::InBatch,
        "remitted"  => CodStatus::Remitted,
        "disputed"  => CodStatus::Disputed,
        _           => CodStatus::Collected,
    }
}
fn status_str(s: CodStatus) -> &'static str {
    match s {
        CodStatus::Collected => "collected",
        CodStatus::InBatch   => "in_batch",
        CodStatus::Remitted  => "remitted",
        CodStatus::Disputed  => "disputed",
    }
}

const SELECT_COLS: &str =
    "id, tenant_id, merchant_id, shipment_id, driver_id, pod_id, amount_cents, currency,
     status, collected_at, remitted_at, batch_id";

impl From<CodRow> for CodCollection {
    fn from(r: CodRow) -> Self {
        let currency = if r.currency == "USD" { Currency::USD } else { Currency::PHP };
        CodCollection {
            id:           r.id,
            tenant_id:    TenantId::from_uuid(r.tenant_id),
            merchant_id:  MerchantId::from_uuid(r.merchant_id),
            shipment_id:  r.shipment_id,
            driver_id:    r.driver_id,
            pod_id:       r.pod_id,
            amount:       Money::new(r.amount_cents, currency),
            status:       parse_status(&r.status),
            collected_at: r.collected_at,
            remitted_at:  r.remitted_at,
            batch_id:     r.batch_id,
        }
    }
}

#[async_trait]
impl CodRepository for PgCodRepository {
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<CodCollection>> {
        let sql = format!(
            "SELECT {SELECT_COLS} FROM payments.cod_collections WHERE id = $1"
        );
        let row = sqlx::query_as::<_, CodRow>(&sql)
            .bind(id).fetch_optional(&self.pool).await?;
        Ok(row.map(CodCollection::from))
    }

    async fn find_by_shipment(&self, shipment_id: Uuid) -> anyhow::Result<Option<CodCollection>> {
        let sql = format!(
            "SELECT {SELECT_COLS} FROM payments.cod_collections WHERE shipment_id = $1"
        );
        let row = sqlx::query_as::<_, CodRow>(&sql)
            .bind(shipment_id).fetch_optional(&self.pool).await?;
        Ok(row.map(CodCollection::from))
    }

    async fn list_pending_by_tenant(&self, tenant_id: &TenantId) -> anyhow::Result<Vec<CodCollection>> {
        let sql = format!(
            "SELECT {SELECT_COLS}
               FROM payments.cod_collections
              WHERE tenant_id = $1 AND status = 'collected'
              ORDER BY collected_at ASC"
        );
        let rows = sqlx::query_as::<_, CodRow>(&sql)
            .bind(tenant_id.inner()).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(CodCollection::from).collect())
    }

    async fn save(&self, c: &CodCollection) -> anyhow::Result<()> {
        let status = status_str(c.status);
        let currency = format!("{:?}", c.amount.currency);
        sqlx::query(
            r#"INSERT INTO payments.cod_collections
                   (id, tenant_id, merchant_id, shipment_id, driver_id, pod_id, amount_cents,
                    currency, status, collected_at, remitted_at, batch_id)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
               ON CONFLICT (shipment_id) DO UPDATE SET
                   status      = EXCLUDED.status,
                   remitted_at = EXCLUDED.remitted_at,
                   batch_id    = EXCLUDED.batch_id"#
        )
        .bind(c.id).bind(c.tenant_id.inner()).bind(c.merchant_id.inner())
        .bind(c.shipment_id).bind(c.driver_id).bind(c.pod_id)
        .bind(c.amount.amount).bind(currency).bind(status)
        .bind(c.collected_at).bind(c.remitted_at).bind(c.batch_id)
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn list_unbatched_for_merchant(
        &self,
        tenant_id:   &TenantId,
        merchant_id: &MerchantId,
        cutoff:      chrono::DateTime<chrono::Utc>,
    ) -> anyhow::Result<Vec<CodCollection>> {
        let sql = format!(
            "SELECT {SELECT_COLS}
               FROM payments.cod_collections
              WHERE tenant_id   = $1
                AND merchant_id = $2
                AND status      = 'collected'
                AND batch_id    IS NULL
                AND collected_at <= $3
              ORDER BY collected_at ASC"
        );
        let rows = sqlx::query_as::<_, CodRow>(&sql)
            .bind(tenant_id.inner())
            .bind(merchant_id.inner())
            .bind(cutoff)
            .fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(CodCollection::from).collect())
    }

    async fn assign_to_batch(
        &self,
        tenant_id: &TenantId,
        cod_ids:   &[Uuid],
        batch_id:  Uuid,
    ) -> anyhow::Result<u64> {
        if cod_ids.is_empty() {
            return Ok(0);
        }
        let res = sqlx::query(
            r#"UPDATE payments.cod_collections
                  SET status = 'in_batch', batch_id = $1
                WHERE tenant_id = $2
                  AND id = ANY($3)
                  AND status = 'collected'
                  AND batch_id IS NULL"#
        )
        .bind(batch_id)
        .bind(tenant_id.inner())
        .bind(cod_ids)
        .execute(&self.pool).await?;
        Ok(res.rows_affected())
    }

    async fn mark_batch_remitted(
        &self,
        tenant_id: &TenantId,
        batch_id:  Uuid,
    ) -> anyhow::Result<u64> {
        let res = sqlx::query(
            r#"UPDATE payments.cod_collections
                  SET status = 'remitted', remitted_at = NOW()
                WHERE tenant_id = $1
                  AND batch_id  = $2
                  AND status    = 'in_batch'"#
        )
        .bind(tenant_id.inner())
        .bind(batch_id)
        .execute(&self.pool).await?;
        Ok(res.rows_affected())
    }
}
