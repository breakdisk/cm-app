//! Consumes WEIGHT_DISCREPANCY_FOUND events from hub-ops.
//!
//! When a hub scale finds that actual weight > declared weight, hub-ops emits
//! this event. We look up the merchant's most recently issued ShipmentCharges
//! invoice and append a weight surcharge adjustment via InvoiceService.
//!
//! If no issued invoice exists yet (merchant is pre-billing), the adjustment
//! is deferred — it will be applied during the next billing run.
//!
//! Surcharge calculation:
//!   surcharge_cents = delta_grams * WEIGHT_SURCHARGE_CENTS_PER_KG / 1000
//! Configurable via WEIGHT_SURCHARGE_CENTS_PER_KG env var (default: 5000 = PHP 50/kg).

use std::sync::Arc;
use logisticos_errors::AppError;
use logisticos_events::{envelope::Event, payloads::WeightDiscrepancyFound, topics};
use logisticos_types::{MerchantId, TenantId};
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    config::ClientConfig,
    message::Message,
};
use tokio::sync::watch;
use uuid::Uuid;

use crate::{
    application::commands::ApplyWeightAdjustmentCommand,
    application::services::InvoiceService,
    domain::repositories::InvoiceRepository,
};

pub struct WeightDiscrepancyConsumer {
    consumer:        StreamConsumer,
    invoice_service: Arc<InvoiceService>,
    invoice_repo:    Arc<dyn InvoiceRepository>,
    /// PHP cents per kg of excess weight. Default 5000 = PHP 50/kg.
    surcharge_rate_cents_per_kg: i64,
}

impl WeightDiscrepancyConsumer {
    pub fn new(
        brokers:         &str,
        group_id:        &str,
        invoice_service: Arc<InvoiceService>,
        invoice_repo:    Arc<dyn InvoiceRepository>,
    ) -> anyhow::Result<Self> {
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("group.id", &format!("{}-weight-discrepancy", group_id))
            .set("auto.offset.reset", "earliest")
            .set("enable.auto.commit", "false")
            .create()?;

        consumer.subscribe(&[topics::WEIGHT_DISCREPANCY_FOUND])?;

        let surcharge_rate_cents_per_kg = std::env::var("WEIGHT_SURCHARGE_CENTS_PER_KG")
            .ok()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(5_000); // PHP 50 per kg default

        Ok(Self { consumer, invoice_service, invoice_repo, surcharge_rate_cents_per_kg })
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow_and_update() {
                        tracing::info!("Weight-discrepancy consumer shutting down");
                        break;
                    }
                }
                result = self.consumer.recv() => {
                    match result {
                        Ok(msg) => {
                            if let Some(payload) = msg.payload() {
                                if let Err(e) = self.handle(payload).await {
                                    tracing::warn!(err = %e, "weight-discrepancy handler error (skipping)");
                                }
                            }
                            self.consumer.commit_message(&msg, CommitMode::Async).ok();
                        }
                        Err(e) => {
                            tracing::error!(err = %e, "weight-discrepancy consumer recv error");
                            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        }
                    }
                }
            }
        }
    }

    async fn handle(&self, payload: &[u8]) -> anyhow::Result<()> {
        let event: Event<WeightDiscrepancyFound> = serde_json::from_slice(payload)?;
        let d = &event.data;

        // Only process if there is actual excess weight
        if d.delta_grams <= 0 {
            tracing::debug!(
                awb = %d.master_awb,
                delta_grams = d.delta_grams,
                "Weight discrepancy not positive — skipping surcharge"
            );
            return Ok(());
        }

        let tenant_id   = TenantId::from_uuid(d.tenant_id);
        let merchant_id = MerchantId::from_uuid(d.merchant_id);

        // Surcharge = delta_grams * rate_per_kg / 1000
        let surcharge_cents = (d.delta_grams as i64)
            .saturating_mul(self.surcharge_rate_cents_per_kg)
            / 1_000;

        // Find the merchant's current issued invoice to append the surcharge to
        let invoice = self.invoice_repo
            .find_latest_issued_for_merchant(&tenant_id, &merchant_id)
            .await
            .map_err(|e| anyhow::anyhow!("DB error finding invoice: {e}"))?;

        let Some(invoice) = invoice else {
            tracing::info!(
                awb         = %d.master_awb,
                merchant_id = %d.merchant_id,
                delta_grams = d.delta_grams,
                "No issued invoice found — weight surcharge deferred to next billing run"
            );
            return Ok(());
        };

        let cmd = ApplyWeightAdjustmentCommand {
            invoice_id:      invoice.id.inner(),
            awb:             d.master_awb.clone(),
            declared_grams:  d.declared_grams,
            actual_grams:    d.actual_grams,
            surcharge_cents,
            applied_by:      Uuid::nil(), // system actor
        };

        match self.invoice_service.apply_weight_adjustment(&tenant_id, cmd).await {
            Ok(Some(_)) => tracing::info!(
                awb             = %d.master_awb,
                invoice_id      = %invoice.id,
                surcharge_cents = surcharge_cents,
                delta_grams     = d.delta_grams,
                "Weight surcharge applied to invoice"
            ),
            Ok(None) => tracing::warn!(
                awb        = %d.master_awb,
                invoice_id = %invoice.id,
                "apply_weight_adjustment returned None — invoice may have been paid/cancelled"
            ),
            Err(AppError::Validation(msg)) => {
                // AWB format issue from hub-ops — log and skip rather than retry forever
                tracing::warn!(awb = %d.master_awb, err = %msg, "Weight adjustment skipped: invalid AWB");
            }
            Err(e) => {
                return Err(anyhow::anyhow!("apply_weight_adjustment failed: {e}"));
            }
        }

        Ok(())
    }
}
