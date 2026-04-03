use async_trait::async_trait;
use sqlx::PgPool;
use logisticos_types::{InvoiceId, MerchantId, Money, Currency};
use uuid::Uuid;
use crate::domain::{
    entities::{Invoice, InvoiceLineItem, InvoiceStatus},
    repositories::InvoiceRepository,
};

pub struct PgInvoiceRepository { pool: PgPool }
impl PgInvoiceRepository { pub fn new(pool: PgPool) -> Self { Self { pool } } }

#[derive(sqlx::FromRow)]
struct InvoiceRow {
    id:          Uuid,
    tenant_id:   Uuid,
    merchant_id: Uuid,
    status:      String,
    line_items:  serde_json::Value,
    currency:    String,
    issued_at:   chrono::DateTime<chrono::Utc>,
    due_at:      chrono::DateTime<chrono::Utc>,
    paid_at:     Option<chrono::DateTime<chrono::Utc>>,
}

fn parse_currency(s: &str) -> Currency { if s == "USD" { Currency::USD } else { Currency::PHP } }
fn parse_status(s: &str) -> InvoiceStatus {
    match s {
        "paid"      => InvoiceStatus::Paid,
        "overdue"   => InvoiceStatus::Overdue,
        "disputed"  => InvoiceStatus::Disputed,
        "cancelled" => InvoiceStatus::Cancelled,
        "draft"     => InvoiceStatus::Draft,
        _           => InvoiceStatus::Issued,
    }
}
fn status_str(s: InvoiceStatus) -> &'static str {
    match s {
        InvoiceStatus::Draft     => "draft",
        InvoiceStatus::Issued    => "issued",
        InvoiceStatus::Paid      => "paid",
        InvoiceStatus::Overdue   => "overdue",
        InvoiceStatus::Disputed  => "disputed",
        InvoiceStatus::Cancelled => "cancelled",
    }
}

impl From<InvoiceRow> for Invoice {
    fn from(r: InvoiceRow) -> Self {
        let currency = parse_currency(&r.currency);
        let line_items: Vec<InvoiceLineItem> = serde_json::from_value(r.line_items).unwrap_or_default();
        Invoice {
            id: InvoiceId::from_uuid(r.id),
            merchant_id: MerchantId::from_uuid(r.merchant_id),
            line_items,
            status: parse_status(&r.status),
            issued_at: r.issued_at,
            due_at: r.due_at,
            paid_at: r.paid_at,
            currency,
        }
    }
}

#[async_trait]
impl InvoiceRepository for PgInvoiceRepository {
    async fn find_by_id(&self, id: &InvoiceId) -> anyhow::Result<Option<Invoice>> {
        let row = sqlx::query_as::<_, InvoiceRow>(
            "SELECT id, tenant_id, merchant_id, status, line_items, currency, issued_at, due_at, paid_at
             FROM payments.invoices WHERE id = $1"
        ).bind(id.inner()).fetch_optional(&self.pool).await?;
        Ok(row.map(Invoice::from))
    }

    async fn list_by_merchant(&self, merchant_id: &MerchantId) -> anyhow::Result<Vec<Invoice>> {
        let rows = sqlx::query_as::<_, InvoiceRow>(
            "SELECT id, tenant_id, merchant_id, status, line_items, currency, issued_at, due_at, paid_at
             FROM payments.invoices WHERE merchant_id = $1 ORDER BY issued_at DESC"
        ).bind(merchant_id.inner()).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(Invoice::from).collect())
    }

    async fn save(&self, inv: &Invoice) -> anyhow::Result<()> {
        let status = status_str(inv.status);
        let currency = format!("{:?}", inv.currency);
        let line_items = serde_json::to_value(&inv.line_items)?;
        // Extract tenant_id from merchant_id (same UUID in this 1:1 setup)
        let tenant_id = inv.merchant_id.inner();
        sqlx::query(
            r#"INSERT INTO payments.invoices
                   (id, tenant_id, merchant_id, status, line_items, currency, issued_at, due_at, paid_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
               ON CONFLICT (id) DO UPDATE SET
                   status = EXCLUDED.status, paid_at = EXCLUDED.paid_at"#
        )
        .bind(inv.id.inner()).bind(tenant_id).bind(inv.merchant_id.inner())
        .bind(status).bind(line_items).bind(currency).bind(inv.issued_at).bind(inv.due_at).bind(inv.paid_at)
        .execute(&self.pool).await?;
        Ok(())
    }
}
