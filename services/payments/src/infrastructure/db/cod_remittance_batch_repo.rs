use async_trait::async_trait;
use sqlx::PgPool;
use logisticos_types::{Currency, MerchantId, TenantId};
use uuid::Uuid;
use crate::domain::{
    entities::{CodBatchStatus, CodRemittanceBatch},
    repositories::CodRemittanceBatchRepository,
};

pub struct PgCodRemittanceBatchRepository { pool: PgPool }
impl PgCodRemittanceBatchRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[derive(sqlx::FromRow)]
struct BatchRow {
    id:                 Uuid,
    tenant_id:          Uuid,
    merchant_id:        Uuid,
    cutoff_date:        chrono::NaiveDate,
    currency:           String,
    cod_count:          i32,
    gross_cents:        i64,
    platform_fee_cents: i64,
    net_cents:          i64,
    status:             String,
    failure_reason:     Option<String>,
    created_at:         chrono::DateTime<chrono::Utc>,
    paid_at:            Option<chrono::DateTime<chrono::Utc>>,
}

fn parse_status(s: &str) -> CodBatchStatus {
    match s {
        "paid"   => CodBatchStatus::Paid,
        "failed" => CodBatchStatus::Failed,
        _        => CodBatchStatus::Created,
    }
}
fn status_str(s: CodBatchStatus) -> &'static str {
    match s {
        CodBatchStatus::Created => "created",
        CodBatchStatus::Paid    => "paid",
        CodBatchStatus::Failed  => "failed",
    }
}

impl From<BatchRow> for CodRemittanceBatch {
    fn from(r: BatchRow) -> Self {
        let currency = if r.currency == "USD" { Currency::USD } else { Currency::PHP };
        CodRemittanceBatch {
            id:                 r.id,
            tenant_id:          TenantId::from_uuid(r.tenant_id),
            merchant_id:        MerchantId::from_uuid(r.merchant_id),
            cutoff_date:        r.cutoff_date,
            currency,
            cod_count:          r.cod_count,
            gross_cents:        r.gross_cents,
            platform_fee_cents: r.platform_fee_cents,
            net_cents:          r.net_cents,
            status:             parse_status(&r.status),
            created_at:         r.created_at,
            paid_at:            r.paid_at,
            failure_reason:     r.failure_reason,
        }
    }
}

const SELECT_COLS: &str =
    "id, tenant_id, merchant_id, cutoff_date, currency, cod_count,
     gross_cents, platform_fee_cents, net_cents, status, failure_reason,
     created_at, paid_at";

#[async_trait]
impl CodRemittanceBatchRepository for PgCodRemittanceBatchRepository {
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<CodRemittanceBatch>> {
        let sql = format!(
            "SELECT {SELECT_COLS} FROM payments.cod_remittance_batches WHERE id = $1"
        );
        let row = sqlx::query_as::<_, BatchRow>(&sql)
            .bind(id).fetch_optional(&self.pool).await?;
        Ok(row.map(CodRemittanceBatch::from))
    }

    async fn save(&self, b: &CodRemittanceBatch) -> anyhow::Result<()> {
        let currency = format!("{:?}", b.currency);
        sqlx::query(
            r#"INSERT INTO payments.cod_remittance_batches
                   (id, tenant_id, merchant_id, cutoff_date, currency, cod_count,
                    gross_cents, platform_fee_cents, net_cents, status, failure_reason,
                    created_at, paid_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
               ON CONFLICT (id) DO UPDATE SET
                   cod_count          = EXCLUDED.cod_count,
                   gross_cents        = EXCLUDED.gross_cents,
                   platform_fee_cents = EXCLUDED.platform_fee_cents,
                   net_cents          = EXCLUDED.net_cents,
                   status             = EXCLUDED.status,
                   failure_reason     = EXCLUDED.failure_reason,
                   paid_at            = EXCLUDED.paid_at"#
        )
        .bind(b.id)
        .bind(b.tenant_id.inner())
        .bind(b.merchant_id.inner())
        .bind(b.cutoff_date)
        .bind(currency)
        .bind(b.cod_count)
        .bind(b.gross_cents)
        .bind(b.platform_fee_cents)
        .bind(b.net_cents)
        .bind(status_str(b.status))
        .bind(b.failure_reason.as_deref())
        .bind(b.created_at)
        .bind(b.paid_at)
        .execute(&self.pool).await?;
        Ok(())
    }
}
