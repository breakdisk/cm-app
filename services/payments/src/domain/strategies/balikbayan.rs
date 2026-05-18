//! Track A — Balikbayan Box Billing Strategy (Sea-Freight, UAE → PH Corridor)
//!
//! ## Two-Stage Collection Lifecycle
//!
//! ```text
//! Stage 1 — Empty Box Delivery
//! ─────────────────────────────
//! Driver arrives at customer's home with an empty box.
//! Collects flat AED deposit (e.g., 50 AED) via cash or mobile card reader.
//!
//! → Invoice #1 issued: Deposit charge (Issued status — not Paid until cash confirmed)
//! → DriverLedger debited: -deposit_amount (driver holds this cash)
//! → Kafka: balikbayan.deposit.collected
//! → Manifest lock: ACTIVE (cannot assign to sea container)
//!
//! Stage 2 — Full Box Retrieval
//! ─────────────────────────────
//! Customer fills box. Driver returns, measures box size,
//! looks up flat ocean freight rate, subtracts the deposit already paid.
//! Collects the remaining balance at the doorstep.
//!
//! → Invoice #1 marked Paid (deposit confirmed)
//! → Invoice #2 issued: Ocean freight − deposit credit (net balance due)
//! → Invoice #2 marked Paid (balance confirmed)
//! → DriverLedger debited: -balance_amount (driver holds this cash too)
//! → Kafka: balikbayan.final_payment.collected
//! → Manifest lock: CLEARED (shipment can be assigned to sea container / manifest)
//! ```
//!
//! ## Enforcement Rule
//!
//! A box **CANNOT** be assigned to a sea container ID or customs manifest until
//! BOTH invoices for the shipment are in `Paid` status.
//! Enforced via the `payments.billing_clearance` view (see migration 0011).

use std::sync::Arc;
use async_trait::async_trait;
use chrono::Utc;
use logisticos_errors::{AppError, AppResult};
use logisticos_events::{envelope::Event, producer::KafkaProducer, topics};
use logisticos_types::{
    invoice::{ChargeType, InvoiceNumber, InvoiceType},
    Currency, MerchantId, Money,
};
use uuid::Uuid;

use crate::{
    application::services::InvoiceSequenceSource,
    domain::{
        entities::{
            BillingPeriod, Invoice, InvoiceLineItem, InvoiceStatus,
            driver_ledger::{DriverLedger, LedgerEntryType},
        },
        events::{BalikbayanDepositCollected, BalikbayanFinalCollected},
        repositories::{DriverLedgerRepository, InvoiceRepository},
        strategies::{BillingStrategy, MilestoneContext, MilestoneOutcome, ShipmentMilestone},
    },
};

// ── Box Size Rate Table ───────────────────────────────────────────────────────

/// Flat ocean freight rates by standard box size (in AED cents).
///
/// These are hard-coded here for now; in production, load from a
/// `freight_rate_tables` config table seeded per tenant.
#[derive(Debug, Clone, Copy)]
pub struct BoxFreightRate {
    pub size_code:        &'static str,
    pub ocean_freight_aed_cents: i64,
}

const BOX_RATES: &[BoxFreightRate] = &[
    BoxFreightRate { size_code: "small",  ocean_freight_aed_cents: 22000 },  // 220 AED
    BoxFreightRate { size_code: "medium", ocean_freight_aed_cents: 30000 },  // 300 AED
    BoxFreightRate { size_code: "large",  ocean_freight_aed_cents: 38000 },  // 380 AED
    BoxFreightRate { size_code: "jumbo",  ocean_freight_aed_cents: 50000 },  // 500 AED
];

pub fn lookup_ocean_freight(size_code: &str) -> Option<i64> {
    BOX_RATES.iter()
        .find(|r| r.size_code == size_code)
        .map(|r| r.ocean_freight_aed_cents)
}

// ── BalikbayanStrategy ────────────────────────────────────────────────────────

pub struct BalikbayanStrategy {
    invoice_repo:    Arc<dyn InvoiceRepository>,
    ledger_repo:     Arc<dyn DriverLedgerRepository>,
    kafka:           Arc<KafkaProducer>,
    sequence_source: Arc<dyn InvoiceSequenceSource>,
}

impl BalikbayanStrategy {
    pub fn new(
        invoice_repo:    Arc<dyn InvoiceRepository>,
        ledger_repo:     Arc<dyn DriverLedgerRepository>,
        kafka:           Arc<KafkaProducer>,
        sequence_source: Arc<dyn InvoiceSequenceSource>,
    ) -> Self {
        Self { invoice_repo, ledger_repo, kafka, sequence_source }
    }

    // ── Stage 1: Empty box delivered, deposit collected ───────────────────────

    async fn process_stage1_deposit(
        &self,
        ctx: &MilestoneContext,
    ) -> AppResult<MilestoneOutcome> {
        let deposit_cents = ctx.collected_amount_cents;
        if deposit_cents <= 0 {
            return Err(AppError::BusinessRule(
                "Balikbayan Stage 1: collected_amount_cents must be > 0 (deposit required)".into(),
            ));
        }

        let box_id = ctx.box_id.as_deref().unwrap_or("UNKNOWN");
        let today  = Utc::now().date_naive();

        // ── Generate invoice for the deposit ──────────────────────────────────
        let seq = self.sequence_source
            .next_sequence(InvoiceType::PaymentReceipt, &ctx.tenant_code, today)
            .await.map_err(AppError::Internal)?;

        let inv_number = InvoiceNumber::generate(
            InvoiceType::PaymentReceipt, &ctx.tenant_code, today, seq,
        ).map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;

        let mut invoice = Invoice::new(
            inv_number,
            InvoiceType::PaymentReceipt,
            ctx.tenant_id.clone(),
            MerchantId::from_uuid(Uuid::nil()),     // B2C receipt — no merchant
            ctx.customer_id.map(logisticos_types::CustomerId::from_uuid),
            BillingPeriod::single_day(today),
            ctx.currency,
        );

        // Deposit line item
        invoice.add_line_item(InvoiceLineItem::document_level(
            ChargeType::BalikbayanDeposit,
            format!("Balikbayan box deposit — box {box_id}"),
            Money::new(deposit_cents, ctx.currency),
        )).map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;

        // Attach domain metadata in workflow_metadata (stored by the repo)
        // We encode it as a field on the invoice struct via the repo-level upsert.
        invoice.issue().map_err(|e| AppError::BusinessRule(e.to_string()))?;

        // Cash collected immediately at doorstep → mark Paid right away.
        invoice.mark_paid().map_err(|e| AppError::BusinessRule(e.to_string()))?;

        let invoice_id = invoice.id.inner();
        let inv_number_str = invoice.invoice_number.to_string();
        let total_cents    = invoice.total_due().amount;

        // Save with domain metadata
        self.invoice_repo
            .save_with_domain(
                &invoice,
                "balikbayan",
                serde_json::json!({
                    "stage":                "deposit",
                    "box_id":               box_id,
                    "deposit_amount_cents":  deposit_cents,
                    "shipment_id":           ctx.shipment_id,
                    "stage_1_invoice_id":    invoice_id,
                }),
            )
            .await
            .map_err(AppError::Internal)?;

        // ── Debit driver ledger ───────────────────────────────────────────────
        let mut ledger = self
            .ledger_repo
            .find_or_create_for_shift(&ctx.tenant_id, ctx.driver_id, ctx.shift_id)
            .await
            .map_err(AppError::Internal)?;

        ledger.record_cash_collected(
            LedgerEntryType::BalikbayanDepositCollected,
            deposit_cents,
            format!("Balikbayan deposit box={box_id} AWB={}", ctx.tracking_number),
            ctx.shipment_id,
            Some(invoice_id),
            Some(ctx.pod_id),
        ).map_err(|e| AppError::BusinessRule(e.to_string()))?;

        self.ledger_repo.save(&ledger).await.map_err(AppError::Internal)?;

        // ── Kafka: balikbayan.deposit.collected ───────────────────────────────
        let event = Event::new(
            "payments",
            topics::BALIKBAYAN_DEPOSIT_COLLECTED,
            ctx.tenant_id.inner(),
            BalikbayanDepositCollected {
                invoice_id,
                invoice_number:  inv_number_str.clone(),
                shipment_id:     ctx.shipment_id,
                driver_id:       ctx.driver_id,
                box_id:          box_id.to_owned(),
                deposit_cents,
                currency:        currency_str(ctx.currency),
                customer_id:     ctx.customer_id.unwrap_or(Uuid::nil()),
                customer_name:   ctx.customer_name.clone(),
                customer_phone:  ctx.customer_phone.clone(),
                tenant_id:       ctx.tenant_id.inner(),
                tracking_number: ctx.tracking_number.clone(),
                collected_at:    Utc::now().to_rfc3339(),
            },
        );
        self.kafka
            .publish_event(topics::BALIKBAYAN_DEPOSIT_COLLECTED, &event)
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(
            invoice_id    = %invoice_id,
            box_id        = %box_id,
            deposit_cents = deposit_cents,
            driver_id     = %ctx.driver_id,
            "Balikbayan Stage 1: deposit collected and invoiced"
        );

        Ok(MilestoneOutcome::DepositRecorded {
            invoice_id,
            deposit_cents,
        })
    }

    // ── Stage 2: Full box retrieved, final balance collected ──────────────────

    async fn process_stage2_final(
        &self,
        ctx: &MilestoneContext,
    ) -> AppResult<MilestoneOutcome> {
        let box_size = ctx.box_size.as_deref().ok_or_else(|| {
            AppError::BusinessRule("Balikbayan Stage 2: box_size is required".into())
        })?;

        let ocean_freight_cents = ctx.ocean_freight_cents.unwrap_or_else(|| {
            lookup_ocean_freight(box_size).unwrap_or(0)
        });
        if ocean_freight_cents <= 0 {
            return Err(AppError::BusinessRule(
                format!("Balikbayan Stage 2: unknown box_size '{box_size}' — no rate found"),
            ));
        }

        let deposit_paid_cents = ctx.deposit_paid_cents.unwrap_or(0);
        let balance_due_cents  = (ocean_freight_cents - deposit_paid_cents).max(0);
        let box_id             = ctx.box_id.as_deref().unwrap_or("UNKNOWN");
        let today              = Utc::now().date_naive();

        // ── Mark Stage 1 invoice as paid (if not already) ────────────────────
        if let Some(stage_1_id) = ctx.stage_1_invoice_id {
            if let Some(mut s1_inv) = self.invoice_repo
                .find_by_id(&logisticos_types::InvoiceId::from_uuid(stage_1_id))
                .await.map_err(AppError::Internal)?
            {
                if s1_inv.status == InvoiceStatus::Issued {
                    s1_inv.mark_paid()
                        .map_err(|e| AppError::BusinessRule(e.to_string()))?;
                    self.invoice_repo.save(&s1_inv).await.map_err(AppError::Internal)?;
                }
            }
        }

        // ── Generate final invoice (ocean freight net of deposit) ─────────────
        let seq = self.sequence_source
            .next_sequence(InvoiceType::PaymentReceipt, &ctx.tenant_code, today)
            .await.map_err(AppError::Internal)?;

        let inv_number = InvoiceNumber::generate(
            InvoiceType::PaymentReceipt, &ctx.tenant_code, today, seq,
        ).map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;

        let mut invoice = Invoice::new(
            inv_number,
            InvoiceType::PaymentReceipt,
            ctx.tenant_id.clone(),
            MerchantId::from_uuid(Uuid::nil()),
            ctx.customer_id.map(logisticos_types::CustomerId::from_uuid),
            BillingPeriod::single_day(today),
            ctx.currency,
        );

        // Ocean freight gross line
        invoice.add_line_item(InvoiceLineItem::document_level(
            ChargeType::BalikbayanOceanFreight,
            format!("Balikbayan ocean freight — {box_size} box (box {box_id})"),
            Money::new(ocean_freight_cents, ctx.currency),
        )).map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;

        // Deposit credit line (negative — reduces amount due)
        if deposit_paid_cents > 0 {
            invoice.add_line_item(InvoiceLineItem::document_level(
                ChargeType::BalikbayanDepositCredit,
                format!("Deposit credit applied (box {box_id})"),
                Money::new(-deposit_paid_cents, ctx.currency),
            )).map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;
        }

        invoice.issue().map_err(|e| AppError::BusinessRule(e.to_string()))?;
        invoice.mark_paid().map_err(|e| AppError::BusinessRule(e.to_string()))?;

        let invoice_id     = invoice.id.inner();
        let inv_number_str = invoice.invoice_number.to_string();
        let total_cents    = invoice.total_due().amount;

        self.invoice_repo
            .save_with_domain(
                &invoice,
                "balikbayan",
                serde_json::json!({
                    "stage":                    "final",
                    "box_id":                    box_id,
                    "box_size":                  box_size,
                    "ocean_freight_cents":        ocean_freight_cents,
                    "deposit_deducted_cents":     deposit_paid_cents,
                    "balance_collected_cents":    balance_due_cents,
                    "stage_1_invoice_id":         ctx.stage_1_invoice_id,
                    "shipment_id":                ctx.shipment_id,
                }),
            )
            .await
            .map_err(AppError::Internal)?;

        // ── Debit driver ledger for the balance collected ─────────────────────
        let mut ledger = self
            .ledger_repo
            .find_or_create_for_shift(&ctx.tenant_id, ctx.driver_id, ctx.shift_id)
            .await
            .map_err(AppError::Internal)?;

        if balance_due_cents > 0 {
            ledger.record_cash_collected(
                LedgerEntryType::BalikbayanFinalCollected,
                balance_due_cents,
                format!(
                    "Balikbayan final balance box={box_id} size={box_size} \
                     freight={ocean_freight_cents} less deposit={deposit_paid_cents}"
                ),
                ctx.shipment_id,
                Some(invoice_id),
                Some(ctx.pod_id),
            ).map_err(|e| AppError::BusinessRule(e.to_string()))?;

            self.ledger_repo.save(&ledger).await.map_err(AppError::Internal)?;
        }

        // ── Kafka: balikbayan.final_payment.collected → manifest cleared ──────
        let event = Event::new(
            "payments",
            topics::BALIKBAYAN_FINAL_COLLECTED,
            ctx.tenant_id.inner(),
            BalikbayanFinalCollected {
                invoice_id,
                invoice_number:        inv_number_str.clone(),
                stage_1_invoice_id:    ctx.stage_1_invoice_id.unwrap_or(Uuid::nil()),
                shipment_id:           ctx.shipment_id,
                driver_id:             ctx.driver_id,
                box_id:                box_id.to_owned(),
                box_size:              box_size.to_owned(),
                ocean_freight_cents,
                deposit_deducted_cents: deposit_paid_cents,
                balance_collected_cents: balance_due_cents,
                total_invoice_cents:   total_cents,
                currency:              currency_str(ctx.currency),
                customer_id:           ctx.customer_id.unwrap_or(Uuid::nil()),
                customer_name:         ctx.customer_name.clone(),
                customer_phone:        ctx.customer_phone.clone(),
                tenant_id:             ctx.tenant_id.inner(),
                tracking_number:       ctx.tracking_number.clone(),
                // Signal to carrier/fleet service that manifest lock is cleared
                manifest_lock_cleared: true,
                collected_at:          Utc::now().to_rfc3339(),
            },
        );
        self.kafka
            .publish_event(topics::BALIKBAYAN_FINAL_COLLECTED, &event)
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(
            invoice_id              = %invoice_id,
            box_id                  = %box_id,
            box_size                = %box_size,
            ocean_freight_cents     = ocean_freight_cents,
            deposit_deducted_cents  = deposit_paid_cents,
            balance_collected_cents = balance_due_cents,
            driver_id               = %ctx.driver_id,
            "Balikbayan Stage 2: final balance collected — manifest lock CLEARED"
        );

        Ok(MilestoneOutcome::ManifestCleared {
            invoice_id,
            invoice_number: inv_number_str,
            total_cents,
        })
    }
}

#[async_trait]
impl BillingStrategy for BalikbayanStrategy {
    async fn process_milestone(
        &self,
        ctx: &MilestoneContext,
    ) -> AppResult<MilestoneOutcome> {
        match ctx.milestone {
            ShipmentMilestone::BalikbayanEmptyBoxDelivered => {
                self.process_stage1_deposit(ctx).await
            }
            ShipmentMilestone::BalikbayanFullBoxRetrieved => {
                self.process_stage2_final(ctx).await
            }
            // Any other milestone is a no-op for Balikbayan billing.
            _ => {
                tracing::debug!(
                    shipment_id = %ctx.shipment_id,
                    milestone   = ?ctx.milestone,
                    "Balikbayan: milestone not billed"
                );
                Ok(MilestoneOutcome::NoAction)
            }
        }
    }

    fn domain_name(&self) -> &'static str { "balikbayan" }
}

fn currency_str(c: Currency) -> String {
    format!("{c:?}")
}
