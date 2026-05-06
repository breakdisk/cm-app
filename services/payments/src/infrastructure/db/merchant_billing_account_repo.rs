use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;
use crate::domain::{
    entities::MerchantBillingAccount,
    repositories::MerchantBillingAccountRepository,
};

pub struct PgMerchantBillingAccountRepository { pool: PgPool }
impl PgMerchantBillingAccountRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[derive(sqlx::FromRow)]
struct AccountRow {
    id:                          Uuid,
    tenant_id:                   Uuid,
    merchant_id:                 Uuid,
    base_rate_override_centavos: Option<i64>,
    payment_terms_days:          i16,
    credit_limit_centavos:       i64,
    tin:                         Option<String>,
    vat_registered:              bool,
    billing_email:               String,
    invoice_channel:             String,
    bank_name:                   Option<String>,
    bank_account_number:         Option<String>,
    bank_account_name:           Option<String>,
    created_at:                  chrono::DateTime<chrono::Utc>,
    updated_at:                  chrono::DateTime<chrono::Utc>,
}

impl From<AccountRow> for MerchantBillingAccount {
    fn from(r: AccountRow) -> Self {
        MerchantBillingAccount {
            id: r.id,
            tenant_id: r.tenant_id,
            merchant_id: r.merchant_id,
            base_rate_override_centavos: r.base_rate_override_centavos,
            payment_terms_days: r.payment_terms_days,
            credit_limit_centavos: r.credit_limit_centavos,
            tin: r.tin,
            vat_registered: r.vat_registered,
            billing_email: r.billing_email,
            invoice_channel: r.invoice_channel,
            bank_name: r.bank_name,
            bank_account_number: r.bank_account_number,
            bank_account_name: r.bank_account_name,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

const SELECT: &str = "SELECT id, tenant_id, merchant_id, base_rate_override_centavos,
    payment_terms_days, credit_limit_centavos, tin, vat_registered, billing_email,
    invoice_channel, bank_name, bank_account_number, bank_account_name,
    created_at, updated_at FROM payments.merchant_billing_accounts";

#[async_trait]
impl MerchantBillingAccountRepository for PgMerchantBillingAccountRepository {
    async fn find_by_merchant(&self, merchant_id: Uuid) -> anyhow::Result<Option<MerchantBillingAccount>> {
        let row = sqlx::query_as::<_, AccountRow>(
            &format!("{SELECT} WHERE merchant_id = $1")
        ).bind(merchant_id).fetch_optional(&self.pool).await?;
        Ok(row.map(MerchantBillingAccount::from))
    }

    async fn upsert(&self, a: &MerchantBillingAccount) -> anyhow::Result<()> {
        sqlx::query(
            r#"INSERT INTO payments.merchant_billing_accounts
                (id, tenant_id, merchant_id, base_rate_override_centavos,
                 payment_terms_days, credit_limit_centavos, tin, vat_registered,
                 billing_email, invoice_channel, bank_name, bank_account_number,
                 bank_account_name, created_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
               ON CONFLICT (merchant_id) DO UPDATE SET
                 base_rate_override_centavos = EXCLUDED.base_rate_override_centavos,
                 payment_terms_days          = EXCLUDED.payment_terms_days,
                 credit_limit_centavos       = EXCLUDED.credit_limit_centavos,
                 tin                         = EXCLUDED.tin,
                 vat_registered              = EXCLUDED.vat_registered,
                 billing_email               = EXCLUDED.billing_email,
                 invoice_channel             = EXCLUDED.invoice_channel,
                 bank_name                   = EXCLUDED.bank_name,
                 bank_account_number         = EXCLUDED.bank_account_number,
                 bank_account_name           = EXCLUDED.bank_account_name,
                 updated_at                  = EXCLUDED.updated_at"#
        )
        .bind(a.id).bind(a.tenant_id).bind(a.merchant_id)
        .bind(a.base_rate_override_centavos).bind(a.payment_terms_days)
        .bind(a.credit_limit_centavos).bind(a.tin.as_deref())
        .bind(a.vat_registered).bind(&a.billing_email).bind(&a.invoice_channel)
        .bind(a.bank_name.as_deref()).bind(a.bank_account_number.as_deref())
        .bind(a.bank_account_name.as_deref()).bind(a.created_at).bind(a.updated_at)
        .execute(&self.pool).await?;
        Ok(())
    }
}
