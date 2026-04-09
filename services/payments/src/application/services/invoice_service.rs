//! InvoiceService — generates and manages merchant billing documents.
//!
//! # Flow
//! 1. Billing cron calls `generate()` with pre-computed per-AWB charges.
//! 2. Service builds `InvoiceLineItem` records keyed by AWB + `ChargeType`.
//! 3. Invoice is saved in Draft state, then immediately issued.
//! 4. `InvoiceFinalized` Kafka event is published for downstream consumers
//!    (engagement engine sends email, analytics records the document).
//! 5. Weight-discrepancy events from hub-ops call `apply_weight_adjustment()`
//!    which appends an `InvoiceAdjustment` to the matching issued invoice.

use std::sync::Arc;
use chrono::NaiveDate;
use logisticos_errors::{AppError, AppResult};
use logisticos_types::{
    awb::Awb,
    invoice::{ChargeType, InvoiceNumber, InvoiceType},
    Currency, InvoiceId, MerchantId, TenantId,
};
use logisticos_events::{producer::KafkaProducer, topics, envelope::Event};

use crate::{
    application::commands::{
        GenerateInvoiceCommand, ApplyWeightAdjustmentCommand, InvoiceSummary,
    },
    domain::{
        entities::{BillingPeriod, Invoice, InvoiceAdjustment, InvoiceError, InvoiceLineItem},
        events::InvoiceGenerated,
        repositories::InvoiceRepository,
        value_objects::NET_PAYMENT_TERMS_DAYS,
    },
};

pub struct InvoiceService {
    invoice_repo:     Arc<dyn InvoiceRepository>,
    kafka:            Arc<KafkaProducer>,
    /// Redis/Postgres sequence generator for invoice numbers.
    /// For now, sequences are fetched from the repository layer which
    /// wraps the Redis INCR pattern described in InvoiceNumber::redis_counter_key().
    sequence_source:  Arc<dyn InvoiceSequenceSource>,
}

/// Abstracts the counter used to generate the 5-digit invoice sequence.
/// Implementations: Redis INCR (primary) + Postgres fallback.
#[async_trait::async_trait]
pub trait InvoiceSequenceSource: Send + Sync {
    async fn next_sequence(
        &self,
        invoice_type: InvoiceType,
        tenant_code:  &str,
        period:       NaiveDate,
    ) -> anyhow::Result<u32>;
}

impl InvoiceService {
    pub fn new(
        invoice_repo:    Arc<dyn InvoiceRepository>,
        kafka:           Arc<KafkaProducer>,
        sequence_source: Arc<dyn InvoiceSequenceSource>,
    ) -> Self {
        Self { invoice_repo, kafka, sequence_source }
    }

    /// Generate a shipment-charges invoice for a merchant covering a billing period.
    pub async fn generate(
        &self,
        tenant_id: &TenantId,
        cmd:       GenerateInvoiceCommand,
    ) -> AppResult<Invoice> {
        if cmd.charges.is_empty() {
            return Err(AppError::BusinessRule(
                "Cannot generate an invoice with no charges".into(),
            ));
        }

        let merchant_id    = MerchantId::from_uuid(cmd.merchant_id);
        let billing_period = BillingPeriod::monthly(cmd.billing_period_year, cmd.billing_period_month);
        let period_date    = billing_period.start;

        // ── Generate invoice number (Redis INCR → structured InvoiceNumber) ───
        let sequence = self.sequence_source
            .next_sequence(InvoiceType::ShipmentCharges, &cmd.tenant_code, period_date)
            .await
            .map_err(AppError::Internal)?;

        let invoice_number = InvoiceNumber::generate(
            InvoiceType::ShipmentCharges,
            &cmd.tenant_code,
            period_date,
            sequence,
        )
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;

        let mut invoice = Invoice::new(
            invoice_number,
            InvoiceType::ShipmentCharges,
            tenant_id.clone(),
            merchant_id.clone(),
            billing_period,
            Currency::PHP,
        );

        // ── Build per-AWB line items ──────────────────────────────────────────
        for charge in cmd.charges {
            let charge_type = parse_charge_type(&charge.charge_type)
                .map_err(|e| AppError::Validation(e))?;

            let awb = Awb::parse(&charge.awb)
                .map_err(|e| AppError::Validation(e.to_string()))?;

            let mut item = InvoiceLineItem::for_awb(
                charge_type,
                awb,
                charge.description,
                charge.quantity,
                logisticos_types::Money::new(charge.unit_price_cents, Currency::PHP),
            );
            if let Some(disc) = charge.discount_cents {
                item.discount = Some(logisticos_types::Money::new(disc, Currency::PHP));
            }
            invoice.add_line_item(item)
                .map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;
        }

        // ── Issue (Draft → Issued) ────────────────────────────────────────────
        invoice.issue()
            .map_err(|e| AppError::BusinessRule(e.to_string()))?;

        self.invoice_repo.save(&invoice).await.map_err(AppError::Internal)?;

        // ── Publish InvoiceFinalized event ────────────────────────────────────
        let event = Event::new(
            "payments",
            "invoice.finalized",
            tenant_id.inner(),
            InvoiceGenerated {
                invoice_id:  invoice.id.inner(),
                merchant_id: merchant_id.inner(),
                tenant_id:   tenant_id.inner(),
                total_cents: invoice.total_due().amount,
                due_at:      invoice.due_at,
            },
        );
        self.kafka
            .publish_event(topics::INVOICE_GENERATED, &event)
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(
            invoice_id     = %invoice.id,
            invoice_number = %invoice.invoice_number,
            merchant_id    = %merchant_id,
            awb_count      = invoice.awb_count(),
            total_cents    = invoice.total_due().amount,
            "Invoice generated and issued"
        );
        Ok(invoice)
    }

    /// Apply a weight-discrepancy surcharge to an already-issued invoice.
    ///
    /// Called by the `WeightDiscrepancyFound` Kafka consumer.
    /// If no issued invoice exists for this merchant in the current period,
    /// the adjustment is deferred (returns `Ok(None)`) — it will be picked
    /// up by the next billing run.
    pub async fn apply_weight_adjustment(
        &self,
        tenant_id: &TenantId,
        cmd:       ApplyWeightAdjustmentCommand,
    ) -> AppResult<Option<Invoice>> {
        let invoice_id = InvoiceId::from_uuid(cmd.invoice_id);
        let mut invoice = match self.invoice_repo.find_by_id(&invoice_id).await.map_err(AppError::Internal)? {
            Some(inv) => inv,
            None => return Ok(None),
        };

        let awb = Awb::parse(&cmd.awb)
            .map_err(|e| AppError::Validation(e.to_string()))?;

        let adj = InvoiceAdjustment {
            id:          uuid::Uuid::new_v4(),
            invoice_id:  invoice.id.clone(),
            charge_type: ChargeType::WeightSurcharge,
            amount:      logisticos_types::Money::new(cmd.surcharge_cents, Currency::PHP),
            reason:      format!(
                "Weight discrepancy: declared {}g, actual {}g (+{}g)",
                cmd.declared_grams, cmd.actual_grams,
                cmd.actual_grams as i32 - cmd.declared_grams as i32,
            ),
            awb:         Some(awb),
            created_by:  cmd.applied_by,
            created_at:  chrono::Utc::now(),
        };

        invoice.add_adjustment(adj)
            .map_err(|e| AppError::BusinessRule(e.to_string()))?;

        self.invoice_repo.save(&invoice).await.map_err(AppError::Internal)?;

        tracing::info!(
            invoice_id  = %invoice.id,
            awb         = %cmd.awb,
            surcharge   = cmd.surcharge_cents,
            "Weight adjustment applied to invoice"
        );
        Ok(Some(invoice))
    }

    pub async fn list(&self, merchant_id: &MerchantId) -> AppResult<Vec<InvoiceSummary>> {
        let invoices = self.invoice_repo
            .list_by_merchant(merchant_id)
            .await
            .map_err(AppError::Internal)?;

        Ok(invoices.into_iter().map(invoice_to_summary).collect())
    }

    pub async fn get(&self, invoice_id: &InvoiceId) -> AppResult<Invoice> {
        self.invoice_repo
            .find_by_id(invoice_id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound {
                resource: "Invoice",
                id: invoice_id.inner().to_string(),
            })
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn parse_charge_type(s: &str) -> Result<ChargeType, String> {
    match s {
        "base_freight"           => Ok(ChargeType::BaseFreight),
        "weight_surcharge"       => Ok(ChargeType::WeightSurcharge),
        "dimensional_surcharge"  => Ok(ChargeType::DimensionalSurcharge),
        "remote_area_surcharge"  => Ok(ChargeType::RemoteAreaSurcharge),
        "fuel_surcharge"         => Ok(ChargeType::FuelSurcharge),
        "cod_handling_fee"       => Ok(ChargeType::CodHandlingFee),
        "failed_delivery_fee"    => Ok(ChargeType::FailedDeliveryFee),
        "return_fee"             => Ok(ChargeType::ReturnFee),
        "insurance_fee"          => Ok(ChargeType::InsuranceFee),
        "customs_duty"           => Ok(ChargeType::CustomsDuty),
        "storage_fee"            => Ok(ChargeType::StorageFee),
        "reschedule_fee"         => Ok(ChargeType::RescheduleFee),
        "manual_adjustment"      => Ok(ChargeType::ManualAdjustment),
        other => Err(format!("Unknown charge type: '{other}'")),
    }
}

fn invoice_to_summary(inv: Invoice) -> InvoiceSummary {
    use chrono::Datelike;
    let year  = inv.billing_period.start.year();
    let month = inv.billing_period.start.month();
    InvoiceSummary {
        invoice_id:     inv.id.inner(),
        invoice_number: inv.invoice_number.to_string(),
        invoice_type:   format!("{:?}", inv.invoice_type).to_lowercase(),
        status:         format!("{:?}", inv.status).to_lowercase(),
        awb_count:      inv.awb_count(),
        subtotal_cents: inv.subtotal().amount,
        vat_cents:      inv.vat_amount().amount,
        total_cents:    inv.total_due().amount,
        billing_period: format!("{:04}-{:02}", year, month),
        due_at:         inv.due_at.to_rfc3339(),
        issued_at:      inv.issued_at.to_rfc3339(),
    }
}
