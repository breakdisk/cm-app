//! BillingAggregationService — turns a month of delivered shipments into a
//! merchant invoice.
//!
//! # Flow
//! 1. Caller invokes `run_monthly(cmd)` (cron or ops-triggered).
//! 2. Service checks `billing_runs` for an existing completed run — if found,
//!    returns it (idempotent).
//! 3. Service calls `MerchantBillingSource::list_delivered(tenant, merchant, from, to)`
//!    to fetch per-shipment fee breakdowns from order-intake.
//! 4. Breakdowns are flattened into per-AWB `AwbChargeInput` rows
//!    (base_freight / fuel_surcharge / insurance_fee) and passed to
//!    `InvoiceService::generate`.
//! 5. The resulting invoice id, shipment count and total are persisted in
//!    `payments.billing_runs` so subsequent calls for the same period are noops.

use std::sync::Arc;
use chrono::{Datelike, Duration, NaiveDate, NaiveTime, TimeZone, Utc};
use logisticos_errors::{AppError, AppResult};
use logisticos_types::{MerchantId, TenantId};

use crate::{
    application::{
        commands::{AwbChargeInput, GenerateInvoiceCommand, RunBillingCommand},
        services::InvoiceService,
    },
    domain::{
        entities::Invoice,
        repositories::{BillingRunRecord, BillingRunRepository, MerchantBillingSource},
    },
};

/// Outcome of a billing run — either a freshly issued invoice or a
/// previously issued one returned because the period was already run.
#[derive(Debug, Clone, Copy)]
pub enum BillingRunOutcome {
    Issued,
    AlreadyExisted,
    NoShipments,
}

pub struct BillingAggregationService {
    runs:            Arc<dyn BillingRunRepository>,
    billing_source:  Arc<dyn MerchantBillingSource>,
    invoice_service: Arc<InvoiceService>,
}

impl BillingAggregationService {
    pub fn new(
        runs:            Arc<dyn BillingRunRepository>,
        billing_source:  Arc<dyn MerchantBillingSource>,
        invoice_service: Arc<InvoiceService>,
    ) -> Self {
        Self { runs, billing_source, invoice_service }
    }

    /// Run billing for one (tenant, merchant, year, month).
    ///
    /// Returns the (possibly pre-existing) billing run plus an outcome tag.
    pub async fn run_monthly(
        &self,
        cmd: RunBillingCommand,
    ) -> AppResult<(BillingRunRecord, BillingRunOutcome)> {
        if !(1..=12).contains(&cmd.month) {
            return Err(AppError::Validation(format!("invalid month: {}", cmd.month)));
        }

        let tenant_id   = TenantId::from_uuid(cmd.tenant_id);
        let merchant_id = MerchantId::from_uuid(cmd.merchant_id);

        // ── Compute period window [from, to) in UTC ───────────────────────────
        let period_start = NaiveDate::from_ymd_opt(cmd.year, cmd.month, 1)
            .ok_or_else(|| AppError::Validation(format!("invalid year/month: {}-{}", cmd.year, cmd.month)))?;
        let period_end = end_of_month(period_start);
        let from_utc   = Utc.from_utc_datetime(&period_start.and_time(NaiveTime::MIN));
        let to_utc     = Utc.from_utc_datetime(
            &(period_end + Duration::days(1)).and_time(NaiveTime::MIN),
        );

        // ── Idempotency: has this period already been billed? ─────────────────
        if let Some(existing) = self.runs
            .find_for_period(&tenant_id, &merchant_id, period_start, period_end)
            .await
            .map_err(AppError::Internal)?
        {
            tracing::info!(
                run_id      = %existing.id,
                merchant_id = %merchant_id,
                period      = format!("{}-{:02}", cmd.year, cmd.month),
                "Billing run already exists for period — returning existing"
            );
            return Ok((existing, BillingRunOutcome::AlreadyExisted));
        }

        // ── Pull delivered shipments from order-intake ────────────────────────
        let shipments = self.billing_source
            .list_delivered(cmd.tenant_id, cmd.merchant_id, from_utc, to_utc)
            .await
            .map_err(AppError::Internal)?;

        if shipments.is_empty() {
            // Record an empty run so re-invocations stay noops.
            let run = BillingRunRecord {
                id:             uuid::Uuid::new_v4(),
                tenant_id:      tenant_id.clone(),
                merchant_id:    merchant_id.clone(),
                period_start,
                period_end,
                invoice_id:     None,
                shipment_count: 0,
                total_cents:    0,
                created_at:     Utc::now(),
            };
            self.runs.save(&run).await.map_err(AppError::Internal)?;
            tracing::info!(
                run_id      = %run.id,
                merchant_id = %merchant_id,
                period      = format!("{}-{:02}", cmd.year, cmd.month),
                "Billing run: no delivered shipments in period",
            );
            return Ok((run, BillingRunOutcome::NoShipments));
        }

        // ── Flatten shipments → per-AWB charge inputs ─────────────────────────
        let mut charges = Vec::with_capacity(shipments.len() * 3);
        let mut total_cents: i64 = 0;
        for s in &shipments {
            total_cents = total_cents.saturating_add(s.total_cents);
            if s.base_freight_cents > 0 {
                charges.push(AwbChargeInput {
                    awb:              s.awb.clone(),
                    charge_type:      "base_freight".into(),
                    description:      "Base freight".into(),
                    quantity:         1,
                    unit_price_cents: s.base_freight_cents,
                    discount_cents:   None,
                });
            }
            if s.fuel_surcharge_cents > 0 {
                charges.push(AwbChargeInput {
                    awb:              s.awb.clone(),
                    charge_type:      "fuel_surcharge".into(),
                    description:      "Fuel surcharge".into(),
                    quantity:         1,
                    unit_price_cents: s.fuel_surcharge_cents,
                    discount_cents:   None,
                });
            }
            if s.insurance_cents > 0 {
                charges.push(AwbChargeInput {
                    awb:              s.awb.clone(),
                    charge_type:      "insurance_fee".into(),
                    description:      "Shipment insurance".into(),
                    quantity:         1,
                    unit_price_cents: s.insurance_cents,
                    discount_cents:   None,
                });
            }
        }

        if charges.is_empty() {
            return Err(AppError::BusinessRule(format!(
                "Billing run for {} shipments produced zero charges",
                shipments.len()
            )));
        }

        // ── Issue the invoice via existing InvoiceService ─────────────────────
        let invoice: Invoice = self.invoice_service
            .generate(
                &tenant_id,
                GenerateInvoiceCommand {
                    merchant_id:          cmd.merchant_id,
                    merchant_email:       cmd.merchant_email.clone(),
                    tenant_code:          cmd.tenant_code.clone(),
                    billing_period_year:  cmd.year,
                    billing_period_month: cmd.month,
                    charges,
                },
            )
            .await?;

        // ── Persist the run record for idempotency + audit ────────────────────
        let run = BillingRunRecord {
            id:             uuid::Uuid::new_v4(),
            tenant_id:      tenant_id.clone(),
            merchant_id:    merchant_id.clone(),
            period_start,
            period_end,
            invoice_id:     Some(invoice.id.clone()),
            shipment_count: shipments.len() as i32,
            total_cents,
            created_at:     Utc::now(),
        };
        self.runs.save(&run).await.map_err(AppError::Internal)?;

        tracing::info!(
            run_id         = %run.id,
            invoice_id     = %invoice.id,
            merchant_id    = %merchant_id,
            shipment_count = shipments.len(),
            total_cents,
            period         = format!("{}-{:02}", cmd.year, cmd.month),
            "Billing run issued invoice",
        );
        Ok((run, BillingRunOutcome::Issued))
    }
}

fn end_of_month(start: NaiveDate) -> NaiveDate {
    let (y, m) = (start.year(), start.month());
    let first_of_next = if m == 12 {
        NaiveDate::from_ymd_opt(y + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(y, m + 1, 1)
    }.unwrap();
    first_of_next.pred_opt().unwrap()
}
