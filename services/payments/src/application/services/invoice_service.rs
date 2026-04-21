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
    Currency, CustomerId, InvoiceId, MerchantId, TenantId,
};
use logisticos_events::{producer::KafkaProducer, topics, envelope::Event};

use crate::{
    application::commands::{
        GenerateInvoiceCommand, ApplyWeightAdjustmentCommand, IssuePaymentReceiptCommand,
        InvoiceSummary,
    },
    domain::{
        entities::{BillingPeriod, Invoice, InvoiceAdjustment, InvoiceError, InvoiceLineItem},
        events::InvoiceGenerated,
        repositories::{InvoiceRepository, ShipmentBillingSource},
        value_objects::NET_PAYMENT_TERMS_DAYS,
    },
};

pub struct InvoiceService {
    invoice_repo:     Arc<dyn InvoiceRepository>,
    kafka:            Arc<KafkaProducer>,
    /// Redis/Postgres sequence generator for invoice numbers.
    sequence_source:  Arc<dyn InvoiceSequenceSource>,
    /// HTTP client that fetches per-shipment fee breakdown from order-intake.
    billing_source:   Arc<dyn ShipmentBillingSource>,
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
        billing_source:  Arc<dyn ShipmentBillingSource>,
    ) -> Self {
        Self { invoice_repo, kafka, sequence_source, billing_source }
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
            None,
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

        // ── Publish InvoiceGenerated event ───────────────────────────────────
        let event = Event::new(
            "payments",
            "invoice.generated",
            tenant_id.inner(),
            InvoiceGenerated {
                invoice_id:     invoice.id.inner(),
                invoice_number: invoice.invoice_number.to_string(),
                recipient_type: "merchant".into(),
                merchant_id:    merchant_id.inner(),
                merchant_email: cmd.merchant_email.clone(),
                customer_id:    uuid::Uuid::nil(),
                customer_email: None,
                tenant_id:      tenant_id.inner(),
                total_cents:    invoice.total_due().amount,
                currency:       format!("{:?}", invoice.currency),
                due_at:         invoice.due_at,
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
        _tenant_id: &TenantId,
        cmd:        ApplyWeightAdjustmentCommand,
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

    /// Issue and immediately capture a per-shipment payment receipt for a B2C customer booking.
    ///
    /// Called by `PodConsumer` when `pod.captured` arrives for a shipment where
    /// `booked_by_customer == true`.  The customer's card was preauthorised at
    /// booking time; this call records the receipt and marks it Paid in one
    /// atomic domain transition (`Invoice::issue_and_capture`).
    pub async fn issue_payment_receipt(
        &self,
        tenant_id: &TenantId,
        cmd:       IssuePaymentReceiptCommand,
    ) -> AppResult<Invoice> {
        // ── Fetch fee breakdown from order-intake ─────────────────────────────
        let billing = self.billing_source
            .fetch(cmd.shipment_id)
            .await
            .map_err(AppError::Internal)?;

        // ── Generate invoice number (RC-{tenant}-{YYYY}-{MM}-{NNNNN}) ─────────
        let billing_period = BillingPeriod::single_day(cmd.delivered_on);
        let sequence = self.sequence_source
            .next_sequence(InvoiceType::PaymentReceipt, &cmd.tenant_code, cmd.delivered_on)
            .await
            .map_err(AppError::Internal)?;

        let invoice_number = InvoiceNumber::generate(
            InvoiceType::PaymentReceipt,
            &cmd.tenant_code,
            cmd.delivered_on,
            sequence,
        )
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;

        let currency = match billing.currency.to_uppercase().as_str() {
            "USD" => Currency::USD,
            "SGD" => Currency::SGD,
            "MYR" => Currency::MYR,
            "IDR" => Currency::IDR,
            _     => Currency::PHP,
        };

        // PaymentReceipt invoices: merchant_id = the platform tenant's own merchant UUID
        // (nil placeholder — receipts are customer-facing, not merchant-billing).
        let platform_merchant = MerchantId::from_uuid(uuid::Uuid::nil());
        let customer_id       = CustomerId::from_uuid(cmd.customer_id);

        let mut invoice = Invoice::new(
            invoice_number,
            InvoiceType::PaymentReceipt,
            tenant_id.clone(),
            platform_merchant,
            Some(customer_id),
            billing_period,
            currency,
        );

        // ── Build line items from billing breakdown ───────────────────────────
        let awb = Awb::parse(&billing.awb)
            .map_err(|e| AppError::Validation(e.to_string()))?;

        if billing.base_freight_cents > 0 {
            invoice.add_line_item(InvoiceLineItem::document_level(
                ChargeType::BaseFreight,
                "Base freight charge".into(),
                logisticos_types::Money::new(billing.base_freight_cents, currency),
            )).map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;
        }
        if billing.fuel_surcharge_cents > 0 {
            invoice.add_line_item(InvoiceLineItem::document_level(
                ChargeType::FuelSurcharge,
                "Fuel surcharge".into(),
                logisticos_types::Money::new(billing.fuel_surcharge_cents, currency),
            )).map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;
        }
        if billing.insurance_cents > 0 {
            invoice.add_line_item(InvoiceLineItem::document_level(
                ChargeType::InsuranceFee,
                "Shipment insurance".into(),
                logisticos_types::Money::new(billing.insurance_cents, currency),
            )).map_err(|e| AppError::Internal(anyhow::anyhow!(e.to_string())))?;
        }
        // Sanity check: must have at least one line item
        if invoice.line_items.is_empty() {
            return Err(AppError::BusinessRule(
                format!("Shipment {} billing returned all-zero amounts", billing.awb)
            ));
        }

        // ── Issue + capture in one step (Draft → Issued → Paid) ──────────────
        invoice.issue_and_capture()
            .map_err(|e| AppError::BusinessRule(e.to_string()))?;

        self.invoice_repo.save(&invoice).await.map_err(AppError::Internal)?;

        // ── Publish InvoiceGenerated with recipient_type = "customer" ─────────
        let event = Event::new(
            "payments",
            "invoice.generated",
            tenant_id.inner(),
            InvoiceGenerated {
                invoice_id:     invoice.id.inner(),
                invoice_number: invoice.invoice_number.to_string(),
                recipient_type: "customer".into(),
                merchant_id:    uuid::Uuid::nil(),
                merchant_email: None,
                customer_id:    cmd.customer_id,
                customer_email: cmd.customer_email.clone(),
                tenant_id:      tenant_id.inner(),
                total_cents:    invoice.total_due().amount,
                currency:       format!("{:?}", currency),
                due_at:         invoice.due_at,
            },
        );
        self.kafka
            .publish_event(topics::INVOICE_GENERATED, &event)
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(
            invoice_id     = %invoice.id,
            invoice_number = %invoice.invoice_number,
            customer_id    = %cmd.customer_id,
            shipment_id    = %cmd.shipment_id,
            awb            = %awb,
            total_cents    = invoice.total_due().amount,
            "Payment receipt issued and captured"
        );
        Ok(invoice)
    }

    pub async fn list(&self, merchant_id: &MerchantId) -> AppResult<Vec<InvoiceSummary>> {
        let invoices = self.invoice_repo
            .list_by_merchant(merchant_id)
            .await
            .map_err(AppError::Internal)?;

        Ok(invoices.into_iter().map(invoice_to_summary).collect())
    }

    /// Tenant-wide merchant invoice list for the admin/ops console.
    /// Excludes PaymentReceipt invoices (those belong to the customer app).
    pub async fn list_for_tenant(&self, tenant_id: &TenantId) -> AppResult<Vec<InvoiceSummary>> {
        let invoices = self.invoice_repo
            .list_by_tenant(tenant_id)
            .await
            .map_err(AppError::Internal)?;

        Ok(invoices.into_iter().map(invoice_to_summary).collect())
    }

    /// List PaymentReceipt invoices for a B2C customer (customer app Profile → Receipts).
    pub async fn list_for_customer(&self, customer_id: &CustomerId) -> AppResult<Vec<InvoiceSummary>> {
        let invoices = self.invoice_repo
            .list_by_customer(customer_id)
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

    /// Re-publish the `invoice.generated` Kafka event so the engagement engine
    /// re-sends the email/SMS receipt.  Only the invoice's customer (or an admin
    /// with BILLING_MANAGE) may request a resend.
    pub async fn resend(
        &self,
        invoice_id: &InvoiceId,
        caller_id:  uuid::Uuid,
    ) -> AppResult<()> {
        let invoice = self.get(invoice_id).await?;

        // Authorisation: customers may only resend their own receipts.
        if let Some(ref cid) = invoice.customer_id {
            if cid.inner() != caller_id {
                return Err(AppError::Forbidden {
                    resource: "invoice belonging to another customer".into(),
                });
            }
        }

        let event = Event::new(
            "payments",
            "invoice.resend_requested",
            invoice.tenant_id.inner(),
            InvoiceGenerated {
                invoice_id:     invoice.id.inner(),
                invoice_number: invoice.invoice_number.to_string(),
                recipient_type: invoice.customer_id.as_ref().map(|_| "customer").unwrap_or("merchant").into(),
                merchant_id:    uuid::Uuid::nil(),
                merchant_email: None,
                customer_id:    invoice.customer_id.as_ref().map(|c| c.inner()).unwrap_or(uuid::Uuid::nil()),
                customer_email: None, // engagement engine looks this up from CDP
                tenant_id:      invoice.tenant_id.inner(),
                total_cents:    invoice.total_due().amount,
                currency:       format!("{:?}", invoice.currency),
                due_at:         invoice.due_at,
            },
        );

        self.kafka
            .publish_event(topics::INVOICE_GENERATED, &event)
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(
            invoice_id = %invoice_id,
            caller     = %caller_id,
            "Invoice resend requested"
        );
        Ok(())
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
