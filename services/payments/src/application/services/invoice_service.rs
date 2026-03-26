use std::sync::Arc;
use logisticos_errors::{AppError, AppResult};
use logisticos_types::{InvoiceId, MerchantId, TenantId, Money, Currency};
use logisticos_events::{producer::KafkaProducer, topics, envelope::Event};
use uuid::Uuid;

use crate::{
    application::commands::{GenerateInvoiceCommand, InvoiceSummary},
    domain::{
        entities::{Invoice, InvoiceLineItem, InvoiceStatus},
        events::InvoiceGenerated,
        repositories::InvoiceRepository,
        value_objects::NET_PAYMENT_TERMS_DAYS,
    },
};

pub struct InvoiceService {
    invoice_repo: Arc<dyn InvoiceRepository>,
    kafka: Arc<KafkaProducer>,
}

impl InvoiceService {
    pub fn new(invoice_repo: Arc<dyn InvoiceRepository>, kafka: Arc<KafkaProducer>) -> Self {
        Self { invoice_repo, kafka }
    }

    /// Generate a billing invoice for a merchant covering a set of shipments.
    /// Called by the billing cron job (weekly for Starter, monthly for Business+).
    pub async fn generate(&self, tenant_id: &TenantId, cmd: GenerateInvoiceCommand) -> AppResult<Invoice> {
        let merchant_id = MerchantId::from_uuid(cmd.merchant_id);

        // Build line items — one per shipment in the billing period
        let line_items: Vec<InvoiceLineItem> = cmd.shipment_ids.iter().map(|shipment_id| {
            InvoiceLineItem {
                description: format!("Delivery service — shipment {}", &shipment_id.to_string()[..8]),
                quantity: 1,
                // Base delivery fee: ₱85 (8500 centavos) per shipment
                // In production this comes from the pricing tier lookup
                unit_price: Money::new(8_500, Currency::PHP),
                discount: None,
            }
        }).collect();

        if line_items.is_empty() {
            return Err(AppError::BusinessRule("Cannot generate an invoice with no shipments".into()));
        }

        let now = chrono::Utc::now();
        let invoice = Invoice {
            id: InvoiceId::new(),
            merchant_id: merchant_id.clone(),
            line_items,
            status: InvoiceStatus::Issued,
            issued_at: now,
            due_at: now + chrono::Duration::days(NET_PAYMENT_TERMS_DAYS),
            paid_at: None,
            currency: Currency::PHP,
        };

        self.invoice_repo.save(&invoice).await.map_err(AppError::Internal)?;

        let event = Event::new("payments", "invoice.generated", tenant_id.inner(), InvoiceGenerated {
            invoice_id: invoice.id.inner(),
            merchant_id: merchant_id.inner(),
            tenant_id: tenant_id.inner(),
            total_cents: invoice.total_due().amount,
            due_at: invoice.due_at,
        });
        self.kafka.publish_event(topics::INVOICE_GENERATED, &event).await
            .map_err(AppError::Internal)?;

        tracing::info!(
            invoice_id = %invoice.id,
            merchant_id = %merchant_id,
            total = %invoice.total_due().amount,
            "Invoice generated"
        );
        Ok(invoice)
    }

    pub async fn list(&self, merchant_id: &MerchantId) -> AppResult<Vec<InvoiceSummary>> {
        let invoices = self.invoice_repo.list_by_merchant(merchant_id).await.map_err(AppError::Internal)?;
        Ok(invoices.into_iter().map(|inv| InvoiceSummary {
            invoice_id: inv.id.inner(),
            status: format!("{:?}", inv.status).to_lowercase(),
            subtotal_cents: inv.subtotal().amount,
            vat_cents: inv.vat_amount().amount,
            total_cents: inv.total_due().amount,
            due_at: inv.due_at.to_rfc3339(),
            issued_at: inv.issued_at.to_rfc3339(),
        }).collect())
    }

    pub async fn get(&self, invoice_id: &InvoiceId) -> AppResult<Invoice> {
        self.invoice_repo.find_by_id(invoice_id).await.map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound { resource: "Invoice", id: invoice_id.inner().to_string() })
    }
}
