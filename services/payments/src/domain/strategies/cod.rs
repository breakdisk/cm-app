//! Track C — COD Last-Mile Billing Strategy (Cash on Delivery)
//!
//! ## Invoice Lifecycle
//!
//! ```text
//! SHIPMENT_CREATED       → Draft invoice created (customs manifest baseline).
//!                          No money collected. Driver liability: $0.
//!
//! PICKUP_COMPLETED       → $0 event. Invoice stays Draft. No billing action.
//!
//! LINEHAUL_TRANSIT       → $0 event. Invoice stays Draft. No billing action.
//!
//! COD_DELIVERY_COMPLETED → Driver submits POD with cash/QR confirmation.
//!                          Invoice: Draft → Issued → Paid (atomic).
//!                          DriverLedger: debited by cod_amount_cents.
//!                          Kafka: payments.cod.collected  (→ engagement WhatsApp receipt)
//!                          Kafka: payments.invoice.generated (→ customer app push)
//! ```
//!
//! ## Mobile Enforcement Rule
//!
//! The Kotlin driver app enforces the collection gate at the device layer:
//! the "Complete Delivery" button is locked until the driver either:
//!   (a) confirms cash received and enters the collected amount, or
//!   (b) confirms QR wallet scan with transaction reference.
//!
//! This strategy does NOT re-enforce it server-side (trust the signed payload)
//! but validates that `collected_amount_cents > 0` before recording.
//!
//! ## Refactor Note
//!
//! This strategy consolidates the logic previously scattered across:
//!   - `pod_consumer.rs:handle_pod_captured()` (COD branch)
//!   - `cod_service.rs:reconcile_cod()`
//! Both are now deprecated in favour of routing through this strategy.

use std::sync::Arc;
use async_trait::async_trait;
use chrono::Utc;
use logisticos_errors::{AppError, AppResult};
use logisticos_events::{envelope::Event, producer::KafkaProducer, topics};
use logisticos_types::{
    invoice::{ChargeType, InvoiceNumber, InvoiceType},
    Currency, CustomerId, InvoiceId, MerchantId, Money,
};
use uuid::Uuid;

use crate::{
    application::services::InvoiceSequenceSource,
    domain::{
        entities::{
            BillingPeriod, CodCollection, Invoice, InvoiceLineItem,
            driver_ledger::{DriverLedger, LedgerEntryType},
        },
        events::{CodReconciled, InvoiceGenerated},
        repositories::{CodRepository, DriverLedgerRepository, InvoiceRepository, ShipmentBillingSource},
        strategies::{BillingStrategy, MilestoneContext, MilestoneOutcome, ShipmentMilestone},
    },
};

// ── CodStrategy ───────────────────────────────────────────────────────────────

pub struct CodStrategy {
    invoice_repo:    Arc<dyn InvoiceRepository>,
    cod_repo:        Arc<dyn CodRepository>,
    ledger_repo:     Arc<dyn DriverLedgerRepository>,
    kafka:           Arc<KafkaProducer>,
    sequence_source: Arc<dyn InvoiceSequenceSource>,
    billing_source:  Arc<dyn ShipmentBillingSource>,
}

impl CodStrategy {
    pub fn new(
        invoice_repo:    Arc<dyn InvoiceRepository>,
        cod_repo:        Arc<dyn CodRepository>,
        ledger_repo:     Arc<dyn DriverLedgerRepository>,
        kafka:           Arc<KafkaProducer>,
        sequence_source: Arc<dyn InvoiceSequenceSource>,
        billing_source:  Arc<dyn ShipmentBillingSource>,
    ) -> Self {
        Self { invoice_repo, cod_repo, ledger_repo, kafka, sequence_source, billing_source }
    }

    // ── COD delivery completed: collect cash + issue invoice atomically ────────

    async fn process_cod_delivery(
        &self,
        ctx: &MilestoneContext,
    ) -> AppResult<MilestoneOutcome> {
        let cod_cents = ctx.collected_amount_cents;
        if cod_cents <= 0 {
            return Err(AppError::BusinessRule(
                "COD delivery: collected_amount_cents must be > 0".into(),
            ));
        }

        // Idempotency: if COD already recorded for this shipment, skip.
        if self.cod_repo
            .find_by_shipment(ctx.shipment_id)
            .await
            .map_err(AppError::Internal)?
            .is_some()
        {
            tracing::warn!(
                shipment_id = %ctx.shipment_id,
                "COD: already reconciled for this shipment — skipping (idempotent)"
            );
            return Ok(MilestoneOutcome::NoAction);
        }

        // ── Fetch per-shipment billing breakdown from order-intake ────────────
        let billing = self.billing_source
            .fetch(ctx.shipment_id)
            .await
            .map_err(AppError::Internal)?;

        let currency    = parse_currency(&billing.currency);
        let merchant_id = MerchantId::from_uuid(billing.merchant_id);
        let today       = Utc::now().date_naive();

        // ── 1. Record COD collection (per-shipment cash row) ──────────────────
        let cod_collection = CodCollection::new(
            ctx.tenant_id.clone(),
            merchant_id.clone(),
            ctx.shipment_id,
            ctx.driver_id,
            ctx.pod_id,
            Money::new(cod_cents, currency),
        );

        let cod_id = cod_collection.id;
        let platform_fee_cents = cod_collection.platform_fee().amount;
        let merchant_credit_cents = cod_collection.merchant_credit().amount;

        self.cod_repo.save(&cod_collection).await.map_err(AppError::Internal)?;

        // ── 2. Debit driver ledger ────────────────────────────────────────────
        let mut ledger = self
            .ledger_repo
            .find_or_create_for_shift(&ctx.tenant_id, ctx.driver_id, ctx.shift_id)
            .await
            .map_err(AppError::Internal)?;

        ledger.record_cash_collected(
            LedgerEntryType::CodCollected,
            cod_cents,
            format!("COD AWB={} PHP {:.2}", ctx.tracking_number, cod_cents as f64 / 100.0),
            ctx.shipment_id,
            None,          // invoice_id filled after invoice is created
            Some(ctx.pod_id),
        ).map_err(|e| AppError::BusinessRule(e.to_string()))?;

        self.ledger_repo.save(&ledger).await.map_err(AppError::Internal)?;

        // ── 3. Issue payment receipt invoice (Draft → Issued → Paid atomic) ───
        let seq = self.sequence_source
            .next_sequence(InvoiceType::PaymentReceipt, &ctx.tenant_code, today)
            .await.map_err(AppError::Internal)?;

        let inv_number = InvoiceNumber::generate(
            InvoiceType::PaymentReceipt, &ctx.tenant_code, today, seq,
        ).map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;

        let customer_id = ctx.customer_id.map(CustomerId::from_uuid);

        let mut invoice = Invoice::new(
            inv_number,
            InvoiceType::PaymentReceipt,
            ctx.tenant_id.clone(),
            merchant_id,
            customer_id,
            BillingPeriod::single_day(today),
            currency,
        );

        // Freight charges line items
        if billing.base_freight_cents > 0 {
            invoice.add_line_item(InvoiceLineItem::document_level(
                ChargeType::BaseFreight,
                "Base freight charge".into(),
                Money::new(billing.base_freight_cents, currency),
            )).map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;
        }
        if billing.fuel_surcharge_cents > 0 {
            invoice.add_line_item(InvoiceLineItem::document_level(
                ChargeType::FuelSurcharge,
                "Fuel surcharge".into(),
                Money::new(billing.fuel_surcharge_cents, currency),
            )).map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;
        }
        if billing.insurance_cents > 0 {
            invoice.add_line_item(InvoiceLineItem::document_level(
                ChargeType::InsuranceFee,
                "Shipment insurance".into(),
                Money::new(billing.insurance_cents, currency),
            )).map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;
        }

        // COD collection fee line item (1.5% platform fee on collected cash)
        if platform_fee_cents > 0 {
            invoice.add_line_item(InvoiceLineItem::document_level(
                ChargeType::CodCollectionFee,
                format!("COD collection fee (1.5% of {} cents)", cod_cents),
                Money::new(platform_fee_cents, currency),
            )).map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;
        }

        if invoice.line_items.is_empty() {
            return Err(AppError::BusinessRule(
                format!("COD billing: shipment {} returned all-zero amounts", ctx.tracking_number)
            ));
        }

        // Atomic Draft → Issued → Paid
        invoice.issue_and_capture()
            .map_err(|e| AppError::BusinessRule(e.to_string()))?;

        let invoice_id     = invoice.id.inner();
        let inv_number_str = invoice.invoice_number.to_string();
        let total_cents    = invoice.total_due().amount;

        self.invoice_repo
            .save_with_domain(
                &invoice,
                "cod",
                serde_json::json!({
                    "domain":           "cod",
                    "shipment_id":       ctx.shipment_id,
                    "collected_cents":   cod_cents,
                    "payment_method":    format!("{:?}", ctx.payment_method),
                    "collected_at":      Utc::now().to_rfc3339(),
                    "cod_collection_id": cod_id,
                }),
            )
            .await
            .map_err(AppError::Internal)?;

        // ── 4. Publish events ─────────────────────────────────────────────────

        // cod.collected → engagement WhatsApp COD receipt
        let cod_event = Event::new(
            "payments",
            topics::COD_COLLECTED,
            ctx.tenant_id.inner(),
            CodReconciled {
                cod_id,
                shipment_id:           ctx.shipment_id,
                tenant_id:             ctx.tenant_id.inner(),
                amount_cents:          cod_cents,
                merchant_credit_cents,
                platform_fee_cents,
                tracking_number:       ctx.tracking_number.clone(),
                customer_phone:        ctx.customer_phone.clone(),
                customer_name:         ctx.customer_name.clone(),
            },
        );
        self.kafka
            .publish_event(topics::COD_COLLECTED, &cod_event)
            .await
            .map_err(AppError::Internal)?;

        // invoice.generated → customer app push + email.
        // billing_domain = "cod" tells the engagement engine to suppress WhatsApp here:
        // the cod.collected event above already sent the customer a WhatsApp cod_receipt.
        let inv_event = Event::new(
            "payments",
            topics::INVOICE_GENERATED,
            ctx.tenant_id.inner(),
            InvoiceGenerated {
                invoice_id,
                invoice_number:  inv_number_str.clone(),
                recipient_type:  "customer".into(),
                merchant_id:     Uuid::nil(),
                merchant_email:  None,
                customer_id:     ctx.customer_id.unwrap_or(Uuid::nil()),
                customer_email:  ctx.customer_email.clone(),
                tenant_id:       ctx.tenant_id.inner(),
                total_cents,
                currency:        format!("{currency:?}"),
                due_at:          invoice.due_at,
                tracking_number: ctx.tracking_number.clone(),
                customer_name:   ctx.customer_name.clone(),
                customer_phone:  ctx.customer_phone.clone(),
                paid_at:         today.to_string(),
                billing_domain:  "cod".into(),
            },
        );
        self.kafka
            .publish_event(topics::INVOICE_GENERATED, &inv_event)
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(
            invoice_id    = %invoice_id,
            cod_id        = %cod_id,
            cod_cents     = cod_cents,
            driver_id     = %ctx.driver_id,
            tracking_number = %ctx.tracking_number,
            "COD delivery: cash collected, invoice issued, driver liability recorded"
        );

        Ok(MilestoneOutcome::InvoiceIssued {
            invoice_id,
            invoice_number: inv_number_str,
            total_cents,
            ledger_updated: true,
        })
    }
}

#[async_trait]
impl BillingStrategy for CodStrategy {
    async fn process_milestone(
        &self,
        ctx: &MilestoneContext,
    ) -> AppResult<MilestoneOutcome> {
        match ctx.milestone {
            ShipmentMilestone::CodDeliveryCompleted
            | ShipmentMilestone::PodCaptured if ctx.collected_amount_cents > 0 => {
                self.process_cod_delivery(ctx).await
            }
            // Pickup, transit — $0 for COD. Invoice stays Draft.
            _ => {
                tracing::debug!(
                    shipment_id = %ctx.shipment_id,
                    milestone   = ?ctx.milestone,
                    cod_cents   = ctx.collected_amount_cents,
                    "COD: milestone not billed (pre-delivery or zero amount)"
                );
                Ok(MilestoneOutcome::NoAction)
            }
        }
    }

    fn domain_name(&self) -> &'static str { "cod" }
}

fn parse_currency(s: &str) -> Currency {
    match s.to_uppercase().as_str() {
        "AED" => Currency::AED,
        "USD" => Currency::USD,
        "SGD" => Currency::SGD,
        "MYR" => Currency::MYR,
        "IDR" => Currency::IDR,
        _     => Currency::PHP,
    }
}
