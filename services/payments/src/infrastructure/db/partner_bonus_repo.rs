use sqlx::PgPool;
use uuid::Uuid;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PartnerBonus {
    pub id:              Uuid,
    pub tenant_id:       Uuid,
    pub merchant_id:     Uuid,
    pub amount_centavos: i64,
    pub currency:        String,
    pub reason:          String,
    pub effective_month: NaiveDate,
    pub created_by:      Uuid,
    pub created_at:      chrono::DateTime<chrono::Utc>,
}

pub struct PgPartnerBonusRepo { pool: PgPool }
impl PgPartnerBonusRepo { pub fn new(pool: PgPool) -> Self { Self { pool } } }

impl PgPartnerBonusRepo {
    pub async fn insert(&self, b: &PartnerBonus) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO payments.partner_bonuses
             (id, tenant_id, merchant_id, amount_centavos, currency, reason,
              effective_month, created_by, created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)"
        )
        .bind(b.id).bind(b.tenant_id).bind(b.merchant_id).bind(b.amount_centavos)
        .bind(&b.currency).bind(&b.reason).bind(b.effective_month)
        .bind(b.created_by).bind(b.created_at)
        .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn sum_for_merchant_month(
        &self,
        merchant_id: Uuid,
        month_start: NaiveDate,
    ) -> anyhow::Result<i64> {
        let row: (Option<i64>,) = sqlx::query_as(
            "SELECT COALESCE(SUM(amount_centavos), 0)
             FROM payments.partner_bonuses
             WHERE merchant_id = $1
               AND date_trunc('month', effective_month) = date_trunc('month', $2::date)"
        ).bind(merchant_id).bind(month_start).fetch_one(&self.pool).await?;
        Ok(row.0.unwrap_or(0))
    }
}
