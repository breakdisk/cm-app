use std::sync::Arc;
use logisticos_errors::{AppError, AppResult};
use logisticos_types::{Money, Currency, TenantId};
use logisticos_events::{producer::KafkaProducer, topics, envelope::Event};

use crate::{
    application::commands::ReconcileCodCommand,
    domain::{
        entities::{CodCollection, WalletTransaction},
        events::CodReconciled,
        repositories::{CodRepository, WalletRepository},
    },
};

pub struct CodService {
    cod_repo: Arc<dyn CodRepository>,
    wallet_repo: Arc<dyn WalletRepository>,
    kafka: Arc<KafkaProducer>,
}

impl CodService {
    pub fn new(
        cod_repo: Arc<dyn CodRepository>,
        wallet_repo: Arc<dyn WalletRepository>,
        kafka: Arc<KafkaProducer>,
    ) -> Self {
        Self { cod_repo, wallet_repo, kafka }
    }

    /// Called when a POD is submitted with COD amount collected.
    /// Creates the COD record and immediately credits the merchant wallet.
    pub async fn reconcile_cod(&self, tenant_id: &TenantId, cmd: ReconcileCodCommand) -> AppResult<()> {
        // Idempotency: one COD collection per shipment
        if self.cod_repo.find_by_shipment(cmd.shipment_id).await.map_err(AppError::Internal)?.is_some() {
            tracing::warn!(shipment_id = %cmd.shipment_id, "COD already reconciled — skipping");
            return Ok(());
        }

        if cmd.amount_cents <= 0 {
            return Err(AppError::BusinessRule("COD amount must be positive".into()));
        }

        let amount = Money::new(cmd.amount_cents, Currency::PHP);

        let mut cod = CodCollection::new(
            tenant_id.clone(),
            cmd.shipment_id,
            cmd.driver_id,
            cmd.pod_id,
            amount,
        );
        let platform_fee = cod.platform_fee();
        let merchant_credit = cod.merchant_credit();
        let cod_id = cod.id;

        cod.mark_remitted(); // Auto-remit in this simplified flow — real impl has batch remittance
        self.cod_repo.save(&cod).await.map_err(AppError::Internal)?;

        // Load or create merchant wallet
        let mut wallet = self.wallet_repo.find_by_tenant(tenant_id).await.map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound { resource: "Wallet", id: tenant_id.inner().to_string() })?;

        // Deduct platform fee, credit net amount
        wallet.credit(merchant_credit).map_err(|e| AppError::BusinessRule(e.to_string()))?;

        self.wallet_repo.save_wallet(&wallet).await.map_err(AppError::Internal)?;

        // Record both ledger entries
        let credit_tx = WalletTransaction::cod_credit(wallet.id, tenant_id.clone(), merchant_credit, cod_id);
        let fee_tx    = WalletTransaction::platform_fee_debit(wallet.id, tenant_id.clone(), platform_fee, cod_id);
        self.wallet_repo.record_transaction(&credit_tx).await.map_err(AppError::Internal)?;
        self.wallet_repo.record_transaction(&fee_tx).await.map_err(AppError::Internal)?;

        // Events for analytics and engagement (customer confirmation email)
        let event = Event::new("payments", "cod.reconciled", tenant_id.inner(), CodReconciled {
            cod_id,
            shipment_id: cmd.shipment_id,
            tenant_id: tenant_id.inner(),
            amount_cents: cmd.amount_cents,
            merchant_credit_cents: merchant_credit.amount,
            platform_fee_cents: platform_fee.amount,
        });
        self.kafka.publish_event(topics::COD_COLLECTED, &event).await.map_err(AppError::Internal)?;

        tracing::info!(
            cod_id = %cod_id,
            shipment_id = %cmd.shipment_id,
            amount = %cmd.amount_cents,
            merchant_credit = %merchant_credit.amount,
            "COD reconciled"
        );
        Ok(())
    }
}
