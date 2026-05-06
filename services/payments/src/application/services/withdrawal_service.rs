use std::sync::Arc;
use uuid::Uuid;
use logisticos_errors::{AppError, AppResult};
use logisticos_types::{Currency, Money, TenantId};
use logisticos_events::{producer::KafkaProducer, topics, envelope::Event};

use crate::{
    domain::{
        entities::{
            Wallet, WalletTransaction, TransactionType,
            WithdrawalRequest, WithdrawalStatus,
        },
        events::{WithdrawalDisbursed, WithdrawalRejected},
        repositories::WalletRepository,
        value_objects::MIN_WITHDRAWAL_CENTS,
    },
    infrastructure::db::PgWithdrawalRequestRepository,
};

pub struct WithdrawalService {
    wallet_repo:      Arc<dyn WalletRepository>,
    withdrawal_repo:  Arc<PgWithdrawalRequestRepository>,
    kafka:            Arc<KafkaProducer>,
}

impl WithdrawalService {
    pub fn new(
        wallet_repo:     Arc<dyn WalletRepository>,
        withdrawal_repo: Arc<PgWithdrawalRequestRepository>,
        kafka:           Arc<KafkaProducer>,
    ) -> Self {
        Self { wallet_repo, withdrawal_repo, kafka }
    }

    /// Find a withdrawal request by ID or return NotFound.
    /// Also verifies the request belongs to the given tenant (returns NotFound if not, to avoid leaking IDs).
    async fn find_or_error(&self, id: Uuid, tenant_id: &TenantId) -> AppResult<WithdrawalRequest> {
        let req = self.withdrawal_repo
            .find_by_id(id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound { resource: "withdrawal_request", id: id.to_string() })?;
        if req.tenant_id != tenant_id.inner() {
            return Err(AppError::NotFound { resource: "withdrawal_request", id: id.to_string() });
        }
        Ok(req)
    }

    /// Find a wallet by ID or return NotFound.
    async fn wallet_or_error(&self, wallet_id: Uuid) -> AppResult<Wallet> {
        self.wallet_repo
            .find_by_id(wallet_id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound { resource: "wallet", id: wallet_id.to_string() })
    }

    /// Create a withdrawal request: validate minimum amount, reserve funds in wallet,
    /// persist request, and return it.
    pub async fn request(
        &self,
        tenant_id:       &TenantId,
        amount_centavos: i64,
        requested_by:    Uuid,
    ) -> AppResult<WithdrawalRequest> {
        if amount_centavos < MIN_WITHDRAWAL_CENTS {
            return Err(AppError::BusinessRule(format!(
                "Minimum withdrawal is ₱{:.2}",
                MIN_WITHDRAWAL_CENTS as f64 / 100.0
            )));
        }

        let mut wallet = self.wallet_repo
            .find_by_tenant(tenant_id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound { resource: "wallet", id: tenant_id.inner().to_string() })?;

        wallet.reserve(amount_centavos)
            .map_err(|e| AppError::BusinessRule(e.to_string()))?;

        self.wallet_repo.save_wallet(&wallet).await.map_err(AppError::Internal)?;

        let req = WithdrawalRequest::new(tenant_id.inner(), wallet.id, amount_centavos, requested_by);
        self.withdrawal_repo.insert(&req).await.map_err(AppError::Internal)?;

        tracing::info!(
            withdrawal_id = %req.id,
            tenant_id     = %tenant_id.inner(),
            amount        = amount_centavos,
            "Withdrawal request created — funds reserved",
        );

        Ok(req)
    }

    /// Approve a pending withdrawal request (finance review step).
    pub async fn approve(&self, id: Uuid, reviewed_by: Uuid, tenant_id: &TenantId) -> AppResult<WithdrawalRequest> {
        let mut req = self.find_or_error(id, tenant_id).await?;

        req.approve(reviewed_by)
            .map_err(|e| AppError::BusinessRule(e.to_string()))?;

        self.withdrawal_repo.update(&req).await.map_err(AppError::Internal)?;

        tracing::info!(withdrawal_id = %id, reviewed_by = %reviewed_by, "Withdrawal approved");

        Ok(req)
    }

    /// Disburse an approved withdrawal: debit wallet balance, clear the reservation,
    /// record ledger transaction, publish Kafka event.
    pub async fn disburse(
        &self,
        id:          Uuid,
        reviewed_by: Uuid,
        tenant_id:   &TenantId,
    ) -> AppResult<WithdrawalRequest> {
        let mut req = self.find_or_error(id, tenant_id).await?;
        let mut wallet = self.wallet_or_error(req.wallet_id).await?;

        req.disburse(reviewed_by)
            .map_err(|e| AppError::BusinessRule(e.to_string()))?;

        // Status update first: if wallet save fails, retry sees Disbursed and refuses — preventing double-debit.
        // Full atomicity requires a DB transaction (future improvement).
        self.withdrawal_repo.update(&req).await.map_err(AppError::Internal)?;

        // Debit balance and clear the prior reservation in a single version bump (domain-level).
        wallet.complete_withdrawal(req.amount_centavos)
            .map_err(|e| AppError::BusinessRule(e.to_string()))?;

        self.wallet_repo.save_wallet(&wallet).await.map_err(AppError::Internal)?;

        let ledger_tx = WalletTransaction {
            id:               Uuid::new_v4(),
            wallet_id:        wallet.id,
            tenant_id:        tenant_id.clone(),
            transaction_type: TransactionType::Withdrawal,
            amount:           Money::new(req.amount_centavos, Currency::PHP),
            reference_id:     req.id,
            description:      format!(
                "Withdrawal disbursed: ₱{:.2}",
                req.amount_centavos as f64 / 100.0
            ),
            created_at: chrono::Utc::now(),
        };
        self.wallet_repo.record_transaction(&ledger_tx).await.map_err(AppError::Internal)?;

        let event = Event::new(
            "payments",
            "wallet.withdrawal_disbursed",
            tenant_id.inner(),
            WithdrawalDisbursed {
                withdrawal_id:   req.id,
                tenant_id:       tenant_id.inner(),
                wallet_id:       req.wallet_id,
                amount_centavos: req.amount_centavos,
                disbursed_by:    reviewed_by,
            },
        );
        self.kafka
            .publish_event(topics::WALLET_WITHDRAWAL_DISBURSED, &event)
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(
            withdrawal_id = %id,
            reviewed_by   = %reviewed_by,
            amount        = req.amount_centavos,
            "Withdrawal disbursed — wallet debited",
        );

        Ok(req)
    }

    /// Reject a pending or approved withdrawal: release wallet reservation,
    /// publish Kafka event.
    pub async fn reject(
        &self,
        id:          Uuid,
        reviewed_by: Uuid,
        note:        String,
        tenant_id:   &TenantId,
    ) -> AppResult<WithdrawalRequest> {
        let mut req = self.find_or_error(id, tenant_id).await?;
        let mut wallet = self.wallet_or_error(req.wallet_id).await?;

        req.reject(reviewed_by, note)
            .map_err(|e| AppError::BusinessRule(e.to_string()))?;

        // Release the reservation so funds become available again.
        wallet.release_reservation(req.amount_centavos);
        self.wallet_repo.save_wallet(&wallet).await.map_err(AppError::Internal)?;

        self.withdrawal_repo.update(&req).await.map_err(AppError::Internal)?;

        let event = Event::new(
            "payments",
            "wallet.withdrawal_rejected",
            tenant_id.inner(),
            WithdrawalRejected {
                withdrawal_id:   req.id,
                tenant_id:       tenant_id.inner(),
                wallet_id:       req.wallet_id,
                amount_centavos: req.amount_centavos,
                rejected_by:     reviewed_by,
                review_note:     req.review_note.clone(),
            },
        );
        self.kafka
            .publish_event(topics::WALLET_WITHDRAWAL_REJECTED, &event)
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(
            withdrawal_id = %id,
            reviewed_by   = %reviewed_by,
            "Withdrawal rejected — reservation released",
        );

        Ok(req)
    }

    /// List all pending withdrawal requests for a tenant.
    pub async fn list_pending(&self, tenant_id: Uuid) -> AppResult<Vec<WithdrawalRequest>> {
        self.withdrawal_repo
            .list_by_status(tenant_id, WithdrawalStatus::Pending)
            .await
            .map_err(AppError::Internal)
    }
}
