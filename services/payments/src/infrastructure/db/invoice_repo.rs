use async_trait::async_trait;
use chrono::NaiveDate;
use sqlx::PgPool;
use uuid::Uuid;
use logisticos_types::{
    invoice::{ChargeType, InvoiceNumber, InvoiceType},
    Currency, CustomerId, InvoiceId, MerchantId, TenantId,
};

use crate::domain::{
    entities::{BillingPeriod, Invoice, InvoiceAdjustment, InvoiceLineItem, InvoiceStatus},
    repositories::InvoiceRepository,
};

pub struct PgInvoiceRepository { pool: PgPool }
impl PgInvoiceRepository { pub fn new(pool: PgPool) -> Self { Self { pool } } }

// ── Row type ──────────────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct InvoiceRow {
    id:              Uuid,
    invoice_number:  String,
    invoice_type:    String,
    tenant_id:       Uuid,
    merchant_id:     Uuid,
    customer_id:     Option<Uuid>,
    billing_start:   NaiveDate,
    billing_end:     NaiveDate,
    status:          String,
    line_items:      serde_json::Value,
    adjustments:     serde_json::Value,
    currency:        String,
    issued_at:       chrono::DateTime<chrono::Utc>,
    due_at:          chrono::DateTime<chrono::Utc>,
    paid_at:         Option<chrono::DateTime<chrono::Utc>>,
    created_at:      chrono::DateTime<chrono::Utc>,
    updated_at:      chrono::DateTime<chrono::Utc>,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn parse_currency(s: &str) -> Currency {
    match s {
        "USD" => Currency::USD,
        "SGD" => Currency::SGD,
        "MYR" => Currency::MYR,
        "IDR" => Currency::IDR,
        _     => Currency::PHP,
    }
}

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

fn parse_invoice_type(s: &str) -> InvoiceType {
    match s {
        "payment_receipt" => InvoiceType::PaymentReceipt,
        _                 => InvoiceType::ShipmentCharges,
    }
}

fn invoice_type_str(t: InvoiceType) -> &'static str {
    match t {
        InvoiceType::ShipmentCharges => "shipment_charges",
        InvoiceType::PaymentReceipt  => "payment_receipt",
        _                            => "other",
    }
}

// ── Row → Domain ──────────────────────────────────────────────────────────────

impl From<InvoiceRow> for Invoice {
    fn from(r: InvoiceRow) -> Self {
        let currency    = parse_currency(&r.currency);
        let inv_type    = parse_invoice_type(&r.invoice_type);
        let line_items: Vec<InvoiceLineItem>    = serde_json::from_value(r.line_items).unwrap_or_default();
        let adjustments: Vec<InvoiceAdjustment> = serde_json::from_value(r.adjustments).unwrap_or_default();

        // Re-parse the invoice number; fall back to a placeholder on corruption.
        let invoice_number = InvoiceNumber::parse(&r.invoice_number)
            .unwrap_or_else(|_| InvoiceNumber::parse("IN-PH1-2026-01-00001").unwrap());

        Invoice {
            id:             InvoiceId::from_uuid(r.id),
            invoice_number,
            invoice_type:   inv_type,
            tenant_id:      TenantId::from_uuid(r.tenant_id),
            merchant_id:    MerchantId::from_uuid(r.merchant_id),
            customer_id:    r.customer_id.map(CustomerId::from_uuid),
            billing_period: BillingPeriod { start: r.billing_start, end: r.billing_end },
            line_items,
            adjustments,
            status:         parse_status(&r.status),
            currency,
            issued_at:      r.issued_at,
            due_at:         r.due_at,
            paid_at:        r.paid_at,
            created_at:     r.created_at,
            updated_at:     r.updated_at,
        }
    }
}

// ── Repository impl ───────────────────────────────────────────────────────────

const SELECT: &str = "SELECT id, invoice_number, invoice_type, tenant_id, merchant_id,
    customer_id, billing_start, billing_end, status, line_items, adjustments, currency,
    issued_at, due_at, paid_at, created_at, updated_at
    FROM payments.invoices";

#[async_trait]
impl InvoiceRepository for PgInvoiceRepository {
    async fn find_by_id(&self, id: &InvoiceId) -> anyhow::Result<Option<Invoice>> {
        let row = sqlx::query_as::<_, InvoiceRow>(
            &format!("{SELECT} WHERE id = $1"),
        )
        .bind(id.inner())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Invoice::from))
    }

    async fn list_by_merchant(&self, merchant_id: &MerchantId) -> anyhow::Result<Vec<Invoice>> {
        let rows = sqlx::query_as::<_, InvoiceRow>(
            &format!("{SELECT} WHERE merchant_id = $1 ORDER BY issued_at DESC"),
        )
        .bind(merchant_id.inner())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Invoice::from).collect())
    }

    async fn list_by_customer(&self, customer_id: &CustomerId) -> anyhow::Result<Vec<Invoice>> {
        let rows = sqlx::query_as::<_, InvoiceRow>(
            &format!("{SELECT} WHERE customer_id = $1 ORDER BY issued_at DESC"),
        )
        .bind(customer_id.inner())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Invoice::from).collect())
    }

    async fn list_by_tenant(&self, tenant_id: &TenantId) -> anyhow::Result<Vec<Invoice>> {
        // Exclude per-shipment customer receipts — the admin oversight view
        // is about merchant billing only.
        let rows = sqlx::query_as::<_, InvoiceRow>(
            &format!(
                "{SELECT} WHERE tenant_id = $1 AND invoice_type <> 'payment_receipt' \
                 ORDER BY issued_at DESC"
            ),
        )
        .bind(tenant_id.inner())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Invoice::from).collect())
    }

    async fn find_latest_issued_for_merchant(
        &self,
        tenant_id:   &TenantId,
        merchant_id: &MerchantId,
    ) -> anyhow::Result<Option<Invoice>> {
        let row = sqlx::query_as::<_, InvoiceRow>(
            &format!(
                "{SELECT} WHERE tenant_id = $1 AND merchant_id = $2
                  AND status = 'issued'
                  AND invoice_type = 'shipment_charges'
                 ORDER BY issued_at DESC LIMIT 1"
            ),
        )
        .bind(tenant_id.inner())
        .bind(merchant_id.inner())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Invoice::from))
    }

    async fn save(&self, inv: &Invoice) -> anyhow::Result<()> {
        let status       = status_str(inv.status);
        let inv_type     = invoice_type_str(inv.invoice_type);
        let currency     = format!("{:?}", inv.currency);
        let line_items   = serde_json::to_value(&inv.line_items)?;
        let adjustments  = serde_json::to_value(&inv.adjustments)?;
        let inv_num      = inv.invoice_number.to_string();
        let customer_id  = inv.customer_id.as_ref().map(|c| c.inner());

        sqlx::query(
            r#"INSERT INTO payments.invoices
                   (id, invoice_number, invoice_type, tenant_id, merchant_id, customer_id,
                    billing_start, billing_end,
                    status, line_items, adjustments, currency,
                    issued_at, due_at, paid_at, created_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)
               ON CONFLICT (id) DO UPDATE SET
                   status      = EXCLUDED.status,
                   line_items  = EXCLUDED.line_items,
                   adjustments = EXCLUDED.adjustments,
                   paid_at     = EXCLUDED.paid_at,
                   customer_id = EXCLUDED.customer_id,
                   updated_at  = EXCLUDED.updated_at"#,
        )
        .bind(inv.id.inner())
        .bind(&inv_num)
        .bind(inv_type)
        .bind(inv.tenant_id.inner())
        .bind(inv.merchant_id.inner())
        .bind(customer_id)
        .bind(inv.billing_period.start)
        .bind(inv.billing_period.end)
        .bind(status)
        .bind(line_items)
        .bind(adjustments)
        .bind(&currency)
        .bind(inv.issued_at)
        .bind(inv.due_at)
        .bind(inv.paid_at)
        .bind(inv.created_at)
        .bind(inv.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
