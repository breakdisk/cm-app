use std::sync::Arc;
use logisticos_errors::{AppError, AppResult};
use logisticos_types::{Currency, TenantId};

use crate::{
    application::commands::WalletSummary,
    domain::{
        entities::{Wallet, WalletTransaction},
        repositories::WalletRepository,
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
            wallet_id:     wallet.id,
            tenant_id:     tenant_id.inner(),
            balance_php:   wallet.balance.amount as f64 / 100.0,
            reserved_php:  wallet.reserved_centavos as f64 / 100.0,
            available_php: wallet.available_centavos() as f64 / 100.0,
            currency:      wallet.currency.to_string(),
            updated_at:    wallet.updated_at.to_rfc3339(),
        })
    }

    pub async fn list_transactions(&self, tenant_id: &TenantId, limit: u32) -> AppResult<Vec<WalletTransaction>> {
        let wallet = self.get_or_create(tenant_id).await?;
        self.wallet_repo.list_transactions(wallet.id, limit).await.map_err(AppError::Internal)
    }
}
