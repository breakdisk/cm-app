use async_trait::async_trait;
use sqlx::PgPool;
use logisticos_types::{Money, Currency, TenantId};
use uuid::Uuid;
use crate::domain::{
    entities::{Wallet, WalletTransaction, TransactionType},
    repositories::WalletRepository,
};

pub struct PgWalletRepository { pool: PgPool }
impl PgWalletRepository { pub fn new(pool: PgPool) -> Self { Self { pool } } }

#[derive(sqlx::FromRow)]
struct WalletRow {
    id:            Uuid,
    tenant_id:     Uuid,
    balance_cents: i64,
    currency:      String,
    version:       i64,
    created_at:    chrono::DateTime<chrono::Utc>,
    updated_at:    chrono::DateTime<chrono::Utc>,
}

#[derive(sqlx::FromRow)]
struct TxRow {
    id:               Uuid,
    wallet_id:        Uuid,
    tenant_id:        Uuid,
    transaction_type: String,
    amount_cents:     i64,
    currency:         String,
    reference_id:     Uuid,
    description:      String,
    created_at:       chrono::DateTime<chrono::Utc>,
}

fn parse_tx_type(s: &str) -> TransactionType {
    match s {
        "invoice_debit"     => TransactionType::InvoiceDebit,
        "platform_fee"      => TransactionType::PlatformFeeDebit,
        "withdrawal"        => TransactionType::Withdrawal,
        "refund_credit"     => TransactionType::RefundCredit,
        "adjustment_credit" => TransactionType::AdjustmentCredit,
        "adjustment_debit"  => TransactionType::AdjustmentDebit,
        _                   => TransactionType::CodCredit,
    }
}

fn tx_type_str(t: TransactionType) -> &'static str {
    match t {
        TransactionType::CodCredit        => "cod_credit",
        TransactionType::InvoiceDebit     => "invoice_debit",
        TransactionType::PlatformFeeDebit => "platform_fee",
        TransactionType::Withdrawal       => "withdrawal",
        TransactionType::RefundCredit     => "refund_credit",
        TransactionType::AdjustmentCredit => "adjustment_credit",
        TransactionType::AdjustmentDebit  => "adjustment_debit",
    }
}

impl From<WalletRow> for Wallet {
    fn from(r: WalletRow) -> Self {
        let currency = if r.currency == "USD" { Currency::USD } else { Currency::PHP };
        Wallet {
            id: r.id,
            tenant_id: TenantId::from_uuid(r.tenant_id),
            balance: Money::new(r.balance_cents, currency),
            currency,
            version: r.version,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

impl From<TxRow> for WalletTransaction {
    fn from(r: TxRow) -> Self {
        let currency = if r.currency == "USD" { Currency::USD } else { Currency::PHP };
        WalletTransaction {
            id: r.id,
            wallet_id: r.wallet_id,
            tenant_id: TenantId::from_uuid(r.tenant_id),
            transaction_type: parse_tx_type(&r.transaction_type),
            amount: Money::new(r.amount_cents, currency),
            reference_id: r.reference_id,
            description: r.description,
            created_at: r.created_at,
        }
    }
}

#[async_trait]
impl WalletRepository for PgWalletRepository {
    async fn find_by_tenant(&self, tenant_id: &TenantId) -> anyhow::Result<Option<Wallet>> {
        let row = sqlx::query_as!(WalletRow,
            "SELECT id, tenant_id, balance_cents, currency, version, created_at, updated_at
             FROM payments.wallets WHERE tenant_id = $1",
            tenant_id.inner()
        ).fetch_optional(&self.pool).await?;
        Ok(row.map(Wallet::from))
    }

    async fn save_wallet(&self, w: &Wallet) -> anyhow::Result<()> {
        let currency = format!("{:?}", w.currency);
        // Optimistic concurrency: the WHERE version check prevents double-credit
        let rows = sqlx::query!(
            r#"INSERT INTO payments.wallets (id, tenant_id, balance_cents, currency, version, created_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7)
               ON CONFLICT (tenant_id) DO UPDATE SET
                   balance_cents = EXCLUDED.balance_cents,
                   version       = EXCLUDED.version,
                   updated_at    = EXCLUDED.updated_at
               WHERE payments.wallets.version = $5 - 1"#,
            w.id, w.tenant_id.inner(), w.balance.amount, currency,
            w.version, w.created_at, w.updated_at,
        ).execute(&self.pool).await?;

        if rows.rows_affected() == 0 {
            anyhow::bail!("Wallet optimistic lock conflict — concurrent modification detected");
        }
        Ok(())
    }

    async fn record_transaction(&self, tx: &WalletTransaction) -> anyhow::Result<()> {
        let tx_type = tx_type_str(tx.transaction_type);
        let currency = format!("{:?}", tx.amount.currency);
        sqlx::query!(
            "INSERT INTO payments.wallet_transactions
                 (id, wallet_id, tenant_id, transaction_type, amount_cents, currency,
                  reference_id, description, created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
            tx.id, tx.wallet_id, tx.tenant_id.inner(), tx_type,
            tx.amount.amount, currency, tx.reference_id, tx.description, tx.created_at,
        ).execute(&self.pool).await?;
        Ok(())
    }

    async fn list_transactions(&self, wallet_id: Uuid, limit: u32) -> anyhow::Result<Vec<WalletTransaction>> {
        let rows = sqlx::query_as!(TxRow,
            "SELECT id, wallet_id, tenant_id, transaction_type, amount_cents, currency,
                    reference_id, description, created_at
             FROM payments.wallet_transactions
             WHERE wallet_id = $1
             ORDER BY created_at DESC
             LIMIT $2",
            wallet_id, limit as i64
        ).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(WalletTransaction::from).collect())
    }
}
