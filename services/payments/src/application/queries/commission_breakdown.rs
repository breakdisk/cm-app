use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct CommissionBreakdown {
    pub period:                  String,
    pub base_charges_centavos:   i64,
    pub cod_remittance_centavos: i64,
    pub bonuses_centavos:        i64,
    pub total_centavos:          i64,
    pub currency:                String,
}

pub struct CommissionBreakdownQuery { pool: PgPool }
impl CommissionBreakdownQuery {
    pub fn new(pool: PgPool) -> Self { Self { pool } }

    pub async fn run(
        &self,
        merchant_id: Uuid,
        year:        i32,
        month:       u32,
    ) -> anyhow::Result<CommissionBreakdown> {
        let month_start = NaiveDate::from_ymd_opt(year, month, 1)
            .ok_or_else(|| anyhow::anyhow!("invalid year/month: {year}-{month}"))?;

        // Base charges — invoices for this merchant in the given month
        let invoice_rows: Vec<(serde_json::Value, serde_json::Value)> = sqlx::query_as(
            r#"SELECT i.line_items, i.adjustments
               FROM payments.invoices i
               WHERE i.merchant_id = $1
                 AND i.invoice_type = 'shipment_charges'
                 AND i.status IN ('issued', 'paid', 'overdue')
                 AND date_trunc('month', i.billing_start) = date_trunc('month', $2::date)"#
        ).bind(merchant_id).bind(month_start).fetch_all(&self.pool).await?;

        let base_charges_centavos = invoice_rows.iter().map(|(items_json, adjs_json)| {
            let items: Vec<crate::domain::entities::InvoiceLineItem> =
                serde_json::from_value(items_json.clone()).unwrap_or_else(|e| {
                    tracing::warn!(err = %e, "failed to deserialize invoice line_items");
                    vec![]
                });
            let adjs: Vec<crate::domain::entities::InvoiceAdjustment> =
                serde_json::from_value(adjs_json.clone()).unwrap_or_else(|e| {
                    tracing::warn!(err = %e, "failed to deserialize invoice adjustments");
                    vec![]
                });
            let subtotal: i64 = items.iter().map(|i| i.net().amount).sum();
            let adj_total: i64 = adjs.iter().map(|a| a.amount.amount).sum();
            let taxable = subtotal + adj_total;
            let vat = (taxable as f64 * 0.12).round() as i64;
            taxable + vat
        }).sum::<i64>();

        // COD remittance for this merchant in the given month
        let (cod_centavos,): (Option<i64>,) = sqlx::query_as(
            r#"SELECT COALESCE(SUM(b.net_cents), 0)
               FROM payments.cod_remittance_batches b
               WHERE b.merchant_id = $1
                 AND b.status = 'paid'
                 AND date_trunc('month', b.paid_at) = date_trunc('month', $2::date)"#
        ).bind(merchant_id).bind(month_start).fetch_one(&self.pool).await?;
        let cod_centavos = cod_centavos.unwrap_or(0);

        // Bonuses for this merchant in the given month
        let (bonuses_centavos,): (Option<i64>,) = sqlx::query_as(
            "SELECT COALESCE(SUM(amount_centavos), 0)
             FROM payments.partner_bonuses
             WHERE merchant_id = $1
               AND date_trunc('month', effective_month) = date_trunc('month', $2::date)"
        ).bind(merchant_id).bind(month_start).fetch_one(&self.pool).await?;
        let bonuses_centavos = bonuses_centavos.unwrap_or(0);

        let total = base_charges_centavos + cod_centavos + bonuses_centavos;

        Ok(CommissionBreakdown {
            period:                  format!("{year}-{month:02}"),
            base_charges_centavos,
            cod_remittance_centavos: cod_centavos,
            bonuses_centavos,
            total_centavos:          total,
            currency:                "PHP".into(),
        })
    }
}
