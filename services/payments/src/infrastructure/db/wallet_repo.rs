use async_trait::async_trait;
use sqlx::PgPool;
use logisticos_types::{Money, Currency, TenantId};
use uuid::Uuid;
use crate::domain::{
    entities::{Wallet, WalletTransaction, TransactionType},
    repositories::{UnsettledCarrierEarnings, WalletRepository},
};

pub struct PgWalletRepository { pool: PgPool }
impl PgWalletRepository { pub fn new(pool: PgPool) -> Self { Self { pool } } }

#[derive(sqlx::FromRow)]
struct WalletRow {
    id:                Uuid,
    tenant_id:         Uuid,
    balance_cents:     i64,
    currency:          String,
    version:           i64,
    reserved_centavos: i64,
    created_at:        chrono::DateTime<chrono::Utc>,
    updated_at:        chrono::DateTime<chrono::Utc>,
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
        "carrier_margin"    => TransactionType::CarrierMargin,
        "cod_commission"    => TransactionType::CodCommission,
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
        TransactionType::CarrierMargin    => "carrier_margin",
        TransactionType::CodCommission    => "cod_commission",
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
            reserved_centavos: r.reserved_centavos,
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
        // reserved_cents is the canonical column name (migration 0002); alias to match WalletRow.
        // Filter to owner_type='merchant' since 0002 introduced a multi-owner wallet schema.
        let row = sqlx::query_as::<_, WalletRow>(
            "SELECT id, tenant_id, balance_cents, currency, version,
                    reserved_cents AS reserved_centavos, created_at, updated_at
             FROM payments.wallets
             WHERE tenant_id = $1 AND owner_type = 'merchant'"
        ).bind(tenant_id.inner()).fetch_optional(&self.pool).await?;
        Ok(row.map(Wallet::from))
    }

    async fn find_by_id(&self, id: uuid::Uuid) -> anyhow::Result<Option<Wallet>> {
        let row = sqlx::query_as::<_, WalletRow>(
            "SELECT id, tenant_id, balance_cents, currency, version,
                    reserved_cents AS reserved_centavos, created_at, updated_at
             FROM payments.wallets WHERE id = $1"
        ).bind(id).fetch_optional(&self.pool).await?;
        Ok(row.map(Wallet::from))
    }

    async fn save_wallet(&self, w: &Wallet) -> anyhow::Result<()> {
        let currency = w.currency.to_string();
        let tenant_uuid = w.tenant_id.inner();
        // ON CONFLICT target matches the wallets_tenant_owner_unique constraint from migration 0002.
        // owner_id = tenant_id for merchant wallets (1:1 in the merchant portal).
        let rows = sqlx::query(
            r#"INSERT INTO payments.wallets
                   (id, tenant_id, owner_type, owner_id,
                    balance_cents, currency, version, reserved_cents, created_at, updated_at)
               VALUES ($1,$2,'merchant',$2,$3,$4,$5,$6,$7,$8)
               ON CONFLICT (tenant_id, owner_type, owner_id) DO UPDATE SET
                   balance_cents  = EXCLUDED.balance_cents,
                   version        = EXCLUDED.version,
                   reserved_cents = EXCLUDED.reserved_cents,
                   updated_at     = EXCLUDED.updated_at
               WHERE payments.wallets.version = $5 - 1"#
        )
        .bind(w.id).bind(tenant_uuid).bind(w.balance.amount).bind(currency)
        .bind(w.version).bind(w.reserved_centavos).bind(w.created_at).bind(w.updated_at)
        .execute(&self.pool).await?;

        if rows.rows_affected() == 0 {
            anyhow::bail!("Wallet optimistic lock conflict — concurrent modification detected");
        }
        Ok(())
    }

    async fn record_transaction(&self, tx: &WalletTransaction) -> anyhow::Result<()> {
        let tx_type = tx_type_str(tx.transaction_type);
        let currency = tx.amount.currency.to_string();
        sqlx::query(
            "INSERT INTO payments.wallet_transactions
                 (id, wallet_id, tenant_id, transaction_type, amount_cents, currency,
                  reference_id, description, created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)"
        )
        .bind(tx.id).bind(tx.wallet_id).bind(tx.tenant_id.inner()).bind(tx_type)
        .bind(tx.amount.amount).bind(currency).bind(tx.reference_id).bind(&tx.description).bind(tx.created_at)
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn list_transactions(&self, wallet_id: Uuid, limit: u32) -> anyhow::Result<Vec<WalletTransaction>> {
        let rows = sqlx::query_as::<_, TxRow>(
            "SELECT id, wallet_id, tenant_id, transaction_type, amount_cents, currency,
                    reference_id, description, created_at
             FROM payments.wallet_transactions
             WHERE wallet_id = $1
             ORDER BY created_at DESC
             LIMIT $2"
        ).bind(wallet_id).bind(limit as i64).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(WalletTransaction::from).collect())
    }

    async fn find_carrier_wallet(&self, tenant_id: &TenantId, carrier_id: Uuid) -> anyhow::Result<Option<Wallet>> {
        let row = sqlx::query_as::<_, WalletRow>(
            "SELECT id, tenant_id, balance_cents, currency, version,
                    reserved_cents AS reserved_centavos, created_at, updated_at
             FROM payments.wallets
             WHERE tenant_id = $1 AND owner_type = 'carrier' AND owner_id = $2"
        ).bind(tenant_id.inner()).bind(carrier_id).fetch_optional(&self.pool).await?;
        Ok(row.map(Wallet::from))
    }

    async fn save_carrier_wallet(&self, w: &Wallet, carrier_id: Uuid) -> anyhow::Result<()> {
        let currency = w.currency.to_string();
        let tenant_uuid = w.tenant_id.inner();
        let rows = sqlx::query(
            r#"INSERT INTO payments.wallets
                   (id, tenant_id, owner_type, owner_id,
                    balance_cents, currency, version, reserved_cents, created_at, updated_at)
               VALUES ($1,$2,'carrier',$3,$4,$5,$6,$7,$8,$9)
               ON CONFLICT (tenant_id, owner_type, owner_id) DO UPDATE SET
                   balance_cents  = EXCLUDED.balance_cents,
                   version        = EXCLUDED.version,
                   reserved_cents = EXCLUDED.reserved_cents,
                   updated_at     = EXCLUDED.updated_at
               WHERE payments.wallets.version = $6 - 1"#
        )
        .bind(w.id).bind(tenant_uuid).bind(carrier_id)
        .bind(w.balance.amount).bind(currency)
        .bind(w.version).bind(w.reserved_centavos).bind(w.created_at).bind(w.updated_at)
        .execute(&self.pool).await?;

        if rows.rows_affected() == 0 {
            anyhow::bail!("Carrier wallet optimistic lock conflict");
        }
        Ok(())
    }

    async fn unsettled_carrier_earnings(
        &self,
        tenant_id: &TenantId,
    ) -> anyhow::Result<Vec<UnsettledCarrierEarnings>> {
        #[derive(sqlx::FromRow)]
        struct Row {
            carrier_id:           Uuid,
            wallet_id:            Uuid,
            total_margin_cents:   i64,
            total_cod_comm_cents: i64,
            delivery_count:       i64,
        }
        let rows = sqlx::query_as::<_, Row>(
            r#"SELECT
                   w.owner_id                                              AS carrier_id,
                   w.id                                                    AS wallet_id,
                   COALESCE(SUM(t.amount_cents) FILTER (
                       WHERE t.transaction_type = 'carrier_margin'), 0)   AS total_margin_cents,
                   COALESCE(SUM(t.amount_cents) FILTER (
                       WHERE t.transaction_type = 'cod_commission'), 0)   AS total_cod_comm_cents,
                   COUNT(*) FILTER (
                       WHERE t.transaction_type = 'carrier_margin')       AS delivery_count
               FROM payments.wallets w
               JOIN payments.wallet_transactions t ON t.wallet_id = w.id
               WHERE w.tenant_id   = $1
                 AND w.owner_type  = 'carrier'
                 AND t.tenant_id   = $1
                 AND t.transaction_type IN ('carrier_margin', 'cod_commission')
                 AND t.carrier_settlement_id IS NULL
               GROUP BY w.owner_id, w.id
               HAVING SUM(t.amount_cents) > 0"#
        ).bind(tenant_id.inner()).fetch_all(&self.pool).await?;

        Ok(rows.into_iter().map(|r| UnsettledCarrierEarnings {
            carrier_id:           r.carrier_id,
            wallet_id:            r.wallet_id,
            total_margin_cents:   r.total_margin_cents,
            total_cod_comm_cents: r.total_cod_comm_cents,
            delivery_count:       r.delivery_count,
        }).collect())
    }

    async fn mark_transactions_settled(
        &self,
        wallet_id:     Uuid,
        settlement_id: Uuid,
        tx_types:      &[&str],
    ) -> anyhow::Result<u64> {
        let now = chrono::Utc::now();
        let result = sqlx::query(
            r#"UPDATE payments.wallet_transactions
               SET carrier_settlement_id = $1,
                   settled_at            = $2
               WHERE wallet_id           = $3
                 AND transaction_type    = ANY($4)
                 AND carrier_settlement_id IS NULL"#
        )
        .bind(settlement_id)
        .bind(now)
        .bind(wallet_id)
        .bind(tx_types)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }
}
