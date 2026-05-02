//! CodRemittanceService — groups COD collections into merchant-scoped
//! remittance batches and credits the merchant wallet when finance confirms
//! the batch has been paid.
//!
//! Lifecycle:
//! 1. `create_batch(tenant, merchant, cutoff_date)` — enumerates all
//!    collected-but-unbatched COD rows up to end-of-day `cutoff_date` UTC,
//!    sums them, computes platform fee, creates a `Created` batch and flips
//!    member rows `collected` → `in_batch`.
//! 2. `confirm_batch(tenant, batch_id)` — finance has verified the cash;
//!    member rows flip `in_batch` → `remitted`, merchant wallet is credited
//!    **net** (gross − fee), and a `cod.remitted` event is emitted.
//!
//! Idempotency: confirming a `Paid` batch is a noop; confirming a `Failed`
//! batch errors.

use std::sync::Arc;
use chrono::{Duration, NaiveTime, TimeZone, Utc};
use logisticos_errors::{AppError, AppResult};
use logisticos_events::{envelope::Event, producer::KafkaProducer, topics};
use logisticos_types::{MerchantId, Money, TenantId};

use crate::{
    application::commands::{ConfirmCodBatchCommand, CreateCodBatchCommand},
    domain::{
        entities::{CodBatchStatus, CodRemittanceBatch, WalletTransaction},
        events::CodRemitted,
        repositories::{
            CodRemittanceBatchRepository, CodRepository, WalletRepository,
        },
    },
};

pub struct CodRemittanceService {
    cod_repo:      Arc<dyn CodRepository>,
    batch_repo:    Arc<dyn CodRemittanceBatchRepository>,
    wallet_repo:   Arc<dyn WalletRepository>,
    kafka:         Arc<KafkaProducer>,
}

impl CodRemittanceService {
    pub fn new(
        cod_repo:    Arc<dyn CodRepository>,
        batch_repo:  Arc<dyn CodRemittanceBatchRepository>,
        wallet_repo: Arc<dyn WalletRepository>,
        kafka:       Arc<KafkaProducer>,
    ) -> Self {
        Self { cod_repo, batch_repo, wallet_repo, kafka }
    }

    /// Create a Created-status batch for (tenant, merchant) up to cutoff_date.
    ///
    /// Errors with `BusinessRule("NO_UNBATCHED_COD")` if there is nothing to
    /// batch — caller (cron or ops) decides whether to treat this as benign.
    pub async fn create_batch(
        &self,
        cmd: CreateCodBatchCommand,
    ) -> AppResult<CodRemittanceBatch> {
        let tenant_id   = TenantId::from_uuid(cmd.tenant_id);
        let merchant_id = MerchantId::from_uuid(cmd.merchant_id);

        // End of cutoff_date UTC, exclusive — cutoff + 1 day @ 00:00.
        let cutoff_utc = Utc.from_utc_datetime(
            &(cmd.cutoff_date + Duration::days(1)).and_time(NaiveTime::MIN),
        );

        let rows = self.cod_repo
            .list_unbatched_for_merchant(&tenant_id, &merchant_id, cutoff_utc)
            .await
            .map_err(AppError::Internal)?;

        if rows.is_empty() {
            return Err(AppError::BusinessRule("NO_UNBATCHED_COD".into()));
        }

        // Enforce consistent currency across the batch — mixing PHP and USD
        // in a single wallet credit makes no sense.
        let currency = rows[0].amount.currency;
        if rows.iter().any(|r| r.amount.currency != currency) {
            return Err(AppError::BusinessRule(
                "Mixed currencies in COD batch are not supported".into(),
            ));
        }

        let gross_cents: i64 = rows.iter().map(|r| r.amount.amount).sum();
        let fee_cents:   i64 = rows.iter().map(|r| r.platform_fee().amount).sum();

        let batch = CodRemittanceBatch::new(
            tenant_id.clone(),
            merchant_id.clone(),
            cmd.cutoff_date,
            currency,
            rows.len() as i32,
            gross_cents,
            fee_cents,
        );

        // Persist batch first so the FK target exists when we flip cod rows.
        self.batch_repo.save(&batch).await.map_err(AppError::Internal)?;

        let cod_ids: Vec<_> = rows.iter().map(|r| r.id).collect();
        let updated = self.cod_repo
            .assign_to_batch(&tenant_id, &cod_ids, batch.id)
            .await
            .map_err(AppError::Internal)?;

        if updated as usize != rows.len() {
            // Another concurrent batch picked some of these up between our
            // SELECT and UPDATE — surface this so ops can investigate.
            tracing::warn!(
                batch_id = %batch.id,
                expected = rows.len(),
                updated  = updated,
                "Concurrent COD batching detected — some rows not assigned",
            );
            return Err(AppError::Conflict(
                "Concurrent batch update — retry".into(),
            ));
        }

        tracing::info!(
            batch_id     = %batch.id,
            merchant_id  = %merchant_id,
            cod_count    = batch.cod_count,
            gross_cents  = batch.gross_cents,
            net_cents    = batch.net_cents,
            cutoff       = %cmd.cutoff_date,
            "COD remittance batch created",
        );
        Ok(batch)
    }

    /// Sweep all (tenant, merchant) pairs that have unbatched COD up to `cutoff`
    /// and create a remittance batch for each. Failures per-merchant are non-fatal —
    /// a warn is logged and the next nightly run will retry.
    ///
    /// Designed to be called from a Tokio interval task in bootstrap.
    pub async fn run_daily_batching(&self, cutoff: chrono::DateTime<chrono::Utc>) -> anyhow::Result<()> {
        let pairs = self.cod_repo
            .distinct_merchants_with_unbatched_cod(cutoff)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to query unbatched COD merchants: {e}"))?;

        tracing::info!(merchant_count = pairs.len(), cutoff = %cutoff, "COD daily batching run started");

        for (tenant_id, merchant_id) in pairs {
            let cmd = CreateCodBatchCommand {
                tenant_id,
                merchant_id,
                cutoff_date: cutoff.date_naive(),
            };
            match self.create_batch(cmd).await {
                Ok(batch) => tracing::info!(
                    batch_id    = %batch.id,
                    merchant_id = %merchant_id,
                    gross_cents = batch.gross_cents,
                    "COD batch created by nightly cron"
                ),
                Err(logisticos_errors::AppError::BusinessRule(ref msg)) if msg == "NO_UNBATCHED_COD" => {
                    // Race: another process already batched — benign
                }
                Err(e) => tracing::warn!(
                    merchant_id = %merchant_id,
                    err         = %e,
                    "COD batch creation failed — will retry next run"
                ),
            }
        }

        Ok(())
    }

    /// Confirm that finance has received/verified the batch's cash.
    ///
    /// Idempotent: confirming an already-Paid batch returns it unchanged.
    pub async fn confirm_batch(
        &self,
        cmd: ConfirmCodBatchCommand,
    ) -> AppResult<CodRemittanceBatch> {
        let tenant_id = TenantId::from_uuid(cmd.tenant_id);

        let mut batch = self.batch_repo
            .find_by_id(cmd.batch_id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound {
                resource: "CodRemittanceBatch",
                id: cmd.batch_id.to_string(),
            })?;

        if batch.tenant_id != tenant_id {
            return Err(AppError::Forbidden {
                resource: format!("CodRemittanceBatch/{}", batch.id),
            });
        }

        match batch.status {
            CodBatchStatus::Paid => {
                tracing::info!(
                    batch_id = %batch.id,
                    "Batch already paid — returning"
                );
                return Ok(batch);
            }
            CodBatchStatus::Failed => {
                return Err(AppError::BusinessRule(
                    "Cannot confirm a failed batch".into(),
                ));
            }
            CodBatchStatus::Created => {}
        }

        batch.mark_paid().map_err(|e| AppError::BusinessRule(e.to_string()))?;

        // Flip COD rows in_batch → remitted.
        let flipped = self.cod_repo
            .mark_batch_remitted(&tenant_id, batch.id)
            .await
            .map_err(AppError::Internal)?;
        if flipped as i64 != batch.cod_count as i64 {
            tracing::warn!(
                batch_id = %batch.id,
                expected = batch.cod_count,
                flipped  = flipped,
                "COD row count mismatch on remittance — audit required",
            );
        }

        // Credit merchant wallet net-of-fee.
        let mut wallet = self.wallet_repo
            .find_by_tenant(&tenant_id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound {
                resource: "Wallet",
                id: tenant_id.inner().to_string(),
            })?;

        let net_money = Money::new(batch.net_cents, batch.currency);
        wallet.credit(net_money)
            .map_err(|e| AppError::BusinessRule(e.to_string()))?;
        self.wallet_repo.save_wallet(&wallet).await.map_err(AppError::Internal)?;

        // Single ledger entry per batch — reference_id = batch.id.
        let tx = WalletTransaction::cod_credit(
            wallet.id,
            tenant_id.clone(),
            net_money,
            batch.id,
        );
        self.wallet_repo.record_transaction(&tx).await.map_err(AppError::Internal)?;

        // Persist the now-Paid batch.
        self.batch_repo.save(&batch).await.map_err(AppError::Internal)?;

        // Emit cod.remitted.
        let remitted_at = batch.paid_at.unwrap_or_else(Utc::now);
        let event_payload = CodRemitted {
            batch_id:           batch.id,
            tenant_id:          tenant_id.inner(),
            merchant_id:        batch.merchant_id.inner(),
            cutoff_date:        batch.cutoff_date,
            cod_count:          batch.cod_count,
            gross_cents:        batch.gross_cents,
            platform_fee_cents: batch.platform_fee_cents,
            net_credit_cents:   batch.net_cents,
            remitted_at,
        };
        let event = Event::new("payments", "cod.remitted", tenant_id.inner(), event_payload);
        self.kafka
            .publish_event(topics::COD_REMITTED, &event)
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(
            batch_id     = %batch.id,
            merchant_id  = %batch.merchant_id,
            cod_count    = batch.cod_count,
            net_credit   = batch.net_cents,
            "COD remittance batch paid; merchant wallet credited",
        );
        Ok(batch)
    }
}

