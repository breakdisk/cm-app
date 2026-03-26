use std::sync::Arc;
use logisticos_errors::{AppError, AppResult};
use logisticos_types::{Money, Currency, TenantId};

use crate::{
    application::commands::{RequestWithdrawalCommand, WalletSummary},
    domain::{
        entities::{Wallet, WalletTransaction, TransactionType},
        repositories::WalletRepository,
        value_objects::MIN_WITHDRAWAL_CENTS,
    },
};

pub struct WalletService {
    wallet_repo: Arc<dyn WalletRepository>,
}

impl WalletService {
    pub fn new(wallet_repo: Arc<dyn WalletRepository>) -> Self {
        Self { wallet_repo }
    }

    pub async fn get_or_create(&self, tenant_id: &TenantId) -> AppResult<Wallet> {
        if let Some(wallet) = self.wallet_repo.find_by_tenant(tenant_id).await.map_err(AppError::Internal)? {
            return Ok(wallet);
        }

        // Auto-provision wallet on first access
        let wallet = Wallet::new(tenant_id.clone(), Currency::PHP);
        self.wallet_repo.save_wallet(&wallet).await.map_err(AppError::Internal)?;
        tracing::info!(tenant_id = %tenant_id, wallet_id = %wallet.id, "Wallet created");
        Ok(wallet)
    }

    pub async fn summary(&self, tenant_id: &TenantId) -> AppResult<WalletSummary> {
        let wallet = self.get_or_create(tenant_id).await?;
        Ok(WalletSummary {
            wallet_id: wallet.id,
            balance_cents: wallet.balance.amount,
            currency: format!("{:?}", wallet.currency),
            updated_at: wallet.updated_at.to_rfc3339(),
        })
    }

    pub async fn request_withdrawal(
        &self,
        tenant_id: &TenantId,
        cmd: RequestWithdrawalCommand,
    ) -> AppResult<()> {
        // Business rule: minimum withdrawal amount
        if cmd.amount_cents < MIN_WITHDRAWAL_CENTS {
            return Err(AppError::BusinessRule(format!(
                "Minimum withdrawal is ₱{:.2}",
                MIN_WITHDRAWAL_CENTS as f64 / 100.0
            )));
        }

        let mut wallet = self.get_or_create(tenant_id).await?;

        wallet.debit(Money::new(cmd.amount_cents, Currency::PHP))
            .map_err(|e| AppError::BusinessRule(e.to_string()))?;

        self.wallet_repo.save_wallet(&wallet).await.map_err(AppError::Internal)?;

        let tx = WalletTransaction {
            id: uuid::Uuid::new_v4(),
            wallet_id: wallet.id,
            tenant_id: tenant_id.clone(),
            transaction_type: TransactionType::Withdrawal,
            amount: Money::new(cmd.amount_cents, Currency::PHP),
            reference_id: cmd.bank_account_id,
            description: format!("Withdrawal to bank account: ₱{:.2}", cmd.amount_cents as f64 / 100.0),
            created_at: chrono::Utc::now(),
        };
        self.wallet_repo.record_transaction(&tx).await.map_err(AppError::Internal)?;

        // In production: trigger PayMongo/GCash payout API call here
        tracing::info!(
            tenant_id = %tenant_id,
            amount = %cmd.amount_cents,
            bank_account = %cmd.bank_account_id,
            "Withdrawal requested"
        );
        Ok(())
    }

    pub async fn list_transactions(&self, tenant_id: &TenantId, limit: u32) -> AppResult<Vec<WalletTransaction>> {
        let wallet = self.get_or_create(tenant_id).await?;
        self.wallet_repo.list_transactions(wallet.id, limit).await.map_err(AppError::Internal)
    }
}
