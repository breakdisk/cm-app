//! Consumes `hub.container.customs_cleared` from hub-ops.
//!
//! When a container clears customs with duties owed, hub-ops attaches a
//! per-shipment `details` list (the "enrich from request" data) carrying the
//! payer merchant and the duty amount for each shipment. For every billable
//! detail we generate a customs-duty invoice for the merchant via
//! `InvoiceService::generate` (charge type `customs_duty`).
//!
//! Details with no merchant or a non-positive duty are skipped — duties are only
//! billed when both a payer and a positive amount are present.

use std::sync::Arc;

use logisticos_events::{envelope::Event, payloads::{ContainerCustomsCleared, ShipmentMilestoneDetail}, topics};
use logisticos_types::TenantId;
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    config::ClientConfig,
    message::Message,
};
use tokio::sync::watch;

use crate::application::commands::{AwbChargeInput, GenerateInvoiceCommand};
use crate::application::services::InvoiceService;

/// A detail is billable for duties only when a payer merchant and a positive
/// duty amount are both present.
fn is_billable(detail: &ShipmentMilestoneDetail) -> bool {
    detail.merchant_id.is_some() && detail.duty_cents > 0
}

pub struct CustomsDutyConsumer {
    consumer:        StreamConsumer,
    invoice_service: Arc<InvoiceService>,
}

impl CustomsDutyConsumer {
    pub fn new(
        brokers:         &str,
        group_id:        &str,
        invoice_service: Arc<InvoiceService>,
    ) -> anyhow::Result<Self> {
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("group.id", format!("{}-customs-duty", group_id))
            .set("auto.offset.reset", "earliest")
            .set("enable.auto.commit", "false")
            .create()?;

        consumer.subscribe(&[topics::CONTAINER_CUSTOMS_CLEARED])?;
        Ok(Self { consumer, invoice_service })
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow_and_update() {
                        tracing::info!("Customs-duty consumer shutting down");
                        break;
                    }
                }
                result = self.consumer.recv() => {
                    match result {
                        Ok(msg) => {
                            if let Some(payload) = msg.payload() {
                                if let Err(e) = self.handle(payload).await {
                                    tracing::warn!(err = %e, "customs-duty handler error (skipping)");
                                }
                            }
                            self.consumer.commit_message(&msg, CommitMode::Async).ok();
                        }
                        Err(e) => {
                            tracing::error!(err = %e, "customs-duty consumer recv error");
                            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        }
                    }
                }
            }
        }
    }

    async fn handle(&self, payload: &[u8]) -> anyhow::Result<()> {
        use chrono::Datelike;

        let event: Event<ContainerCustomsCleared> = serde_json::from_slice(payload)?;
        let d = event.data;

        if d.tenant_code.is_empty() {
            tracing::warn!(
                container_id = %d.container_id,
                "customs_cleared has no tenant_code — cannot number duties invoice; skipping"
            );
            return Ok(());
        }

        let tenant_id = TenantId::from_uuid(d.tenant_id);
        let now       = chrono::Utc::now();
        let mut billed = 0usize;

        for detail in d.details.iter().filter(|x| is_billable(x)) {
            // Safe unwrap: is_billable guarantees merchant_id is Some.
            let merchant_id = detail.merchant_id.unwrap();
            let awb = if detail.master_awb.is_empty() {
                detail.tracking_number.clone()
            } else {
                detail.master_awb.clone()
            };

            let cmd = GenerateInvoiceCommand {
                merchant_id,
                merchant_email:       if detail.merchant_email.is_empty() { None } else { Some(detail.merchant_email.clone()) },
                tenant_code:          d.tenant_code.clone(),
                billing_period_year:  now.year(),
                billing_period_month: now.month(),
                charges: vec![AwbChargeInput {
                    awb,
                    charge_type:      "customs_duty".to_string(),
                    description:      "Customs duties".to_string(),
                    quantity:         1,
                    unit_price_cents: detail.duty_cents,
                    discount_cents:   None,
                }],
            };

            match self.invoice_service.generate(&tenant_id, cmd).await {
                Ok(invoice) => {
                    billed += 1;
                    tracing::info!(
                        shipment_id    = %detail.shipment_id,
                        merchant_id    = %merchant_id,
                        duty_cents     = detail.duty_cents,
                        invoice_number = %invoice.invoice_number,
                        "Customs duties invoice generated"
                    );
                }
                Err(e) => {
                    // Skip this payer rather than fail the whole batch / retry forever.
                    tracing::warn!(
                        shipment_id = %detail.shipment_id,
                        err         = %e,
                        "Customs duties invoice failed for shipment — skipping"
                    );
                }
            }
        }

        tracing::info!(
            container_id = %d.container_id,
            payers       = d.details.len(),
            billed,
            "Customs-cleared duties processed"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn detail(merchant: Option<Uuid>, duty: i64) -> ShipmentMilestoneDetail {
        ShipmentMilestoneDetail {
            shipment_id: Uuid::new_v4(),
            merchant_id: merchant,
            duty_cents: duty,
            ..Default::default()
        }
    }

    #[test]
    fn billable_requires_merchant_and_positive_duty() {
        assert!(is_billable(&detail(Some(Uuid::new_v4()), 15_000)));
    }

    #[test]
    fn not_billable_without_merchant() {
        assert!(!is_billable(&detail(None, 15_000)));
    }

    #[test]
    fn not_billable_with_zero_duty() {
        assert!(!is_billable(&detail(Some(Uuid::new_v4()), 0)));
    }
}
