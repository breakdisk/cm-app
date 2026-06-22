//! Carrier settlement run — aggregates all unsettled gig-driver delivery margins
//! and COD commissions per carrier, creates a settlement record, and auto-generates
//! a withdrawal request so the carrier can be paid out.
//!
//! Triggered by:
//!   - Weekly cron (every Saturday at midnight UTC, settling the past 7 days)
//!   - Admin manual trigger: POST /v1/admin/carrier-settlements/run

use std::sync::Arc;
use chrono::{Datelike, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use logisticos_errors::{AppError, AppResult};
use logisticos_types::TenantId;

use crate::domain::repositories::WalletRepository;
use crate::infrastructure::db::PgWithdrawalRequestRepository;

/// Outcome of one carrier's settlement within a run.
#[derive(Debug, Serialize)]
pub struct CarrierSettlementOutcome {
    pub carrier_id:             Uuid,
    pub settlement_id:          Uuid,
    pub total_margin_cents:     i64,
    pub total_cod_comm_cents:   i64,
    pub total_cents:            i64,
    pub delivery_count:         i64,
    pub withdrawal_request_id:  Option<Uuid>,
}

pub struct CarrierSettlementService {
    pool:            PgPool,
    wallet_repo:     Arc<dyn WalletRepository>,
    withdrawal_repo: Arc<PgWithdrawalRequestRepository>,
}

impl CarrierSettlementService {
    pub fn new(
        pool:            PgPool,
        wallet_repo:     Arc<dyn WalletRepository>,
        withdrawal_repo: Arc<PgWithdrawalRequestRepository>,
    ) -> Self {
        Self { pool, wallet_repo, withdrawal_repo }
    }

    /// Run settlement for all carriers in a tenant with unsettled earnings.
    /// `period_end` is inclusive — defaults to yesterday (UTC).
    pub async fn run(
        &self,
        tenant_id:  &TenantId,
        period_end: Option<NaiveDate>,
        actor_id:   Uuid,
    ) -> AppResult<Vec<CarrierSettlementOutcome>> {
        let period_end = period_end.unwrap_or_else(|| (Utc::now() - Duration::days(1)).date_naive());
        let period_start = period_end - Duration::days(6);

        let unsettled = self.wallet_repo
            .unsettled_carrier_earnings(tenant_id)
            .await
            .map_err(AppError::Internal)?;

        if unsettled.is_empty() {
            tracing::info!(tenant_id = %tenant_id, "Carrier settlement run: no unsettled earnings");
            return Ok(vec![]);
        }

        let mut outcomes = Vec::new();

        for earning in unsettled {
            let total = earning.total_margin_cents + earning.total_cod_comm_cents;
            if total <= 0 { continue; }

            // Persist the settlement run record.
            let settlement_id = Uuid::new_v4();
            sqlx::query(
                r#"INSERT INTO payments.carrier_settlement_runs
                       (id, tenant_id, carrier_id, period_start, period_end,
                        total_margin_cents, total_cod_commission_cents,
                        total_cents, delivery_count, status)
                   VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,'pending')
                   ON CONFLICT (tenant_id, carrier_id, period_start, period_end)
                   DO NOTHING"#
            )
            .bind(settlement_id)
            .bind(tenant_id.inner())
            .bind(earning.carrier_id)
            .bind(period_start)
            .bind(period_end)
            .bind(earning.total_margin_cents)
            .bind(earning.total_cod_comm_cents)
            .bind(total)
            .bind(earning.delivery_count as i32)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;

            // Tag all unsettled wallet transactions for this carrier.
            self.wallet_repo
                .mark_transactions_settled(
                    earning.wallet_id,
                    settlement_id,
                    &["carrier_margin", "cod_commission"],
                )
                .await
                .map_err(AppError::Internal)?;

            // Auto-create a withdrawal request so finance can disburse.
            let withdrawal = crate::domain::entities::WithdrawalRequest::new(
                tenant_id.inner(),
                earning.wallet_id,
                total,
                actor_id,
                None, // carrier_email — populated by carrier service if needed
            );
            self.withdrawal_repo
                .insert(&withdrawal)
                .await
                .map_err(|e| AppError::Internal(e))?;

            // Link the withdrawal request back to the settlement run.
            sqlx::query(
                "UPDATE payments.carrier_settlement_runs
                 SET withdrawal_request_id = $1, settled_at = NOW()
                 WHERE id = $2"
            )
            .bind(withdrawal.id)
            .bind(settlement_id)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;

            tracing::info!(
                carrier_id    = %earning.carrier_id,
                settlement_id = %settlement_id,
                total_cents   = total,
                deliveries    = earning.delivery_count,
                "Carrier settlement run: settlement created"
            );

            outcomes.push(CarrierSettlementOutcome {
                carrier_id:           earning.carrier_id,
                settlement_id,
                total_margin_cents:   earning.total_margin_cents,
                total_cod_comm_cents: earning.total_cod_comm_cents,
                total_cents:          total,
                delivery_count:       earning.delivery_count,
                withdrawal_request_id: Some(withdrawal.id),
            });
        }

        Ok(outcomes)
    }

    /// List all settlement runs for a tenant, newest first.
    pub async fn list(&self, tenant_id: &TenantId) -> AppResult<Vec<SettlementRunRow>> {
        let rows = sqlx::query_as::<_, SettlementRunRow>(
            r#"SELECT id, carrier_id, period_start, period_end,
                      total_margin_cents, total_cod_commission_cents,
                      total_cents, delivery_count, status,
                      withdrawal_request_id, created_at, settled_at
               FROM payments.carrier_settlement_runs
               WHERE tenant_id = $1
               ORDER BY created_at DESC
               LIMIT 200"#
        )
        .bind(tenant_id.inner())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
        Ok(rows)
    }
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct SettlementRunRow {
    pub id:                         Uuid,
    pub carrier_id:                 Uuid,
    pub period_start:               NaiveDate,
    pub period_end:                 NaiveDate,
    pub total_margin_cents:         i64,
    pub total_cod_commission_cents: i64,
    pub total_cents:                i64,
    pub delivery_count:             i32,
    pub status:                     String,
    pub withdrawal_request_id:      Option<Uuid>,
    pub created_at:                 chrono::DateTime<Utc>,
    pub settled_at:                 Option<chrono::DateTime<Utc>>,
}
